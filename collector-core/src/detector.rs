use std::sync::Arc;

use crate::alerts::{self, AlertEvent, AlertRule, AlertSeverity, DEFAULT_ATTACK_PORTS};
use crate::auth::AppState;
use crate::bgp_control::{self, AnnounceRequest};

const EVAL_INTERVAL_SECS: u64 = 60;

pub async fn run_detector(state: Arc<AppState>) {
    tracing::info!("Alert detector started");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(EVAL_INTERVAL_SECS)).await;

        let rules = match alerts::load_enabled_rules(&state.db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("load_enabled_rules: {e}");
                continue;
            }
        };

        for rule in rules {
            if let Err(e) = evaluate_rule(&state, &rule).await {
                tracing::warn!("Rule '{}' eval error: {e}", rule.name);
            }
        }

        if let Err(e) = eval_routing_loops(&state).await {
            tracing::warn!("Routing loop check error: {e}");
        }
    }
}

async fn evaluate_rule(state: &AppState, rule: &AlertRule) -> anyhow::Result<()> {
    // Resolve exporter IP(s) for this rule
    let exporter_ips = if let Some(exp_id) = rule.exporter_id {
        let ip: Option<String> =
            sqlx::query_scalar("SELECT ip_address FROM exporters WHERE id = ?")
                .bind(exp_id)
                .fetch_optional(&state.db)
                .await?;
        match ip {
            Some(ip) => vec![ip],
            None => return Ok(()), // exporter deleted
        }
    } else {
        // Apply to all enabled exporters
        sqlx::query_scalar("SELECT ip_address FROM exporters WHERE enabled = 1")
            .fetch_all(&state.db)
            .await?
    };

    for exporter_ip in exporter_ips {
        match rule.rule_type.as_str() {
            "upload_inversion" => {
                if let Err(e) = eval_upload_inversion(state, rule, &exporter_ip).await {
                    tracing::warn!("upload_inversion eval for {exporter_ip}: {e}");
                }
            }
            "attack_signature" => {
                if let Err(e) = eval_attack_signature(state, rule, &exporter_ip).await {
                    tracing::warn!("attack_signature eval for {exporter_ip}: {e}");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ClickHouse query helper
// ---------------------------------------------------------------------------

async fn ch_query<T: serde::de::DeserializeOwned>(url: &str, sql: &str) -> anyhow::Result<Vec<T>> {
    let resp = reqwest::Client::new()
        .post(url)
        .query(&[("default_format", "JSON")])
        .body(sql.to_string())
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let rows = resp["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("no data array in CH response"))?;

    rows.iter()
        .map(|v| serde_json::from_value(v.clone()).map_err(Into::into))
        .collect()
}

// ---------------------------------------------------------------------------
// Upload inversion rule
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct UploadRow {
    src_ip: String,
    upload_bytes: u64,
}

#[derive(serde::Deserialize)]
struct DownloadRow {
    src_ip: String,
    download_bytes: u64,
}

#[derive(serde::Deserialize)]
struct HistRow {
    src_ip: String,
    #[allow(dead_code)]
    hist_upload: u64,
    #[allow(dead_code)]
    hist_download: u64,
    hist_dl_ul_ratio: f64,
}

async fn eval_upload_inversion(
    state: &AppState,
    rule: &AlertRule,
    exporter_ip: &str,
) -> anyhow::Result<()> {
    let p = &rule.params;
    let short_min = p["short_window_min"].as_u64().unwrap_or(10) as u32;
    let history_min = p["history_window_min"].as_u64().unwrap_or(1440) as u32;
    let history_h = history_min.div_ceil(60);
    let inversion_ratio = p["inversion_ratio"].as_f64().unwrap_or(2.0);
    let min_hist_dl_ratio = p["min_hist_download_ratio"].as_f64().unwrap_or(3.0);
    let min_upload_mbps = p["min_upload_mbps"].as_u64().unwrap_or(5);
    let cooldown_min = p["cooldown_min"].as_i64().unwrap_or(30);

    let min_upload_bytes = min_upload_mbps * 125_000 * (short_min as u64) * 60;

    // Step 1: find IPs with significant upload in the short window
    let upload_sql = format!(
        "SELECT src_ip, sum(bytes) AS upload_bytes
         FROM network_flows_v4
         WHERE timestamp >= now() - INTERVAL {short_min} MINUTE
           AND exporter_ip = '{exporter_ip}'
         GROUP BY src_ip
         HAVING upload_bytes >= {min_upload_bytes}
         FORMAT JSON"
    );

    let upload_rows: Vec<UploadRow> = ch_query(&state.clickhouse_url, &upload_sql).await?;
    if upload_rows.is_empty() {
        return Ok(());
    }

    let ip_list: Vec<String> = upload_rows
        .iter()
        .map(|r| format!("'{}'", r.src_ip))
        .collect();
    let ip_csv = ip_list.join(",");

    // Step 2: download bytes for those IPs in the same short window
    let download_sql = format!(
        "SELECT dst_ip AS src_ip, sum(bytes) AS download_bytes
         FROM network_flows_v4
         WHERE timestamp >= now() - INTERVAL {short_min} MINUTE
           AND exporter_ip = '{exporter_ip}'
           AND dst_ip IN ({ip_csv})
         GROUP BY dst_ip
         FORMAT JSON"
    );

    let download_rows: Vec<DownloadRow> = ch_query(&state.clickhouse_url, &download_sql).await?;
    let download_map: std::collections::HashMap<String, u64> = download_rows
        .into_iter()
        .map(|r| (r.src_ip, r.download_bytes))
        .collect();

    // Step 3: historical ratio from materialized view
    let hist_sql = format!(
        "SELECT tx.src_ip,
                sum(tx.bytes) AS hist_upload,
                sum(rx.bytes) AS hist_download,
                sum(rx.bytes) / greatest(sum(tx.bytes), 1) AS hist_dl_ul_ratio
         FROM ip_hourly_tx_v4 tx
         JOIN ip_hourly_rx_v4 rx
           ON rx.src_ip = tx.src_ip
          AND rx.hour = tx.hour
          AND rx.exporter_ip = tx.exporter_ip
         WHERE tx.hour >= toStartOfHour(now()) - INTERVAL {history_h} HOUR
           AND tx.exporter_ip = '{exporter_ip}'
           AND tx.src_ip IN ({ip_csv})
         GROUP BY tx.src_ip
         HAVING hist_dl_ul_ratio >= {min_hist_dl_ratio}
         FORMAT JSON"
    );

    let hist_rows: Vec<HistRow> = ch_query(&state.clickhouse_url, &hist_sql).await?;
    let hist_map: std::collections::HashMap<String, HistRow> = hist_rows
        .into_iter()
        .map(|r| (r.src_ip.clone(), r))
        .collect();

    for up in &upload_rows {
        let current_download = download_map.get(&up.src_ip).copied().unwrap_or(0);
        let current_upload = up.upload_bytes;

        // Must exceed inversion threshold
        if current_upload <= (current_download as f64 * inversion_ratio) as u64 {
            continue;
        }

        // Must have historically been download-heavy
        let hist = match hist_map.get(&up.src_ip) {
            Some(h) => h,
            None => continue,
        };

        let rule_id = rule.id.unwrap_or(0);
        if alerts::already_fired_recently(&state.db, rule_id, &up.src_ip, cooldown_min).await? {
            continue;
        }

        // Critical if ratio > 2× the threshold
        let current_ratio = current_upload as f64 / current_download.max(1) as f64;
        let severity = if current_ratio > inversion_ratio * 2.0 {
            AlertSeverity::Critical
        } else {
            AlertSeverity::Warning
        };

        let ul_mbps = current_upload / (short_min as u64 * 60 * 125_000);
        let dl_mbps = current_download / (short_min as u64 * 60 * 125_000);
        let message = format!(
            "Upload inversion: {} Mbps up vs {} Mbps down (ratio {:.1}×, hist baseline {:.1}× DL/UL)",
            ul_mbps, dl_mbps, current_ratio, hist.hist_dl_ul_ratio
        );

        let event = AlertEvent {
            id: None,
            rule_id: rule.id,
            exporter_ip: exporter_ip.to_string(),
            src_ip: up.src_ip.clone(),
            alert_type: "upload_inversion".to_string(),
            severity,
            message,
            upload_bytes: Some(current_upload as i64),
            download_bytes: Some(current_download as i64),
            pps: None,
            avg_pkt_bytes: None,
            attack_ports: None,
            notified: false,
            bgp_announced: false,
            created_at: None,
        };

        match alerts::insert_event(&state.db, &event).await {
            Ok(id) => tracing::info!(
                "Alert #{id}: upload_inversion {} on {exporter_ip}",
                up.src_ip
            ),
            Err(e) => tracing::warn!("Failed to insert alert event: {e}"),
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Attack signature rule
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct AttackRow {
    src_ip: String,
    #[allow(dead_code)]
    total_packets: u64,
    pps: f64,
    avg_pkt_bytes: f64,
    ports_seen: Vec<u16>,
}

async fn eval_attack_signature(
    state: &AppState,
    rule: &AlertRule,
    exporter_ip: &str,
) -> anyhow::Result<()> {
    let p = &rule.params;
    let window_min = p["window_min"].as_u64().unwrap_or(2) as u32;
    let min_pps = p["min_pps"].as_f64().unwrap_or(1000.0);
    let max_avg_pkt = p["max_avg_pkt_bytes"].as_f64().unwrap_or(200.0);
    let min_total_pkts = p["min_total_packets"].as_u64().unwrap_or(10_000);
    let cooldown_min = p["cooldown_min"].as_i64().unwrap_or(5);

    let attack_ports: Vec<u16> = match p["attack_ports"].as_array() {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u16))
            .collect(),
        _ => DEFAULT_ATTACK_PORTS.to_vec(),
    };

    let sql = format!(
        "SELECT src_ip,
                sum(packets) AS total_packets,
                sum(packets) / ({window_min} * 60.0) AS pps,
                sum(bytes) / greatest(sum(packets), 1) AS avg_pkt_bytes,
                groupArray(dst_port) AS ports_seen
         FROM network_flows_v4
         WHERE timestamp >= now() - INTERVAL {window_min} MINUTE
           AND exporter_ip = '{exporter_ip}'
         GROUP BY src_ip
         HAVING total_packets >= {min_total_pkts}
            AND pps >= {min_pps}
            AND avg_pkt_bytes <= {max_avg_pkt}
         FORMAT JSON"
    );

    let rows: Vec<AttackRow> = ch_query(&state.clickhouse_url, &sql).await?;

    for row in &rows {
        // Port filter in Rust
        let matched_ports: Vec<u16> = row
            .ports_seen
            .iter()
            .filter(|p| attack_ports.contains(p))
            .copied()
            .collect();

        if matched_ports.is_empty() {
            continue;
        }

        let rule_id = rule.id.unwrap_or(0);
        if alerts::already_fired_recently(&state.db, rule_id, &row.src_ip, cooldown_min).await? {
            continue;
        }

        let severity = if row.pps >= min_pps * 5.0 {
            AlertSeverity::Critical
        } else {
            AlertSeverity::Warning
        };

        let ports_str: Vec<String> = matched_ports.iter().map(|p| p.to_string()).collect();
        let ports_display = ports_str.join(",");

        let message = format!(
            "Attack signature: {:.0} PPS, {:.0} avg bytes/pkt, ports [{}]",
            row.pps, row.avg_pkt_bytes, ports_display
        );

        let event = AlertEvent {
            id: None,
            rule_id: rule.id,
            exporter_ip: exporter_ip.to_string(),
            src_ip: row.src_ip.clone(),
            alert_type: "attack_signature".to_string(),
            severity,
            message,
            upload_bytes: None,
            download_bytes: None,
            pps: Some(row.pps),
            avg_pkt_bytes: Some(row.avg_pkt_bytes),
            attack_ports: Some(ports_display),
            notified: false,
            bgp_announced: false,
            created_at: None,
        };

        let event_id = match alerts::insert_event(&state.db, &event).await {
            Ok(id) => {
                tracing::info!(
                    "Alert #{id}: attack_signature {} on {exporter_ip}",
                    row.src_ip
                );
                id
            }
            Err(e) => {
                tracing::warn!("Failed to insert alert event: {e}");
                continue;
            }
        };

        // Auto BGP blackhole
        let auto_bgp = p["auto_bgp_announce"].as_bool().unwrap_or(false);
        if auto_bgp {
            auto_bgp_announce(state, &row.src_ip, event_id, p).await;
        }
    }

    Ok(())
}

async fn auto_bgp_announce(
    state: &AppState,
    src_ip: &str,
    event_id: i64,
    params: &serde_json::Value,
) {
    let community_id = params["bgp_community_id"].as_i64();
    let withdraw_after_min = params["bgp_withdraw_after_min"].as_u64().unwrap_or(60);
    let prefix = format!("{}/32", src_ip);
    let origin_detail = format!("alert_event#{}", event_id);

    let req = AnnounceRequest {
        prefix: prefix.clone(),
        next_hop: "self".to_string(),
        community_id,
        peer_id: None,
        origin: "anomaly_detector".to_string(),
        origin_detail: Some(origin_detail),
    };

    match bgp_control::announce(&state.db, &state.exabgp_pipe, req).await {
        Ok((ann_id, cmd)) => {
            tracing::info!("Auto BGP blackhole for {src_ip}: {cmd}");
            if let Err(e) = alerts::mark_bgp_announced(&state.db, event_id).await {
                tracing::warn!("mark_bgp_announced failed: {e}");
            }
            if withdraw_after_min > 0 {
                let pool = state.db.clone();
                let pipe = state.exabgp_pipe.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(withdraw_after_min * 60))
                        .await;
                    match bgp_control::withdraw(&pool, &pipe, ann_id).await {
                        Ok(_) => tracing::info!("Auto BGP withdraw for announcement #{ann_id}"),
                        Err(e) => tracing::warn!("Auto BGP withdraw failed: {e}"),
                    }
                });
            }
        }
        Err(e) => tracing::warn!("Auto BGP announce for {src_ip} failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Routing loop (ping-pong) detector — task 17.15
// ---------------------------------------------------------------------------

/// Consecutive suspicious minutes needed to alert (R-02)
const LOOP_MINUTES: usize = 5;
const LOOP_COOLDOWN_MIN: i64 = 60;

/// Rows of a raw ClickHouse JSON query (UInt64 comes quoted)
async fn ch_rows(url: &str, sql: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let resp = reqwest::Client::new()
        .post(url)
        .query(&[("default_format", "JSON")])
        .body(sql.to_string())
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(resp["data"].as_array().cloned().unwrap_or_default())
}

fn num(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// SQL for the two loop signals per exporter, destination and minute (R-01):
/// traffic that leaves through the interface it came in, and ICMP TTL exceeded
fn loop_signal_sql(table: &str) -> String {
    format!(
        "SELECT toString(exporter_ip) AS exporter, toString(dst_ip) AS dst, \
                toUnixTimestamp(toStartOfMinute(timestamp)) AS minute, \
                sumIf(packets, in_if = out_if AND in_if != 0) AS hairpin, \
                sumIf(packets, (protocol = 1 AND intDiv(dst_port, 256) = 11) \
                            OR (protocol = 58 AND intDiv(dst_port, 256) = 3)) AS ttl_exceeded \
         FROM {table} \
         WHERE timestamp >= toStartOfMinute(now()) - INTERVAL {LOOP_MINUTES} MINUTE \
           AND timestamp < toStartOfMinute(now()) \
         GROUP BY exporter, dst, minute \
         HAVING hairpin > 0 OR ttl_exceeded > 0"
    )
}

/// (exporter, destination) pairs suspicious in every one of the last minutes
fn persistent_loops(
    rows: &[serde_json::Value],
    rates: &std::collections::HashMap<String, f64>,
    min_samples: f64,
) -> Vec<(String, String)> {
    let mut minutes: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for r in rows {
        let exporter = r["exporter"].as_str().unwrap_or_default().to_string();
        let rate = rates.get(&exporter).copied().unwrap_or(1.0);
        let sampled = num(&r["hairpin"]).max(num(&r["ttl_exceeded"])) / rate;
        if sampled >= min_samples {
            let dst = r["dst"].as_str().unwrap_or_default().to_string();
            *minutes.entry((exporter, dst)).or_default() += 1;
        }
    }
    let mut out: Vec<_> = minutes
        .into_iter()
        .filter(|(_, n)| *n >= LOOP_MINUTES)
        .map(|(k, _)| k)
        .collect();
    out.sort();
    out
}

async fn loop_fired_recently(state: &AppState, exporter: &str, dst: &str) -> anyhow::Result<bool> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_events
         WHERE alert_type = 'routing_loop' AND exporter_ip = ? AND src_ip = ?
           AND created_at >= datetime('now', '-' || ? || ' minutes')",
    )
    .bind(exporter)
    .bind(dst)
    .bind(LOOP_COOLDOWN_MIN)
    .fetch_one(&state.db)
    .await?;
    Ok(n > 0)
}

async fn eval_routing_loops(state: &AppState) -> anyhow::Result<()> {
    let setting = |k: &'static str| crate::settings::get_value(&state.db, k);
    if setting("LOOP_DETECTOR_ENABLED").await.as_deref() == Some("false") {
        return Ok(());
    }
    let min_samples: f64 = setting("LOOP_MIN_SAMPLES")
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(100.0);

    let url = &state.clickhouse_url;
    let mut rows = ch_rows(url, &loop_signal_sql("network_flows_v4")).await?;
    rows.extend(ch_rows(url, &loop_signal_sql("network_flows_v6")).await?);
    let rates = crate::ml_runner::sampling_rates(state);

    for (exporter, dst) in persistent_loops(&rows, &rates, min_samples) {
        if loop_fired_recently(state, &exporter, &dst).await? {
            continue;
        }
        let Ok(dst_ip) = dst.parse::<std::net::IpAddr>() else {
            continue;
        };
        let Ok(exp_ip) = exporter.parse::<std::net::IpAddr>() else {
            continue;
        };
        let (table, dst_lit, zero) = match dst_ip {
            std::net::IpAddr::V4(a) => (
                "network_flows_v4",
                format!("toIPv4('{a}')"),
                "toIPv4('0.0.0.0')",
            ),
            std::net::IpAddr::V6(a) => {
                ("network_flows_v6", format!("toIPv6('{a}')"), "toIPv6('::')")
            }
        };
        let window = format!(
            "timestamp >= toStartOfMinute(now()) - INTERVAL {LOOP_MINUTES} MINUTE \
             AND timestamp < toStartOfMinute(now())"
        );
        let detail_sql = format!(
            "SELECT groupUniqArrayIf(4)(concat(toString(in_if), '→', toString(out_if)), in_if != 0) AS ifaces, \
                    toString(anyIf(next_hop, next_hop != {zero})) AS next_hop, \
                    topK(3)(toString(src_ip)) AS sources, \
                    sum(packets) AS pkts, sum(bytes) AS bytes, \
                    sumIf(packets, (protocol = 1 AND intDiv(dst_port, 256) = 11) \
                                OR (protocol = 58 AND intDiv(dst_port, 256) = 3)) AS ttl_pkts, \
                    (SELECT sum(bytes) FROM {table} WHERE exporter_ip = '{exp_ip}' AND fwd_status < 128 AND {window}) AS exporter_bytes \
             FROM {table} \
             WHERE exporter_ip = '{exp_ip}' AND dst_ip = {dst_lit} AND {window} \
               AND ((in_if = out_if AND in_if != 0) \
                    OR (protocol = 1 AND intDiv(dst_port, 256) = 11) \
                    OR (protocol = 58 AND intDiv(dst_port, 256) = 3))"
        );
        let Some(d) = ch_rows(url, &detail_sql).await?.into_iter().next() else {
            continue;
        };
        let list = |v: &serde_json::Value| {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        };
        let secs = (LOOP_MINUTES * 60) as f64;
        let pps = num(&d["pkts"]) / secs;
        let share = if num(&d["exporter_bytes"]) > 0.0 {
            num(&d["bytes"]) / num(&d["exporter_bytes"]) * 100.0
        } else {
            0.0
        };
        let ifaces = list(&d["ifaces"]);
        let next_hop = d["next_hop"].as_str().unwrap_or_default();
        let ttl_pct = if num(&d["pkts"]) > 0.0 {
            num(&d["ttl_pkts"]) / num(&d["pkts"]) * 100.0
        } else {
            0.0
        };
        let message = format!(
            "Possível loop de roteamento para {dst} há {LOOP_MINUTES} min: ~{pps:.0} pps \
             ({share:.1}% do tráfego do exportador), {ttl_pct:.0}% ICMP TTL expirado; \
             interfaces {}{}; origens mais frequentes: {}",
            if ifaces.is_empty() {
                "—".to_string()
            } else {
                ifaces
            },
            if next_hop.is_empty() || next_hop == "0.0.0.0" || next_hop == "::" {
                String::new()
            } else {
                format!(" via {next_hop}")
            },
            list(&d["sources"]),
        );
        let event = AlertEvent {
            id: None,
            rule_id: None,
            exporter_ip: exporter.clone(),
            src_ip: dst.clone(),
            alert_type: "routing_loop".to_string(),
            severity: if share >= 10.0 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            },
            message,
            upload_bytes: None,
            download_bytes: None,
            pps: Some(pps),
            avg_pkt_bytes: Some(num(&d["bytes"]) / num(&d["pkts"]).max(1.0)),
            attack_ports: None,
            notified: false,
            bgp_announced: false,
            created_at: None,
        };
        match alerts::insert_event(&state.db, &event).await {
            Ok(id) => tracing::warn!("Routing loop alert #{id}: {dst} on {exporter}"),
            Err(e) => tracing::warn!("Routing loop alert insert failed: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod loop_tests {
    use super::*;

    fn row(exporter: &str, dst: &str, minute: u64, hairpin: u64, ttl: u64) -> serde_json::Value {
        serde_json::json!({
            "exporter": exporter, "dst": dst, "minute": minute,
            "hairpin": hairpin.to_string(), "ttl_exceeded": ttl.to_string(),
        })
    }

    // AC-01 / AC-02 (task 17.15)
    #[test]
    fn needs_every_minute_above_the_floor() {
        let rates: std::collections::HashMap<String, f64> =
            [("10.0.0.1".to_string(), 1000.0)].into_iter().collect();
        let mut rows: Vec<_> = (0..5)
            .map(|m| row("10.0.0.1", "170.231.6.0", m * 60, 150_000, 0))
            .collect();
        // 4 good minutes + 1 below the floor: not persistent
        rows.extend((0..4).map(|m| row("10.0.0.1", "170.231.7.0", m * 60, 0, 150_000)));
        rows.push(row("10.0.0.1", "170.231.7.0", 240, 0, 50_000));
        assert_eq!(
            persistent_loops(&rows, &rates, 100.0),
            vec![("10.0.0.1".to_string(), "170.231.6.0".to_string())]
        );
    }
}
