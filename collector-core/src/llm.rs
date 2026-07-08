use crate::alerts::AlertEvent;
use sqlx::Row;

const MAX_TOKENS:       u32 = 120;
const REQUEST_TIMEOUT:  std::time::Duration = std::time::Duration::from_secs(30);
const POLL_INTERVAL:    std::time::Duration = std::time::Duration::from_secs(10);
const BATCH_SIZE:       i64 = 5;

pub struct LlmClient {
    endpoint: String,
    model:    String,
    client:   reqwest::Client,
}

impl LlmClient {
    /// Build from SQLite settings (preferred) falling back to env vars.
    /// Returns `None` when LLM_ENABLED is not "true" in settings or env.
    pub async fn from_settings(pool: &sqlx::SqlitePool) -> Option<Self> {
        let enabled = crate::settings::get_value(pool, "LLM_ENABLED").await
            .unwrap_or_else(|| std::env::var("LLM_ENABLED").unwrap_or_default());

        if enabled != "true" {
            return None;
        }

        let endpoint = crate::settings::get_value(pool, "LLM_ENDPOINT").await
            .unwrap_or_else(|| std::env::var("LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://ollama:11434".to_string()));

        let model = crate::settings::get_value(pool, "LLM_MODEL").await
            .unwrap_or_else(|| std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "qwen2.5:3b".to_string()));

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest LLM client");

        Some(Self { endpoint, model, client })
    }

    /// Fallback for contexts without DB access.
    #[allow(dead_code)]
    pub fn from_env() -> Option<Self> {
        if std::env::var("LLM_ENABLED").as_deref() != Ok("true") {
            return None;
        }
        let endpoint = std::env::var("LLM_ENDPOINT")
            .unwrap_or_else(|_| "http://ollama:11434".to_string());
        let model = std::env::var("LLM_MODEL")
            .unwrap_or_else(|_| "qwen2.5:3b".to_string());
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest LLM client");
        Some(Self { endpoint, model, client })
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model":   self.model,
            "prompt":  prompt,
            "stream":  false,
            "options": { "num_predict": MAX_TOKENS, "temperature": 0.3 }
        });

        let resp: serde_json::Value = self.client
            .post(format!("{}/api/generate", self.endpoint))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        resp["response"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("no 'response' field in LLM output"))
    }

    pub async fn explain(&self, event: &AlertEvent) -> anyhow::Result<String> {
        let prompt = build_prompt(event);
        self.generate(&prompt).await
    }
}

fn build_prompt(ev: &AlertEvent) -> String {
    let ctx = match ev.alert_type.as_str() {
        "upload_inversion" => format!(
            "alert_type=upload_inversion src_ip={} upload={:.1}Mbps download={:.1}Mbps. \
             The subscriber historically downloads far more than it uploads. \
             Current window shows upload exceeding download significantly.",
            ev.src_ip,
            ev.upload_bytes.unwrap_or(0)   as f64 / 125_000.0,
            ev.download_bytes.unwrap_or(0) as f64 / 125_000.0,
        ),
        "attack_signature" => format!(
            "alert_type=attack_signature src_ip={} pps={:.0} avg_pkt_bytes={:.0} ports=[{}]. \
             High packet rate with small average packet size targeting known attack/service ports.",
            ev.src_ip,
            ev.pps.unwrap_or(0.0),
            ev.avg_pkt_bytes.unwrap_or(0.0),
            ev.attack_ports.as_deref().unwrap_or("unknown"),
        ),
        "ml_anomaly" => format!(
            "alert_type=ml_anomaly src_ip={} pps={:.0} avg_pkt_bytes={:.0} detail={}. \
             Isolation Forest model detected a statistical anomaly in this IP's traffic profile \
             compared to its learned baseline.",
            ev.src_ip,
            ev.pps.unwrap_or(0.0),
            ev.avg_pkt_bytes.unwrap_or(0.0),
            ev.message,
        ),
        other => format!(
            "alert_type={other} src_ip={} message={}",
            ev.src_ip, ev.message
        ),
    };

    format!(
        "You are a network security analyst for an ISP. Given this network flow alert, \
         write exactly 2-3 sentences: (1) what traffic pattern this indicates, \
         (2) why it is suspicious or harmful, (3) what the operator should verify. \
         Be concise and technical. Do not repeat the raw numbers verbatim. Context: {ctx}"
    )
}

/// Background task: polls for alert_events without explanation and fills them in.
pub async fn run_llm_explainer(pool: sqlx::SqlitePool, client: LlmClient) {
    tracing::info!("LLM explainer started (model={})", client.model_name());

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let rows = match sqlx::query(
            "SELECT id, rule_id, exporter_ip, src_ip, alert_type, severity, message,
                    upload_bytes, download_bytes, pps, avg_pkt_bytes, attack_ports,
                    notified, bgp_announced, created_at
             FROM alert_events
             WHERE (explanation IS NULL OR explanation = '')
             ORDER BY id DESC
             LIMIT ?",
        )
        .bind(BATCH_SIZE)
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("LLM explainer: DB fetch failed: {e}");
                continue;
            }
        };

        for row in rows {
            let event = match row_to_event(&row) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("LLM explainer: row parse failed: {e}");
                    continue;
                }
            };

            let event_id = match event.id {
                Some(id) => id,
                None => continue,
            };

            match client.explain(&event).await {
                Ok(text) => {
                    let _ = sqlx::query(
                        "UPDATE alert_events SET explanation = ? WHERE id = ?",
                    )
                    .bind(&text)
                    .bind(event_id)
                    .execute(&pool)
                    .await;
                    tracing::debug!("LLM explained alert #{event_id}");
                }
                Err(e) => tracing::warn!("LLM explain failed for alert #{event_id}: {e}"),
            }

            // Respect ~1 req/s to avoid overloading Ollama
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        }
    }
}

fn row_to_event(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<AlertEvent> {
    use crate::alerts::AlertSeverity;

    let severity_str: String = row.try_get("severity")?;
    let severity = if severity_str == "critical" {
        AlertSeverity::Critical
    } else {
        AlertSeverity::Warning
    };

    Ok(AlertEvent {
        id:            row.try_get("id")?,
        rule_id:       row.try_get("rule_id")?,
        exporter_ip:   row.try_get("exporter_ip")?,
        src_ip:        row.try_get("src_ip")?,
        alert_type:    row.try_get("alert_type")?,
        severity,
        message:       row.try_get("message")?,
        upload_bytes:  row.try_get("upload_bytes")?,
        download_bytes: row.try_get("download_bytes")?,
        pps:           row.try_get("pps")?,
        avg_pkt_bytes: row.try_get("avg_pkt_bytes")?,
        attack_ports:  row.try_get("attack_ports")?,
        notified:      row.try_get::<i64, _>("notified")? != 0,
        bgp_announced: row.try_get::<i64, _>("bgp_announced")? != 0,
        created_at:    row.try_get("created_at")?,
    })
}
