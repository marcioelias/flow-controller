use crate::alerts::AlertEvent;
use sqlx::Row;

const MAX_TOKENS: u32 = 120;
// Local models on CPU can take well over 30 s on the first load (task 17.5 R-02)
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
const BATCH_SIZE: i64 = 5;
/// After this many failures an event leaves the queue (task 17.5 R-03)
pub const MAX_ATTEMPTS: i64 = 3;

/// LLM_ENABLED / LLM_MODEL as seen by the explainer: settings first, env fallback
pub async fn llm_settings(pool: &sqlx::SqlitePool) -> (bool, String) {
    let enabled = crate::settings::get_value(pool, "LLM_ENABLED")
        .await
        .unwrap_or_else(|| std::env::var("LLM_ENABLED").unwrap_or_default())
        == "true";
    let model = crate::settings::get_value(pool, "LLM_MODEL")
        .await
        .unwrap_or_else(|| std::env::var("LLM_MODEL").unwrap_or_else(|_| "qwen2.5:3b".into()));
    (enabled, model)
}

/// Per-event explanation state shown in the UI (task 17.5 R-04)
pub fn explanation_status(has_text: bool, attempts: i64, llm_enabled: bool) -> &'static str {
    if has_text {
        "done"
    } else if attempts >= MAX_ATTEMPTS {
        "failed"
    } else if llm_enabled {
        "pending"
    } else {
        "disabled"
    }
}

pub struct LlmClient {
    language: String,
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl LlmClient {
    /// Build from SQLite settings (preferred) falling back to env vars.
    /// Returns `None` when LLM_ENABLED is not "true" in settings or env.
    pub async fn from_settings(pool: &sqlx::SqlitePool) -> Option<Self> {
        let enabled = crate::settings::get_value(pool, "LLM_ENABLED")
            .await
            .unwrap_or_else(|| std::env::var("LLM_ENABLED").unwrap_or_default());

        if enabled != "true" {
            return None;
        }

        let endpoint = crate::settings::get_value(pool, "LLM_ENDPOINT")
            .await
            .unwrap_or_else(|| {
                std::env::var("LLM_ENDPOINT").unwrap_or_else(|_| "http://ollama:11434".to_string())
            });

        let model = crate::settings::get_value(pool, "LLM_MODEL")
            .await
            .unwrap_or_else(|| {
                std::env::var("LLM_MODEL").unwrap_or_else(|_| "qwen2.5:3b".to_string())
            });

        let language = crate::settings::get_value(pool, "APP_LANGUAGE")
            .await
            .unwrap_or_else(|| "pt-BR".to_string());

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest LLM client");

        Some(Self {
            language,
            endpoint,
            model,
            client,
        })
    }

    /// Fallback for contexts without DB access.
    #[allow(dead_code)]
    pub fn from_env() -> Option<Self> {
        if std::env::var("LLM_ENABLED").as_deref() != Ok("true") {
            return None;
        }
        let endpoint =
            std::env::var("LLM_ENDPOINT").unwrap_or_else(|_| "http://ollama:11434".to_string());
        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "qwen2.5:3b".to_string());
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest LLM client");
        Some(Self {
            language: std::env::var("APP_LANGUAGE").unwrap_or_else(|_| "pt-BR".to_string()),
            endpoint,
            model,
            client,
        })
    }

    async fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        let mut options = serde_json::json!({ "num_predict": MAX_TOKENS, "temperature": 0.3 });
        // Match the container's CPU quota (task 17.5 R-08): more threads than
        // cores under a cgroup limit only adds contention
        if let Some(n) = std::env::var("LLM_NUM_THREADS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n > 0)
        {
            options["num_thread"] = n.into();
        }
        let body = serde_json::json!({
            "model":   self.model,
            "prompt":  prompt,
            "stream":  false,
            "options": options
        });

        let resp: serde_json::Value = self
            .client
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

    pub async fn explain(
        &self,
        event: &AlertEvent,
        window_min: Option<i64>,
    ) -> anyhow::Result<String> {
        let prompt = build_prompt(event, window_min, &self.language);
        self.generate(&prompt).await
    }
}

/// Nome humano do idioma — modelos pequenos seguem melhor uma instrução
/// explícita ("português do Brasil") do que uma tag IETF crua.
fn language_name(tag: &str) -> &str {
    match tag {
        t if t.starts_with("pt") => "Brazilian Portuguese (português do Brasil)",
        t if t.starts_with("es") => "Spanish (español)",
        t if t.starts_with("en") => "English",
        _ => tag,
    }
}

fn build_prompt(ev: &AlertEvent, window_min: Option<i64>, language: &str) -> String {
    // Bytes are summed over the rule's short window, not per second
    let window_secs = window_min.unwrap_or(10).max(1) as f64 * 60.0;
    let mbps = |b: Option<i64>| b.unwrap_or(0) as f64 * 8.0 / window_secs / 1_000_000.0;
    let ctx = match ev.alert_type.as_str() {
        "upload_inversion" => format!(
            "alert_type=upload_inversion src_ip={} upload={:.1}Mbps download={:.1}Mbps \
             (average over the last {:.0} min). \
             The subscriber historically downloads far more than it uploads. \
             Current window shows upload exceeding download significantly.",
            ev.src_ip,
            mbps(ev.upload_bytes),
            mbps(ev.download_bytes),
            window_secs / 60.0,
        ),
        "attack_signature" => format!(
            "alert_type=attack_signature src_ip={} pps={:.0} avg_pkt_bytes={:.0} ports=[{}]. \
             High packet rate with small average packet size targeting known attack/service ports.",
            ev.src_ip,
            ev.pps.unwrap_or(0.0),
            ev.avg_pkt_bytes.unwrap_or(0.0),
            ev.attack_ports.as_deref().unwrap_or("unknown"),
        ),
        "ml_anomaly" => {
            let pps = ev.pps.unwrap_or(0.0);
            // O modelo tende a papaguear "high packet rate" para qualquer
            // anomalia — classificar a taxa aqui impede a alucinação
            let rate_class = if pps < 100.0 {
                "LOW volume — the anomaly is behavioral (pattern deviation), NOT volume"
            } else if pps < 10_000.0 {
                "MODERATE volume"
            } else {
                "HIGH volume"
            };
            format!(
                "alert_type=ml_anomaly src_ip={} pps={:.0} ({rate_class}) avg_pkt_bytes={:.0} \
                 detail={}. Isolation Forest model detected a statistical anomaly in this IP's \
                 traffic profile compared to its learned baseline. Describe the traffic rate \
                 accurately — never call a low rate high.",
                ev.src_ip,
                pps,
                ev.avg_pkt_bytes.unwrap_or(0.0),
                ev.message,
            )
        }
        other => format!(
            "alert_type={other} src_ip={} message={}",
            ev.src_ip, ev.message
        ),
    };

    format!(
        "You are a network security analyst for an ISP. Given this network flow alert, \
         write exactly 2-3 sentences: (1) what traffic pattern this indicates, \
         (2) why it is suspicious or harmful, (3) what the operator should verify. \
         Be concise and technical. Do not repeat the raw numbers verbatim. \
         Write your entire answer in {lang}. Context: {ctx}",
        lang = language_name(language),
    )
}

/// Background task: polls for alert_events without explanation and fills them in.
/// Always running; LLM settings are re-read every cycle so enabling the AI or
/// changing endpoint/model in the UI takes effect without a restart (task 17.5 R-01).
pub async fn run_llm_explainer(pool: sqlx::SqlitePool) {
    let mut active: Option<(String, String)> = None;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let Some(client) = LlmClient::from_settings(&pool).await else {
            if active.take().is_some() {
                tracing::info!("LLM explainer: disabled");
            }
            continue;
        };
        let config = (client.endpoint.clone(), client.model.clone());
        if active.as_ref() != Some(&config) {
            tracing::info!(
                "LLM explainer: using model '{}' at {}",
                client.model,
                client.endpoint
            );
            active = Some(config);
        }

        let rows = match sqlx::query(
            "SELECT e.id, e.rule_id, e.exporter_ip, e.src_ip, e.alert_type, e.severity, e.message,
                    e.upload_bytes, e.download_bytes, e.pps, e.avg_pkt_bytes, e.attack_ports,
                    e.notified, e.bgp_announced, e.created_at,
                    json_extract(r.params, '$.short_window_min') AS window_min
             FROM alert_events e LEFT JOIN alert_rules r ON r.id = e.rule_id
             WHERE (e.explanation IS NULL OR e.explanation = '')
               AND e.explanation_attempts < ?
             ORDER BY e.id DESC
             LIMIT ?",
        )
        .bind(MAX_ATTEMPTS)
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
            let window_min: Option<i64> = row.try_get("window_min").ok().flatten();

            match client.explain(&event, window_min).await {
                Ok(text) if !text.is_empty() => {
                    let _ = sqlx::query(
                        "UPDATE alert_events SET explanation = ?, explanation_error = NULL
                         WHERE id = ?",
                    )
                    .bind(&text)
                    .bind(event_id)
                    .execute(&pool)
                    .await;
                    tracing::debug!("LLM explained alert #{event_id}");
                }
                result => {
                    let err = match result {
                        Err(e) => e.to_string(),
                        Ok(_) => "empty response from the model".to_string(),
                    };
                    tracing::warn!("LLM explain failed for alert #{event_id}: {err}");
                    let _ = sqlx::query(
                        "UPDATE alert_events
                         SET explanation_attempts = explanation_attempts + 1,
                             explanation_error = ?
                         WHERE id = ?",
                    )
                    .bind(err.chars().take(300).collect::<String>())
                    .bind(event_id)
                    .execute(&pool)
                    .await;
                }
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
        id: row.try_get("id")?,
        rule_id: row.try_get("rule_id")?,
        exporter_ip: row.try_get("exporter_ip")?,
        src_ip: row.try_get("src_ip")?,
        alert_type: row.try_get("alert_type")?,
        severity,
        message: row.try_get("message")?,
        upload_bytes: row.try_get("upload_bytes")?,
        download_bytes: row.try_get("download_bytes")?,
        pps: row.try_get("pps")?,
        avg_pkt_bytes: row.try_get("avg_pkt_bytes")?,
        attack_ports: row.try_get("attack_ports")?,
        notified: row.try_get::<i64, _>("notified")? != 0,
        bgp_announced: row.try_get::<i64, _>("bgp_announced")? != 0,
        created_at: row.try_get("created_at")?,
    })
}

// ── Endpoints de suporte à configuração guiada (task 15.4) ──────────────────

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct LlmProbeBody {
    pub endpoint: String,
    /// Necessário só para o teste de geração
    pub model: Option<String>,
}

/// POST /api/llm/models — lista os modelos disponíveis no endpoint informado
/// (proxy de GET /api/tags do Ollama; o navegador não alcança o Ollama direto)
pub async fn list_models_handler(
    State(_state): State<Arc<crate::auth::AppState>>,
    Json(body): Json<LlmProbeBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let resp = client
        .get(format!("{}/api/tags", body.endpoint.trim_end_matches('/')))
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let val: serde_json::Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let models: Vec<String> = val["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(serde_json::json!({ "models": models })))
}

/// POST /api/llm/test — gera uma frase curta para validar endpoint+modelo
pub async fn test_llm_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(body): Json<LlmProbeBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(model) = body.model else {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    };
    let language = crate::settings::get_value(&state.db, "APP_LANGUAGE")
        .await
        .unwrap_or_else(|| "pt-BR".to_string());
    let client = LlmClient {
        language: language.clone(),
        endpoint: body.endpoint,
        model,
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    };
    let prompt = format!(
        "Reply with one short sentence in {} confirming you are ready to explain          network anomalies.",
        language_name(&language)
    );
    match client.generate(&prompt).await {
        Ok(text) => Ok(Json(
            serde_json::json!({ "ok": true, "response": text.trim() }),
        )),
        Err(e) => Ok(Json(
            serde_json::json!({ "ok": false, "error": e.to_string() }),
        )),
    }
}
