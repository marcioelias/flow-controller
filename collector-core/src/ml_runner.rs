use crate::alerts::{AlertEvent, AlertSeverity};
use crate::auth::AppState;
use crate::ml_model::{ExporterModel, ANOMALY_THRESHOLD, WARMUP_SAMPLES};
use flow_types::FlowFeatures;
use flume::Receiver;
use std::collections::HashMap;
use std::sync::Arc;

const RETRAIN_EVERY:    u64 = 360;
const ML_COOLDOWN_SECS: i64 = 300;

/// Snapshot of a single exporter's model state — cheap to clone, safe to publish.
#[derive(Clone, serde::Serialize)]
pub struct MlModelStatus {
    pub exporter_ip:       String,
    pub status:            &'static str, // "warming_up" | "active"
    pub samples_collected: usize,
    pub samples_needed:    usize,
    pub n_scored:          u64,
}

pub type SharedMlStatus = Arc<std::sync::RwLock<Vec<MlModelStatus>>>;

pub fn new_shared_status() -> SharedMlStatus {
    Arc::new(std::sync::RwLock::new(Vec::new()))
}

pub async fn run_ml(
    state: Arc<AppState>,
    rx: Receiver<Vec<FlowFeatures>>,
    shared_status: SharedMlStatus,
) {
    tracing::info!("ML anomaly detector started (warmup_samples={})", WARMUP_SAMPLES);
    let mut models: HashMap<String, ExporterModel> = HashMap::new();
    let mut batch_count: u64 = 0;

    while let Ok(batch) = rx.recv_async().await {
        batch_count += 1;

        for feat in batch {
            let vec      = feat.to_vec();
            let exporter = feat.exporter_ip.clone();
            let src_ip   = feat.src_ip.clone();

            let m = models.entry(exporter.clone()).or_insert_with(ExporterModel::new);
            let warm = m.push(vec.clone());

            if !warm {
                continue;
            }

            if m.n_scored % RETRAIN_EVERY == 0 {
                let samples = m.buffer.len();
                if let Err(e) = tokio::task::block_in_place(|| m.train()) {
                    tracing::warn!("ML retrain failed for {exporter}: {e}");
                    continue;
                }
                tracing::debug!("ML model retrained for {exporter} ({samples} samples)");
            }

            let score = match m.score(&vec) {
                Some(s) => s,
                None => continue,
            };

            if score < ANOMALY_THRESHOLD {
                continue;
            }

            match already_ml_fired_recently(&state, &src_ip, ML_COOLDOWN_SECS).await {
                Ok(true)  => continue,
                Ok(false) => {}
                Err(e)    => {
                    tracing::warn!("ML cooldown check failed: {e}");
                    continue;
                }
            }

            let severity = if score >= ANOMALY_THRESHOLD + 0.15 {
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
                "ML anomaly: score={:.3} (threshold={:.2}) pps={:.0} unique_dst_ports={} ul_ratio={:.2}",
                score, ANOMALY_THRESHOLD, feat.pps, feat.unique_dst_ports, ul_ratio,
            );

            let event = AlertEvent {
                id:            None,
                rule_id:       None,
                exporter_ip:   exporter.clone(),
                src_ip:        src_ip.clone(),
                alert_type:    "ml_anomaly".to_string(),
                severity,
                message,
                upload_bytes:  Some(feat.upload_bytes as i64),
                download_bytes: Some(feat.download_bytes as i64),
                pps:           Some(feat.pps),
                avg_pkt_bytes: Some(feat.avg_pkt_bytes),
                attack_ports:  None,
                notified:      false,
                bgp_announced: false,
                created_at:    None,
            };

            match crate::alerts::insert_event(&state.db, &event).await {
                Ok(id) => tracing::info!("ML alert #{id}: {src_ip} on {exporter} (score={score:.3})"),
                Err(e) => tracing::warn!("ML alert insert failed: {e}"),
            }
        }

        // Publish status snapshot every 10 batches
        if batch_count % 10 == 0 {
            let snapshot: Vec<MlModelStatus> = models
                .iter()
                .map(|(ip, m)| MlModelStatus {
                    exporter_ip:       ip.clone(),
                    status:            if m.is_ready() { "active" } else { "warming_up" },
                    samples_collected: m.samples_collected(),
                    samples_needed:    WARMUP_SAMPLES,
                    n_scored:          m.n_scored,
                })
                .collect();

            if let Ok(mut guard) = shared_status.write() {
                *guard = snapshot;
            }
        }
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
