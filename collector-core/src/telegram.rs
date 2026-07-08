use std::sync::Arc;

use crate::alerts::{AlertEvent, AlertSeverity, TelegramConfig};
use crate::auth::AppState;

const POLL_INTERVAL_SECS: u64 = 30;
const BATCH_SIZE: i64 = 10;

pub async fn run_notifier(state: Arc<AppState>) {
    tracing::info!("Telegram notifier started");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let cfg = match load_telegram_config(&state.db).await {
            Ok(c) if c.enabled && !c.bot_token.is_empty() => c,
            _ => continue,
        };

        let events = match fetch_unnotified(&state.db, &cfg, BATCH_SIZE).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to fetch unnotified events: {e}");
                continue;
            }
        };

        for event in &events {
            let text = format_message(event);
            match send_message(&cfg, &text).await {
                Ok(()) => {
                    let event_id = event.id.unwrap_or(0);
                    if let Err(e) = mark_notified(&state.db, event_id).await {
                        tracing::warn!("Failed to mark event {event_id} as notified: {e}");
                    }
                    // Respect Telegram's 1 message/second per chat limit
                    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
                }
                Err(e) => {
                    tracing::warn!("Telegram send failed: {e}");
                    break; // stop batch; retry next poll
                }
            }
        }
    }
}

pub async fn load_telegram_config(pool: &sqlx::SqlitePool) -> anyhow::Result<TelegramConfig> {
    let row = sqlx::query(
        "SELECT bot_token, chat_id, enabled, min_severity FROM telegram_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    use sqlx::Row;
    let min_sev: String = row.try_get("min_severity")?;
    let severity = if min_sev == "critical" {
        AlertSeverity::Critical
    } else {
        AlertSeverity::Warning
    };

    Ok(TelegramConfig {
        bot_token: row.try_get("bot_token")?,
        chat_id: row.try_get("chat_id")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        min_severity: severity,
    })
}

async fn fetch_unnotified(
    pool: &sqlx::SqlitePool,
    cfg: &TelegramConfig,
    limit: i64,
) -> anyhow::Result<Vec<AlertEvent>> {
    let severities: &[&str] = match cfg.min_severity {
        AlertSeverity::Critical => &["critical"],
        AlertSeverity::Warning => &["warning", "critical"],
    };

    // Build dynamic IN clause
    let placeholders: String = severities.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query_str = format!(
        "SELECT id, rule_id, exporter_ip, src_ip, alert_type, severity, message,
                upload_bytes, download_bytes, pps, avg_pkt_bytes, attack_ports,
                notified, created_at
         FROM alert_events
         WHERE notified = 0 AND severity IN ({placeholders})
         ORDER BY created_at ASC LIMIT ?"
    );

    let mut q = sqlx::query(&query_str);
    for s in severities {
        q = q.bind(*s);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(pool).await?;

    rows.into_iter()
        .map(|r| {
            use sqlx::Row;
            Ok(AlertEvent {
                id: r.try_get("id")?,
                rule_id: r.try_get("rule_id")?,
                exporter_ip: r.try_get("exporter_ip")?,
                src_ip: r.try_get("src_ip")?,
                alert_type: r.try_get("alert_type")?,
                severity: {
                    let s: &str = r.try_get("severity")?;
                    if s == "critical" { AlertSeverity::Critical } else { AlertSeverity::Warning }
                },
                message: r.try_get("message")?,
                upload_bytes: r.try_get("upload_bytes")?,
                download_bytes: r.try_get("download_bytes")?,
                pps: r.try_get("pps")?,
                avg_pkt_bytes: r.try_get("avg_pkt_bytes")?,
                attack_ports: r.try_get("attack_ports")?,
                notified: {
                    let n: i64 = r.try_get("notified")?;
                    n != 0
                },
                bgp_announced: {
                    let n: i64 = r.try_get("bgp_announced").unwrap_or(0);
                    n != 0
                },
                created_at: r.try_get("created_at")?,
            })
        })
        .collect()
}

async fn mark_notified(pool: &sqlx::SqlitePool, event_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE alert_events SET notified = 1 WHERE id = ?")
        .bind(event_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn send_message(cfg: &TelegramConfig, text: &str) -> anyhow::Result<()> {
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        cfg.bot_token
    );
    let body = serde_json::json!({
        "chat_id": cfg.chat_id,
        "text": text,
        "parse_mode": "HTML"
    });

    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(anyhow::anyhow!("Telegram API error: {status} — {text}"))
    }
}

fn format_message(event: &AlertEvent) -> String {
    let icon = match event.severity {
        AlertSeverity::Critical => "🔴",
        AlertSeverity::Warning => "🟡",
    };

    let severity_label = match event.severity {
        AlertSeverity::Critical => "CRITICAL",
        AlertSeverity::Warning => "WARNING",
    };

    let ip_line = format!("\n<b>IP:</b> <code>{}</code>", event.src_ip);

    let detail_line = match event.alert_type.as_str() {
        "upload_inversion" => {
            match (event.upload_bytes, event.download_bytes) {
                (Some(ul), Some(dl)) if dl > 0 => {
                    let window_sec = 600u64; // assume 10 min default window
                    let ul_mbps = ul as u64 / window_sec / 125_000;
                    let dl_mbps = dl as u64 / window_sec / 125_000;
                    let ratio = ul as f64 / dl as f64;
                    format!(
                        "\n<b>Upload:</b> {} Mbps  |  <b>Download:</b> {} Mbps  |  <b>Ratio:</b> {:.1}×",
                        ul_mbps, dl_mbps, ratio
                    )
                }
                _ => String::new(),
            }
        }
        "attack_signature" => {
            let mut parts = Vec::new();
            if let Some(pps) = event.pps {
                parts.push(format!("<b>PPS:</b> {:.0}", pps));
            }
            if let Some(avg) = event.avg_pkt_bytes {
                parts.push(format!("<b>Avg pkt:</b> {:.0} bytes", avg));
            }
            if let Some(ports) = &event.attack_ports {
                parts.push(format!("<b>Ports:</b> [{}]", ports));
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!("\n{}", parts.join("  |  "))
            }
        }
        _ => String::new(),
    };

    format!(
        "{icon} <b>[{severity_label}] {alert_type}</b>\n\
         <b>Exporter:</b> <code>{exporter}</code>{ip_line}\n\
         {message}{detail_line}",
        icon = icon,
        severity_label = severity_label,
        alert_type = event.alert_type,
        exporter = event.exporter_ip,
        ip_line = ip_line,
        message = event.message,
        detail_line = detail_line,
    )
}
