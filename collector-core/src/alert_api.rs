use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

use crate::alerts::{AlertEvent, AlertRule, AlertSeverity};
use crate::auth::AppState;

// ---------------------------------------------------------------------------
// Alert Rules
// ---------------------------------------------------------------------------

pub async fn list_rules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AlertRule>>, StatusCode> {
    let rows = sqlx::query(
        "SELECT id, name, exporter_id, rule_type, enabled, params, created_at
         FROM alert_rules ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rules: Vec<AlertRule> = rows
        .into_iter()
        .map(|r| {
            use sqlx::Row;
            AlertRule {
                id: r.try_get("id").ok(),
                name: r.try_get("name").unwrap_or_default(),
                exporter_id: r.try_get("exporter_id").ok().flatten(),
                rule_type: r.try_get("rule_type").unwrap_or_default(),
                enabled: r.try_get::<i64, _>("enabled").unwrap_or(0) != 0,
                params: {
                    let s: String = r.try_get("params").unwrap_or_default();
                    serde_json::from_str(&s)
                        .unwrap_or(serde_json::Value::Object(Default::default()))
                },
                created_at: r.try_get("created_at").ok().flatten(),
            }
        })
        .collect();

    Ok(Json(rules))
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub exporter_id: Option<i64>,
    pub rule_type: String,
    pub enabled: Option<bool>,
    pub params: serde_json::Value,
}

pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<AlertRule>), (StatusCode, Json<serde_json::Value>)> {
    validate_rule_type(&req.rule_type, &req.params)?;

    let params_str = serde_json::to_string(&req.params).unwrap_or("{}".to_string());
    let enabled = req.enabled.unwrap_or(true);

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO alert_rules (name, exporter_id, rule_type, enabled, params)
         VALUES (?,?,?,?,?) RETURNING id",
    )
    .bind(&req.name)
    .bind(req.exporter_id)
    .bind(&req.rule_type)
    .bind(enabled)
    .bind(&params_str)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let rule = AlertRule {
        id: Some(id),
        name: req.name,
        exporter_id: req.exporter_id,
        rule_type: req.rule_type,
        enabled,
        params: req.params,
        created_at: None,
    };

    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<Json<AlertRule>, (StatusCode, Json<serde_json::Value>)> {
    validate_rule_type(&req.rule_type, &req.params)?;

    let params_str = serde_json::to_string(&req.params).unwrap_or("{}".to_string());
    let enabled = req.enabled.unwrap_or(true);

    let rows = sqlx::query(
        "UPDATE alert_rules SET name=?, exporter_id=?, rule_type=?, enabled=?, params=?
         WHERE id=?",
    )
    .bind(&req.name)
    .bind(req.exporter_id)
    .bind(&req.rule_type)
    .bind(enabled)
    .bind(&params_str)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?
    .rows_affected();

    if rows == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "rule not found"})),
        ));
    }

    Ok(Json(AlertRule {
        id: Some(id),
        name: req.name,
        exporter_id: req.exporter_id,
        rule_type: req.rule_type,
        enabled,
        params: req.params,
        created_at: None,
    }))
}

pub async fn delete_rule(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> StatusCode {
    match sqlx::query("DELETE FROM alert_rules WHERE id=?")
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => StatusCode::NO_CONTENT,
        Ok(_) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
pub struct ToggleResponse {
    id: i64,
    enabled: bool,
}

pub async fn toggle_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ToggleResponse>, StatusCode> {
    let rows = sqlx::query("UPDATE alert_rules SET enabled = NOT enabled WHERE id=?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .rows_affected();

    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    let enabled: i64 = sqlx::query_scalar("SELECT enabled FROM alert_rules WHERE id=?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ToggleResponse {
        id,
        enabled: enabled != 0,
    }))
}

fn validate_rule_type(
    rule_type: &str,
    params: &serde_json::Value,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let bad = |msg: &str| {
        Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        ))
    };

    match rule_type {
        "upload_inversion" => {
            for key in &[
                "short_window_min",
                "history_window_min",
                "inversion_ratio",
                "cooldown_min",
            ] {
                if params.get(key).is_none() {
                    return bad(&format!("upload_inversion requires param: {key}"));
                }
            }
        }
        "attack_signature" => {
            for key in &[
                "window_min",
                "min_pps",
                "max_avg_pkt_bytes",
                "min_total_packets",
            ] {
                if params.get(key).is_none() {
                    return bad(&format!("attack_signature requires param: {key}"));
                }
            }
        }
        _ => return bad("unknown rule_type; valid: upload_inversion, attack_signature"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Alert Events
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EventsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub severity: Option<String>,
    pub notified: Option<String>,
    pub sort: Option<String>,
    pub dir: Option<String>,
}

/// Event plus the rule's short window, so rates can be derived from the
/// window-summed upload/download bytes (task 17.4 R-03)
#[derive(Serialize)]
pub struct EventRow {
    #[serde(flatten)]
    pub event: AlertEvent,
    pub window_min: Option<i64>,
}

#[derive(Serialize)]
pub struct EventsResponse {
    pub total: i64,
    pub events: Vec<EventRow>,
}

#[derive(Serialize)]
pub struct EventRule {
    pub id: i64,
    pub name: String,
    pub rule_type: String,
    pub params: serde_json::Value,
}

#[derive(Serialize)]
pub struct EventDetail {
    #[serde(flatten)]
    pub event: AlertEvent,
    pub window_min: Option<i64>,
    pub explanation: Option<String>,
    /// done | failed | pending | disabled (task 17.5 R-04)
    pub explanation_status: &'static str,
    pub explanation_error: Option<String>,
    pub feedback: Option<String>,
    pub exporter_name: Option<String>,
    pub rule: Option<EventRule>,
}

const EVENT_COLUMNS: &str = "e.id, e.rule_id, e.exporter_ip, e.src_ip, e.alert_type, e.severity, \
     e.message, e.upload_bytes, e.download_bytes, e.pps, e.avg_pkt_bytes, e.attack_ports, \
     e.notified, e.bgp_announced, e.created_at, \
     json_extract(r.params, '$.short_window_min') AS window_min";

/// Whitelisted sort columns (task 17.3 R-05); anything else falls back to time
fn event_order_by(sort: Option<&str>, dir: Option<&str>) -> String {
    let col = match sort {
        Some("severity") => "CASE e.severity WHEN 'critical' THEN 2 ELSE 1 END",
        Some("alert_type") => "e.alert_type",
        Some("exporter_ip") => "e.exporter_ip",
        Some("src_ip") => "e.src_ip",
        Some("notified") => "e.notified",
        Some("bgp_announced") => "e.bgp_announced",
        _ => "e.created_at",
    };
    let dir = if dir == Some("asc") { "ASC" } else { "DESC" };
    format!("{col} {dir}, e.id {dir}")
}

fn event_from_row(r: &sqlx::sqlite::SqliteRow) -> AlertEvent {
    AlertEvent {
        id: r.try_get("id").ok(),
        rule_id: r.try_get("rule_id").ok().flatten(),
        exporter_ip: r.try_get("exporter_ip").unwrap_or_default(),
        src_ip: r.try_get("src_ip").unwrap_or_default(),
        alert_type: r.try_get("alert_type").unwrap_or_default(),
        severity: {
            let s: &str = r.try_get("severity").unwrap_or("warning");
            if s == "critical" {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            }
        },
        message: r.try_get("message").unwrap_or_default(),
        upload_bytes: r.try_get("upload_bytes").ok().flatten(),
        download_bytes: r.try_get("download_bytes").ok().flatten(),
        pps: r.try_get("pps").ok().flatten(),
        avg_pkt_bytes: r.try_get("avg_pkt_bytes").ok().flatten(),
        attack_ports: r.try_get("attack_ports").ok().flatten(),
        notified: r.try_get::<i64, _>("notified").unwrap_or(0) != 0,
        bgp_announced: r.try_get::<i64, _>("bgp_announced").unwrap_or(0) != 0,
        created_at: r.try_get("created_at").ok().flatten(),
    }
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<EventsResponse>, StatusCode> {
    let limit = q.limit.unwrap_or(50).min(500);
    let offset = q.offset.unwrap_or(0);

    let mut conditions = vec!["1=1"];
    match q.severity.as_deref() {
        Some("warning") => conditions.push("e.severity = 'warning'"),
        Some("critical") => conditions.push("e.severity = 'critical'"),
        _ => {}
    }
    match q.notified.as_deref() {
        Some("true") => conditions.push("e.notified = 1"),
        Some("false") => conditions.push("e.notified = 0"),
        _ => {}
    }
    let where_clause = conditions.join(" AND ");
    let order_by = event_order_by(q.sort.as_deref(), q.dir.as_deref());

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM alert_events e WHERE {where_clause}"
    ))
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query(&format!(
        "SELECT {EVENT_COLUMNS}
         FROM alert_events e LEFT JOIN alert_rules r ON r.id = e.rule_id
         WHERE {where_clause}
         ORDER BY {order_by} LIMIT ? OFFSET ?"
    ))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let events = rows
        .iter()
        .map(|r| EventRow {
            event: event_from_row(r),
            window_min: r.try_get("window_min").ok().flatten(),
        })
        .collect();

    Ok(Json(EventsResponse { total, events }))
}

pub async fn get_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<EventDetail>, StatusCode> {
    let row = sqlx::query(&format!(
        "SELECT {EVENT_COLUMNS}, e.explanation, e.feedback,
                e.explanation_attempts, e.explanation_error,
                r.id AS r_id, r.name AS r_name, r.rule_type AS r_type, r.params AS r_params,
                x.name AS exporter_name
         FROM alert_events e
         LEFT JOIN alert_rules r ON r.id = e.rule_id
         LEFT JOIN exporters x ON x.ip_address = e.exporter_ip
         WHERE e.id = ?"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let rule = row
        .try_get::<Option<i64>, _>("r_id")
        .ok()
        .flatten()
        .map(|rid| EventRule {
            id: rid,
            name: row.try_get("r_name").unwrap_or_default(),
            rule_type: row.try_get("r_type").unwrap_or_default(),
            params: row
                .try_get::<String, _>("r_params")
                .ok()
                .and_then(|p| serde_json::from_str(&p).ok())
                .unwrap_or(serde_json::Value::Null),
        });

    let explanation: Option<String> = row
        .try_get::<Option<String>, _>("explanation")
        .ok()
        .flatten()
        .filter(|s| !s.is_empty());
    let attempts: i64 = row.try_get("explanation_attempts").unwrap_or(0);
    let (llm_enabled, _) = crate::llm::llm_settings(&state.db).await;

    Ok(Json(EventDetail {
        event: event_from_row(&row),
        window_min: row.try_get("window_min").ok().flatten(),
        explanation_status: crate::llm::explanation_status(
            explanation.is_some(),
            attempts,
            llm_enabled,
        ),
        explanation_error: row.try_get("explanation_error").ok().flatten(),
        explanation,
        feedback: row.try_get("feedback").ok().flatten(),
        exporter_name: row.try_get("exporter_name").ok().flatten(),
        rule,
    }))
}

/// Puts an event back in the explainer queue (task 17.5 R-05)
pub async fn retry_explanation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> StatusCode {
    match sqlx::query(
        "UPDATE alert_events SET explanation_attempts = 0, explanation_error = NULL WHERE id = ?",
    )
    .bind(id)
    .execute(&state.db)
    .await
    {
        Ok(r) if r.rows_affected() == 0 => StatusCode::NOT_FOUND,
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn clear_events(State(state): State<Arc<AppState>>) -> StatusCode {
    match sqlx::query("DELETE FROM alert_events")
        .execute(&state.db)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ---------------------------------------------------------------------------
// Telegram Config
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TelegramConfigMasked {
    pub bot_token: String,
    pub chat_id: String,
    pub enabled: bool,
    pub min_severity: String,
}

fn mask_token(token: &str) -> String {
    if token.len() <= 4 {
        return "*".repeat(token.len());
    }
    let visible = &token[token.len() - 4..];
    format!("{}{}", "*".repeat(token.len() - 4), visible)
}

fn severity_to_str(s: &AlertSeverity) -> &'static str {
    match s {
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    }
}

pub async fn get_telegram(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TelegramConfigMasked>, StatusCode> {
    let cfg = crate::telegram::load_telegram_config(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TelegramConfigMasked {
        bot_token: mask_token(&cfg.bot_token),
        chat_id: cfg.chat_id,
        enabled: cfg.enabled,
        min_severity: severity_to_str(&cfg.min_severity).to_string(),
    }))
}

#[derive(Deserialize)]
pub struct TelegramUpdateRequest {
    pub bot_token: String,
    pub chat_id: String,
    pub enabled: bool,
    pub min_severity: String,
}

pub async fn update_telegram(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TelegramUpdateRequest>,
) -> Result<Json<TelegramConfigMasked>, StatusCode> {
    sqlx::query(
        "UPDATE telegram_config SET bot_token=?, chat_id=?, enabled=?, min_severity=?,
         updated_at=CURRENT_TIMESTAMP WHERE id=1",
    )
    .bind(&req.bot_token)
    .bind(&req.chat_id)
    .bind(req.enabled)
    .bind(&req.min_severity)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TelegramConfigMasked {
        bot_token: mask_token(&req.bot_token),
        chat_id: req.chat_id,
        enabled: req.enabled,
        min_severity: req.min_severity,
    }))
}

#[derive(Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

pub async fn test_telegram(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TestResult>, (StatusCode, Json<TestResult>)> {
    let cfg = crate::telegram::load_telegram_config(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(TestResult {
                    ok: false,
                    message: "Failed to load config".to_string(),
                }),
            )
        })?;

    if cfg.bot_token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(TestResult {
                ok: false,
                message: "Bot token not configured".to_string(),
            }),
        ));
    }

    let test_msg =
        "🔵 <b>[TEST]</b> FlowVision alert test message — Telegram integration is working.";
    match crate::telegram::send_message(&cfg, test_msg).await {
        Ok(()) => Ok(Json(TestResult {
            ok: true,
            message: "Test message sent successfully".to_string(),
        })),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(TestResult {
                ok: false,
                message: format!("Telegram API error: {e}"),
            }),
        )),
    }
}

// ---------------------------------------------------------------------------
// Exporter IP lookup for rules display
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub async fn get_exporter_ip(pool: &sqlx::SqlitePool, exporter_id: i64) -> Option<String> {
    sqlx::query_scalar("SELECT ip_address FROM exporters WHERE id=?")
        .bind(exporter_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}
