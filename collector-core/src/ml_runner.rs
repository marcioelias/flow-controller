use crate::alerts::{AlertEvent, AlertSeverity};
use crate::auth::AppState;
use crate::ml_model::{ExporterModel, WARMUP_SAMPLES};
use flow_types::FlowFeatures;
use flume::Receiver;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Re-alert the same IP at most every 15 min (task 17.10 R-05)
const ML_COOLDOWN_SECS: i64 = 900;
/// A minute closes this long after its end: flows arrive up to the
/// exporter's active timeout late (R-01)
const MINUTE_GRACE_SECS: u64 = 90;
/// Persistence: anomalous in at least this many of the last N scored minutes
const PERSIST_WINDOW: usize = 5;
const PERSIST_MIN: usize = 3;
/// Persistence history of an IP is dropped after this long without scores
const HISTORY_TTL_SECS: u64 = 1_800;

/// Snapshot of a single exporter's model state — cheap to clone, safe to publish.
#[derive(Clone, serde::Serialize)]
pub struct MlModelStatus {
    pub exporter_ip: String,
    pub status: &'static str, // "warming_up" | "active"
    pub samples_collected: usize,
    pub samples_needed: usize,
    pub n_scored: u64,
}

pub type SharedMlStatus = Arc<std::sync::RwLock<Vec<MlModelStatus>>>;

pub fn new_shared_status() -> SharedMlStatus {
    Arc::new(std::sync::RwLock::new(Vec::new()))
}

/// One IP's traffic in one minute, accumulated from per-second features
#[derive(Default)]
struct MinuteAcc {
    bytes: u64,
    packets: u64,
    flows: u64,
    upload: u64,
    download: u64,
    tcp: f64,
    udp: f64,
    icmp: f64,
    max_dst_ips: u32,
    max_dst_ports: u32,
}

impl MinuteAcc {
    fn add(&mut self, f: &FlowFeatures) {
        self.bytes += f.bytes_total;
        self.packets += f.packets_total;
        self.flows += f.flows_total;
        self.upload += f.upload_bytes;
        self.download += f.download_bytes;
        let b = f.bytes_total as f64;
        self.tcp += f.tcp_ratio * b;
        self.udp += f.udp_ratio * b;
        self.icmp += f.icmp_ratio * b;
        // Sets don't reach the runner: max per second, not a union (R-01)
        self.max_dst_ips = self.max_dst_ips.max(f.unique_dst_ips);
        self.max_dst_ports = self.max_dst_ports.max(f.unique_dst_ports);
    }

    fn into_features(self, exporter_ip: String, src_ip: String, minute: u64) -> FlowFeatures {
        let bytes = self.bytes.max(1) as f64;
        FlowFeatures {
            exporter_ip,
            src_ip,
            window_ts: minute as u32,
            bytes_total: self.bytes,
            packets_total: self.packets,
            flows_total: self.flows,
            avg_pkt_bytes: self.bytes as f64 / self.packets.max(1) as f64,
            pps: self.packets as f64 / 60.0,
            bps: self.bytes as f64 * 8.0 / 60.0,
            unique_dst_ips: self.max_dst_ips,
            unique_dst_ports: self.max_dst_ports,
            upload_bytes: self.upload,
            download_bytes: self.download,
            tcp_ratio: self.tcp / bytes,
            udp_ratio: self.udp / bytes,
            icmp_ratio: self.icmp / bytes,
        }
    }
}

/// Last scored minutes of an IP: true = above threshold
#[derive(Default)]
struct Persistence {
    flags: VecDeque<bool>,
    last_ts: u64,
}

impl Persistence {
    fn push(&mut self, anomalous: bool, ts: u64) -> usize {
        self.flags.push_back(anomalous);
        while self.flags.len() > PERSIST_WINDOW {
            self.flags.pop_front();
        }
        self.last_ts = ts;
        self.flags.iter().filter(|f| **f).count()
    }
}

/// Settings re-read periodically (task 15.5 + 17.10)
struct Tuning {
    min_pps: f64,
    min_bps: f64,
    min_samples: f64,
    alert_pct: f64,
}

impl Tuning {
    async fn load(state: &AppState) -> Self {
        let num = |v: Option<String>, default: f64| {
            v.and_then(|s| s.parse::<f64>().ok()).unwrap_or(default)
        };
        Self {
            min_pps: num(
                crate::settings::get_value(&state.db, "ML_MIN_PPS").await,
                100.0,
            ),
            min_bps: num(
                crate::settings::get_value(&state.db, "ML_MIN_BPS").await,
                1_000_000.0,
            ),
            min_samples: num(
                crate::settings::get_value(&state.db, "ML_MIN_SAMPLES").await,
                10.0,
            ),
            alert_pct: num(
                crate::settings::get_value(&state.db, "ML_ALERT_PERCENTILE").await,
                0.1,
            ),
        }
    }
}

/// Learned sampling rate per exporter (max over observation domains)
fn sampling_rates(state: &AppState) -> HashMap<String, f64> {
    let mut rates = HashMap::new();
    for family in state.metrics.registry.gather() {
        if family.get_name() != "exporter_sampling_rate" {
            continue;
        }
        for metric in family.get_metric() {
            let ip = metric
                .get_label()
                .iter()
                .find(|l| l.get_name() == "exporter_ip")
                .map(|l| l.get_value().to_string())
                .unwrap_or_default();
            let rate = metric.get_gauge().get_value().max(1.0);
            let e = rates.entry(ip).or_insert(1.0);
            if rate > *e {
                *e = rate;
            }
        }
    }
    rates
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn run_ml(
    state: Arc<AppState>,
    rx: Receiver<Vec<FlowFeatures>>,
    shared_status: SharedMlStatus,
) {
    tracing::info!(
        "ML anomaly detector started (1-min windows, warmup_samples={})",
        WARMUP_SAMPLES
    );
    let mut models: HashMap<String, ExporterModel> = HashMap::new();
    let mut open: HashMap<(String, String, u64), MinuteAcc> = HashMap::new();
    let mut history: HashMap<(String, String), Persistence> = HashMap::new();
    let mut closed_through: u64 = 0;
    let mut batch_count: u64 = 0;
    // Falsos positivos marcados pelo operador elevam o threshold daquele IP
    // (task 15.2): +0.05 por FP, teto de +0.20. Recarregado periodicamente.
    let mut fp_counts: HashMap<String, u32> = HashMap::new();
    let mut tuning = Tuning::load(&state).await;
    let mut rates = sampling_rates(&state);

    while let Ok(batch) = rx.recv_async().await {
        batch_count += 1;

        for feat in &batch {
            let minute = feat.window_ts as u64 / 60 * 60;
            if minute <= closed_through {
                continue; // too late: that minute was already scored
            }
            open.entry((feat.exporter_ip.clone(), feat.src_ip.clone(), minute))
                .or_default()
                .add(feat);
        }

        // Close every minute whose grace period has passed
        let now = unix_now();
        let cutoff = now.saturating_sub(60 + MINUTE_GRACE_SECS) / 60 * 60;
        if cutoff > closed_through {
            let ready: Vec<(String, String, u64)> = open
                .keys()
                .filter(|(_, _, m)| *m <= cutoff)
                .cloned()
                .collect();
            let mut closed: Vec<FlowFeatures> = ready
                .into_iter()
                .filter_map(|k| {
                    let acc = open.remove(&k)?;
                    Some(acc.into_features(k.0, k.1, k.2))
                })
                .collect();
            closed.sort_by_key(|f| f.window_ts);
            closed_through = cutoff;

            for feat in closed {
                score_minute(
                    &state,
                    &mut models,
                    &mut history,
                    &fp_counts,
                    &tuning,
                    &rates,
                    feat,
                    now,
                )
                .await;
            }
            history.retain(|_, p| now.saturating_sub(p.last_ts) < HISTORY_TTL_SECS);
        }

        // Publish status snapshot and reload tuning every 10 batches
        if batch_count.is_multiple_of(10) {
            if let Ok(rows) = sqlx::query_as::<_, (String, i64)>(
                "SELECT src_ip, COUNT(*) FROM alert_events \
                 WHERE alert_type = 'ml_anomaly' AND feedback = 'false_positive' \
                 GROUP BY src_ip",
            )
            .fetch_all(&state.db)
            .await
            {
                fp_counts = rows.into_iter().map(|(ip, n)| (ip, n as u32)).collect();
            }
            tuning = Tuning::load(&state).await;
            rates = sampling_rates(&state);

            let snapshot: Vec<MlModelStatus> = models
                .iter()
                .map(|(ip, m)| MlModelStatus {
                    exporter_ip: ip.clone(),
                    status: if m.is_ready() { "active" } else { "warming_up" },
                    samples_collected: m.samples_collected(),
                    samples_needed: WARMUP_SAMPLES,
                    n_scored: m.n_scored,
                })
                .collect();

            if let Ok(mut guard) = shared_status.write() {
                *guard = snapshot;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn score_minute(
    state: &AppState,
    models: &mut HashMap<String, ExporterModel>,
    history: &mut HashMap<(String, String), Persistence>,
    fp_counts: &HashMap<String, u32>,
    tuning: &Tuning,
    rates: &HashMap<String, f64>,
    feat: FlowFeatures,
    now: u64,
) {
    // Significance floors (R-02): real sampled packets first, then rates.
    // Scored and learned populations are the same, so the percentile holds.
    let rate = rates.get(&feat.exporter_ip).copied().unwrap_or(1.0);
    if (feat.packets_total as f64) / rate < tuning.min_samples {
        return;
    }
    if feat.pps < tuning.min_pps && feat.bps < tuning.min_bps {
        return;
    }

    let exporter = feat.exporter_ip.clone();
    let src_ip = feat.src_ip.clone();
    let vec = feat.to_vec();
    let m = models
        .entry(exporter.clone())
        .or_insert_with(ExporterModel::new);
    m.observe(vec.clone(), feat.window_ts as u64, &mut rand::thread_rng());

    if m.should_train(now) {
        let samples = m.samples_collected();
        tokio::task::block_in_place(|| m.train(now, tuning.alert_pct));
        tracing::debug!(
            "ML model retrained for {exporter} ({samples} samples, threshold={:.3})",
            m.threshold
        );
    }

    let Some(score) = m.score(&vec) else {
        return;
    };
    let threshold = m.threshold + 0.05 * fp_counts.get(&src_ip).copied().unwrap_or(0).min(4) as f64;
    let threshold_pct = m.threshold_pct;
    let anomalous = score >= threshold;
    let hits = history
        .entry((exporter.clone(), src_ip.clone()))
        .or_default()
        .push(anomalous, now);

    // Persistence (R-05): the current minute and 3 of the last 5
    if !anomalous || hits < PERSIST_MIN {
        return;
    }

    match already_ml_fired_recently(state, &src_ip, ML_COOLDOWN_SECS).await {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            tracing::warn!("ML cooldown check failed: {e}");
            return;
        }
    }

    let severity = if score >= threshold + 0.1 && hits == PERSIST_WINDOW {
        AlertSeverity::Critical
    } else {
        AlertSeverity::Warning
    };

    let ul_ratio = if feat.upload_bytes + feat.download_bytes > 0 {
        feat.upload_bytes as f64 / (feat.upload_bytes + feat.download_bytes) as f64
    } else {
        0.5
    };

    let message = format!(
        "ML anomaly: score={score:.3} (threshold={threshold:.2}, p{threshold_pct}) \
         {hits}/{PERSIST_WINDOW} min pps={:.0} unique_dst_ports={} ul_ratio={ul_ratio:.2}",
        feat.pps, feat.unique_dst_ports,
    );

    let event = AlertEvent {
        id: None,
        rule_id: None,
        exporter_ip: exporter.clone(),
        src_ip: src_ip.clone(),
        alert_type: "ml_anomaly".to_string(),
        severity,
        message,
        upload_bytes: Some(feat.upload_bytes as i64),
        download_bytes: Some(feat.download_bytes as i64),
        pps: Some(feat.pps),
        avg_pkt_bytes: Some(feat.avg_pkt_bytes),
        attack_ports: None,
        notified: false,
        bgp_announced: false,
        created_at: None,
    };

    match crate::alerts::insert_event(&state.db, &event).await {
        Ok(id) => tracing::info!("ML alert #{id}: {src_ip} on {exporter} (score={score:.3})"),
        Err(e) => tracing::warn!("ML alert insert failed: {e}"),
    }
}

async fn already_ml_fired_recently(
    state: &AppState,
    src_ip: &str,
    cooldown_secs: i64,
) -> anyhow::Result<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE alert_type = 'ml_anomaly'
           AND src_ip = ?
           AND created_at >= datetime('now', '-' || ? || ' seconds')",
    )
    .bind(src_ip)
    .bind(cooldown_secs)
    .fetch_one(&state.db)
    .await?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC-02 (task 17.10)
    #[test]
    fn persistence_needs_three_of_five() {
        let mut p = Persistence::default();
        assert_eq!(p.push(true, 0), 1);
        assert_eq!(p.push(false, 60), 1);
        assert_eq!(p.push(true, 120), 2);
        assert_eq!(p.push(true, 180), 3);
        for t in 0..5 {
            p.push(false, 240 + t * 60);
        }
        assert_eq!(p.flags.len(), PERSIST_WINDOW);
        assert_eq!(p.push(true, 600), 1);
    }

    #[test]
    fn minute_rates_are_averages_and_sets_use_the_max() {
        let sec = |bytes: u64, packets: u64, ips: u32| FlowFeatures {
            exporter_ip: "10.0.0.1".into(),
            src_ip: "100.64.0.1".into(),
            window_ts: 120,
            bytes_total: bytes,
            packets_total: packets,
            flows_total: 1,
            avg_pkt_bytes: 0.0,
            pps: 0.0,
            bps: 0.0,
            unique_dst_ips: ips,
            unique_dst_ports: ips,
            upload_bytes: bytes,
            download_bytes: 0,
            tcp_ratio: 1.0,
            udp_ratio: 0.0,
            icmp_ratio: 0.0,
        };
        let mut acc = MinuteAcc::default();
        acc.add(&sec(60_000, 60, 3));
        acc.add(&sec(60_000, 60, 7));
        let f = acc.into_features("10.0.0.1".into(), "100.64.0.1".into(), 120);
        assert_eq!(f.pps, 2.0);
        assert_eq!(f.bps, 16_000.0);
        assert_eq!(f.unique_dst_ips, 7);
        assert_eq!(f.tcp_ratio, 1.0);
    }
}
