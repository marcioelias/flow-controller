# Task 6.3 — Telegram Notifier

## Goal

Send Telegram messages when new unnotified `alert_events` are found in SQLite.
This is a separate background task that polls for unnotified events every 30 seconds,
batches them, and marks them as notified after a successful send.

Keeping it decoupled from the detector (task 6.2) means Telegram outages never block
detection, and the queue is durable across restarts.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/telegram.rs` | CREATE |
| `collector-core/src/main.rs` | MODIFY — spawn notifier task |

---

## Telegram API

Uses the Bot HTTP API. No external crate needed — `reqwest` is already in scope.

```
POST https://api.telegram.org/bot{token}/sendMessage
Content-Type: application/json

{ "chat_id": "{chat_id}", "text": "...", "parse_mode": "HTML" }
```

---

## `telegram.rs` structure

```rust
use std::sync::Arc;
use crate::auth::AppState;
use crate::alerts::{AlertEvent, AlertSeverity, TelegramConfig};

const POLL_INTERVAL_SECS: u64 = 30;
const BATCH_SIZE: i64 = 10;  // max events per poll cycle

pub async fn run_notifier(state: Arc<AppState>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let cfg = match load_telegram_config(&state.db).await {
            Ok(c) if c.enabled && !c.bot_token.is_empty() => c,
            _ => continue,  // not configured or disabled — skip silently
        };

        let events = match fetch_unnotified(&state.db, &cfg, BATCH_SIZE).await {
            Ok(e) => e,
            Err(e) => { tracing::warn!("Failed to fetch unnotified events: {e}"); continue; }
        };

        for event in &events {
            let text = format_message(event);
            match send_message(&cfg, &text).await {
                Ok(()) => {
                    if let Err(e) = mark_notified(&state.db, event.id.unwrap()).await {
                        tracing::warn!("Failed to mark event {} as notified: {e}", event.id.unwrap());
                    }
                }
                Err(e) => {
                    // Don't mark as notified — will retry next poll
                    tracing::warn!("Telegram send failed: {e}");
                    break;  // stop processing batch; avoid cascading failures
                }
            }
        }
    }
}
```

---

## Message format

```rust
fn format_message(event: &AlertEvent) -> String {
    let icon = match event.severity {
        AlertSeverity::Critical => "🔴",
        AlertSeverity::Warning  => "🟡",
        AlertSeverity::Info     => "🔵",
    };

    let ip_line = match &event.src_ip {
        Some(ip) => format!("\n<b>IP:</b> <code>{}</code>", ip),
        None => String::new(),
    };

    let rate_line = match (event.bytes_short, event.bytes_long) {
        (Some(s), Some(l)) if l > 0 => format!(
            "\n<b>Taxa atual:</b> {} Mbps  |  <b>Base:</b> {} Mbps  |  <b>Ratio:</b> {:.1}×",
            s / 60 / 125_000,
            l / 60 / 125_000,
            s as f64 / l as f64
        ),
        (Some(s), None) => format!("\n<b>Taxa:</b> {} Mbps", s / 60 / 125_000),
        _ => String::new(),
    };

    format!(
        "{icon} <b>[{severity}] {alert_type}</b>\n\
         <b>Exporter:</b> <code>{exporter}</code>{ip_line}\
         \n{message}{rate_line}",
        icon = icon,
        severity = format!("{:?}", event.severity).to_uppercase(),
        alert_type = event.alert_type,
        exporter = event.exporter_ip,
        ip_line = ip_line,
        message = event.message,
        rate_line = rate_line,
    )
}
```

Example output:
```
🔴 [CRITICAL] sustained
Exporter: 10.0.1.1
IP: 192.168.50.33
Tráfego sustentado anômalo: 450 Mbps por 15 min (base: 12 Mbps, ratio 37×)
Taxa atual: 450 Mbps  |  Base: 12 Mbps  |  Ratio: 37.5×
```

---

## `fetch_unnotified` query

```rust
async fn fetch_unnotified(
    pool: &sqlx::SqlitePool,
    cfg: &TelegramConfig,
    limit: i64,
) -> anyhow::Result<Vec<AlertEvent>> {
    let min_severity = match cfg.min_severity {
        AlertSeverity::Info     => vec!["info", "warning", "critical"],
        AlertSeverity::Warning  => vec!["warning", "critical"],
        AlertSeverity::Critical => vec!["critical"],
    };

    // Build IN clause — sqlx doesn't support dynamic IN with bind, use format
    let placeholders: String = min_severity.iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");

    let query_str = format!(
        "SELECT * FROM alert_events
         WHERE notified = 0 AND severity IN ({})
         ORDER BY created_at ASC LIMIT ?",
        placeholders
    );

    // ... bind all parameters and execute
}
```

---

## `load_telegram_config` helper

```rust
pub async fn load_telegram_config(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<TelegramConfig> {
    let row = sqlx::query!(
        "SELECT bot_token, chat_id, enabled, min_severity FROM telegram_config WHERE id = 1"
    )
    .fetch_one(pool)
    .await?;

    let severity = match row.min_severity.as_str() {
        "info"     => AlertSeverity::Info,
        "critical" => AlertSeverity::Critical,
        _          => AlertSeverity::Warning,
    };

    Ok(TelegramConfig {
        bot_token: row.bot_token,
        chat_id: row.chat_id,
        enabled: row.enabled,
        min_severity: severity,
    })
}
```

---

## Rate limiting

The Telegram Bot API allows 30 messages/second per bot globally, but 1 message/second
per chat. Add a 1.1s sleep between messages in the batch loop to stay safely within limits:

```rust
tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
```

---

## `main.rs` changes

```rust
let notifier_state = app_state.clone();
rt.spawn(async move {
    telegram::run_notifier(notifier_state).await;
});
```

---

## Acceptance criteria

- [ ] `cargo build` passes
- [ ] When `telegram_config.enabled = 0` or `bot_token = ''`, no HTTP calls are made
- [ ] Sent events are marked `notified = 1` in SQLite
- [ ] Failed sends are NOT marked notified (will retry next poll)
- [ ] Message format includes severity, alert_type, exporter_ip, src_ip, and rate info
- [ ] No event is sent twice (deduplication via `notified` flag)
- [ ] Respects `min_severity` filter
