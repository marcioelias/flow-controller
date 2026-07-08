use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const DEFAULT_ATTACK_PORTS: &[u16] = &[
    123, 1900, 11211, 5353, 19, 17, // amplification sources
    80, 443, 8080, // HTTP/S flood targets
    53,            // DNS flood
    22,            // SSH brute force
    25, 465, 587,  // SMTP
    3389,          // RDP
    6379,          // Redis
    27017,         // MongoDB
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Warning,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for AlertSeverity {
    fn decode(
        value: sqlx::sqlite::SqliteValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        match s {
            "critical" => Ok(AlertSeverity::Critical),
            _ => Ok(AlertSeverity::Warning),
        }
    }
}

impl sqlx::Type<sqlx::Sqlite> for AlertSeverity {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <&str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for AlertSeverity {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> sqlx::encode::IsNull {
        let s = self.to_string();
        <String as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&s, buf)
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
    pub upload_bytes: Option<i64>,
    pub download_bytes: Option<i64>,
    pub pps: Option<f64>,
    pub avg_pkt_bytes: Option<f64>,
    pub attack_ports: Option<String>,
    pub notified: bool,
    pub bgp_announced: bool,
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

pub async fn init_tables(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS alert_rules (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            exporter_id INTEGER REFERENCES exporters(id) ON DELETE CASCADE,
            rule_type   TEXT NOT NULL
                        CHECK(rule_type IN ('upload_inversion', 'attack_signature', 'ml_anomaly')),
            enabled     BOOLEAN NOT NULL DEFAULT 1,
            params      TEXT NOT NULL DEFAULT '{}',
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS telegram_config (
            id           INTEGER PRIMARY KEY CHECK(id = 1),
            bot_token    TEXT NOT NULL DEFAULT '',
            chat_id      TEXT NOT NULL DEFAULT '',
            enabled      BOOLEAN NOT NULL DEFAULT 0,
            min_severity TEXT NOT NULL DEFAULT 'warning'
                         CHECK(min_severity IN ('info','warning','critical')),
            updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("INSERT OR IGNORE INTO telegram_config (id) VALUES (1)")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS alert_events (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id        INTEGER REFERENCES alert_rules(id) ON DELETE SET NULL,
            exporter_ip    TEXT NOT NULL,
            src_ip         TEXT NOT NULL,
            alert_type     TEXT NOT NULL,
            severity       TEXT NOT NULL CHECK(severity IN ('warning','critical')),
            message        TEXT NOT NULL,
            upload_bytes   INTEGER,
            download_bytes INTEGER,
            pps            REAL,
            avg_pkt_bytes  REAL,
            attack_ports   TEXT,
            notified       BOOLEAN NOT NULL DEFAULT 0,
            bgp_announced  BOOLEAN NOT NULL DEFAULT 0,
            created_at     DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Idempotent migrations
    let _ = sqlx::query(
        "ALTER TABLE alert_events ADD COLUMN bgp_announced BOOLEAN NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "ALTER TABLE alert_events ADD COLUMN explanation TEXT",
    )
    .execute(pool)
    .await;

    // Prune old events at startup
    sqlx::query("DELETE FROM alert_events WHERE created_at < datetime('now', '-7 days')")
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn insert_event(pool: &SqlitePool, event: &AlertEvent) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO alert_events
         (rule_id, exporter_ip, src_ip, alert_type, severity, message,
          upload_bytes, download_bytes, pps, avg_pkt_bytes, attack_ports)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)
         RETURNING id",
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
    pool: &SqlitePool,
    rule_id: i64,
    src_ip: &str,
    cooldown_min: i64,
) -> anyhow::Result<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE rule_id = ? AND src_ip = ?
           AND created_at >= datetime('now', '-' || ? || ' minutes')",
    )
    .bind(rule_id)
    .bind(src_ip)
    .bind(cooldown_min)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn mark_bgp_announced(pool: &SqlitePool, event_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE alert_events SET bgp_announced = 1 WHERE id = ?")
        .bind(event_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn load_enabled_rules(pool: &SqlitePool) -> anyhow::Result<Vec<AlertRule>> {
    let rows = sqlx::query(
        "SELECT id, name, exporter_id, rule_type, enabled, params, created_at
         FROM alert_rules WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            use sqlx::Row;
            Ok(AlertRule {
                id: r.try_get("id")?,
                name: r.try_get("name")?,
                exporter_id: r.try_get("exporter_id")?,
                rule_type: r.try_get("rule_type")?,
                enabled: r.try_get::<i64, _>("enabled")? != 0,
                params: {
                    let s: String = r.try_get("params")?;
                    serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(Default::default()))
                },
                created_at: r.try_get("created_at")?,
            })
        })
        .collect()
}
