# Task 6.1 — Alert Infrastructure (SQLite tables + Rust types)

## Goal

Create the persistence layer for the alerting system: SQLite tables for alert rules,
Telegram configuration, and alert event history. Define the shared Rust types used
by all subsequent alerting tasks (6.2–6.4).

No alerting logic yet — this task is purely schema + types.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/alerts.rs` | CREATE — types + DB init |
| `collector-core/src/main.rs` | MODIFY — call `alerts::init_tables` at startup |

---

## SQLite Schema

```rust
pub async fn init_tables(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS alert_rules (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT NOT NULL,
            exporter_id     INTEGER REFERENCES exporters(id) ON DELETE CASCADE,
            -- null = applies to all exporters
            rule_type       TEXT NOT NULL
                            CHECK(rule_type IN ('upload_inversion', 'attack_signature')),
            enabled         BOOLEAN NOT NULL DEFAULT 1,
            params          TEXT NOT NULL DEFAULT '{}',
            created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#).execute(pool).await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS telegram_config (
            id              INTEGER PRIMARY KEY CHECK(id = 1),
            bot_token       TEXT NOT NULL DEFAULT '',
            chat_id         TEXT NOT NULL DEFAULT '',
            enabled         BOOLEAN NOT NULL DEFAULT 0,
            min_severity    TEXT NOT NULL DEFAULT 'warning'
                            CHECK(min_severity IN ('info','warning','critical')),
            updated_at      DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#).execute(pool).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO telegram_config (id) VALUES (1)"
    ).execute(pool).await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS alert_events (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id         INTEGER REFERENCES alert_rules(id) ON DELETE SET NULL,
            exporter_ip     TEXT NOT NULL,
            src_ip          TEXT NOT NULL,
            alert_type      TEXT NOT NULL,
            severity        TEXT NOT NULL CHECK(severity IN ('warning','critical')),
            message         TEXT NOT NULL,
            -- upload_inversion: bytes sent (upload) in current window
            upload_bytes    INTEGER,
            -- upload_inversion: bytes received (download) in current window
            download_bytes  INTEGER,
            -- attack_signature: packets per second at alert time
            pps             REAL,
            -- attack_signature: average packet size in bytes
            avg_pkt_bytes   REAL,
            -- attack_signature: comma-separated destination ports that matched
            attack_ports    TEXT,
            notified        BOOLEAN NOT NULL DEFAULT 0,
            created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#).execute(pool).await?;

    sqlx::query(
        "DELETE FROM alert_events WHERE created_at < datetime('now', '-7 days')"
    ).execute(pool).await?;

    Ok(())
}
```

---

## Rule params JSON by rule_type

```jsonc
// rule_type = "upload_inversion"
// Fires when a historically download-heavy IP starts uploading more than it downloads.
// Requires a minimum history window to establish the baseline ratio.
{
    "short_window_min": 10,     // window to measure current upload/download ratio
    "history_window_min": 1440, // lookback to establish historical ratio (default 24h)
    "inversion_ratio": 2.0,     // alert when current_upload > current_download * ratio
    "min_hist_download_ratio": 3.0, // IP must have been historically download/upload >= N
                                    // (skip IPs with no clear download-heavy pattern)
    "min_upload_mbps": 5,       // ignore IPs with < N Mbps upload (filter out noise)
    "cooldown_min": 30          // min minutes between repeat alerts for same IP
}

// rule_type = "attack_signature"
// Fires when a src_ip generates high PPS with small average packet size
// towards known attack destination ports.
// No history required — signal is self-evident.
{
    "window_min": 2,            // evaluation window (short — attack traffic is fast)
    "min_pps": 1000,            // minimum packets per second to consider
    "max_avg_pkt_bytes": 200,   // alert only if avg packet size is below this
    "min_total_packets": 10000, // minimum total packets in window (avoid 1-packet noise)
    "attack_ports": [],         // empty = use built-in default list (see below)
                                // or specify custom list: [80, 443, 53, 22]
    "cooldown_min": 5           // repeat alert cooldown (attacks escalate fast)
}
```

### Built-in attack port list (used when `attack_ports` is empty)

These are ports that legitimate subscriber outbound traffic almost never targets at high PPS:

```rust
pub const DEFAULT_ATTACK_PORTS: &[u16] = &[
    // Amplification sources (subscriber sending to these = amplification attempt)
    123,   // NTP
    1900,  // SSDP
    11211, // Memcached
    5353,  // mDNS
    19,    // CHARGEN
    17,    // QOTD
    // Common DDoS flood targets (subscriber blasting these = participating in flood)
    80, 443, 8080,  // HTTP/S
    53,             // DNS flood
    22,             // SSH brute force
    25, 465, 587,   // SMTP spam / brute force
    3389,           // RDP brute force
    6379,           // Redis (commonly targeted)
    27017,          // MongoDB
];
```

---

## Rust Types

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Warning,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Warning  => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub id: Option<i64>,
    pub rule_id: Option<i64>,
    pub exporter_ip: String,
    pub src_ip: String,
    pub alert_type: String,
    pub severity: AlertSeverity,
    pub message: String,
    // upload_inversion fields
    pub upload_bytes: Option<i64>,
    pub download_bytes: Option<i64>,
    // attack_signature fields
    pub pps: Option<f64>,
    pub avg_pkt_bytes: Option<f64>,
    pub attack_ports: Option<String>,
    pub notified: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: Option<i64>,
    pub name: String,
    pub exporter_id: Option<i64>,
    pub rule_type: String,
    pub enabled: bool,
    pub params: serde_json::Value,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
    pub enabled: bool,
    pub min_severity: AlertSeverity,
}
```

---

## `alerts.rs` — insert_event helper

```rust
pub async fn insert_event(
    pool: &sqlx::SqlitePool,
    event: &AlertEvent,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO alert_events
         (rule_id, exporter_ip, src_ip, alert_type, severity, message,
          upload_bytes, download_bytes, pps, avg_pkt_bytes, attack_ports)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)
         RETURNING id"
    )
    .bind(event.rule_id)
    .bind(&event.exporter_ip)
    .bind(&event.src_ip)
    .bind(&event.alert_type)
    .bind(event.severity.to_string())
    .bind(&event.message)
    .bind(event.upload_bytes)
    .bind(event.download_bytes)
    .bind(event.pps)
    .bind(event.avg_pkt_bytes)
    .bind(&event.attack_ports)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn already_fired_recently(
    pool: &sqlx::SqlitePool,
    rule_id: i64,
    src_ip: &str,
    cooldown_min: i64,
) -> anyhow::Result<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE rule_id = ? AND src_ip = ?
           AND created_at >= datetime('now', '-' || ? || ' minutes')"
    )
    .bind(rule_id)
    .bind(src_ip)
    .bind(cooldown_min)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}
```

---

## `main.rs` changes

After `exporters::init_exporters_table(&auth_db)`:
```rust
alerts::init_tables(&auth_db).await?;
tracing::info!("Alert tables initialized");
```

Add `clickhouse_url: String` field to `AppState` (needed by the detector in task 6.2):
```rust
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub ws_tx: broadcast::Sender<LiveFlowStats>,
    pub metrics: Arc<metrics::CollectorMetrics>,
    pub clickhouse_url: String,   // ADD
}
```

---

## Acceptance criteria

- [ ] `cargo build` passes
- [ ] Three tables created on first run
- [ ] Singleton telegram_config row inserted
- [ ] Old events pruned at startup
- [ ] `already_fired_recently` returns false when no prior events exist
- [ ] No existing tests broken
