use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;

use crate::auth::AppState;
use crate::ml_runner::{MlModelStatus, SharedMlStatus};

#[derive(Serialize)]
pub struct MlStatus {
    pub llm_enabled: bool,
    pub llm_model: String,
    pub exporters: Vec<ExporterStatusOut>,
}

#[derive(Serialize)]
pub struct ExporterStatusOut {
    pub exporter_ip: String,
    pub status: &'static str,
    pub samples_collected: usize,
    pub samples_needed: usize,
    pub n_scored: u64,
    pub anomalies_total: i64,
}

#[derive(Serialize)]
pub struct MlStats {
    pub total_ml_anomalies: i64,
    pub anomalies_last_24h: i64,
    pub top_offenders: Vec<TopOffender>,
    pub severity_breakdown: SeverityBreakdown,
}

#[derive(Serialize)]
pub struct TopOffender {
    pub src_ip: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct SeverityBreakdown {
    pub warning: i64,
    pub critical: i64,
}

pub async fn get_ml_status(
    State(state): State<Arc<AppState>>,
    axum::Extension(shared_status): axum::Extension<SharedMlStatus>,
) -> Result<Json<MlStatus>, StatusCode> {
    let (llm_enabled, llm_model) = crate::llm::llm_settings(&state.db).await;

    let snapshot: Vec<MlModelStatus> = shared_status.read().map(|g| g.clone()).unwrap_or_default();

    let mut exporters_out = Vec::with_capacity(snapshot.len());
    for s in snapshot {
        let anomalies_total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM alert_events
             WHERE alert_type = 'ml_anomaly' AND exporter_ip = ?",
        )
        .bind(&s.exporter_ip)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        exporters_out.push(ExporterStatusOut {
            exporter_ip: s.exporter_ip,
            status: s.status,
            samples_collected: s.samples_collected,
            samples_needed: s.samples_needed,
            n_scored: s.n_scored,
            anomalies_total,
        });
    }

    Ok(Json(MlStatus {
        llm_enabled,
        llm_model,
        exporters: exporters_out,
    }))
}

pub async fn get_ml_stats(State(state): State<Arc<AppState>>) -> Result<Json<MlStats>, StatusCode> {
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events WHERE alert_type = 'ml_anomaly'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let last_24h = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE alert_type = 'ml_anomaly'
           AND created_at >= datetime('now', '-24 hours')",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let offender_rows = sqlx::query(
        "SELECT src_ip, COUNT(*) as cnt FROM alert_events
         WHERE alert_type = 'ml_anomaly'
         GROUP BY src_ip ORDER BY cnt DESC LIMIT 10",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let top_offenders: Vec<TopOffender> = offender_rows
        .iter()
        .map(|r| TopOffender {
            src_ip: r.try_get("src_ip").unwrap_or_default(),
            count: r.try_get("cnt").unwrap_or(0),
        })
        .collect();

    let warning = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE alert_type = 'ml_anomaly' AND severity = 'warning'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let critical = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events
         WHERE alert_type = 'ml_anomaly' AND severity = 'critical'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(MlStats {
        total_ml_anomalies: total,
        anomalies_last_24h: last_24h,
        top_offenders,
        severity_breakdown: SeverityBreakdown { warning, critical },
    }))
}

/// GET /api/ml/events — ml_anomaly events paginated, with explanation
pub async fn get_ml_events(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let offset: i64 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // Server-side sort over a whitelist (task 17.3 R-05)
    let col = match params.get("sort").map(String::as_str) {
        Some("exporter_ip") => "exporter_ip",
        Some("src_ip") => "src_ip",
        Some("severity") => "CASE severity WHEN 'critical' THEN 2 ELSE 1 END",
        Some("pps") => "pps",
        _ => "id",
    };
    let dir = if params.get("dir").map(String::as_str) == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events WHERE alert_type = 'ml_anomaly'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query(&format!(
        "SELECT id, exporter_ip, src_ip, severity, message,
                pps, avg_pkt_bytes, upload_bytes, download_bytes,
                explanation, feedback, created_at,
                explanation_attempts, explanation_error
         FROM alert_events
         WHERE alert_type = 'ml_anomaly'
         ORDER BY {col} {dir} NULLS LAST, id {dir}
         LIMIT ? OFFSET ?"
    ))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (llm_enabled, _) = crate::llm::llm_settings(&state.db).await;
    let events: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let explanation = r
                .try_get::<Option<String>, _>("explanation")
                .ok()
                .flatten()
                .filter(|s| !s.is_empty());
            let attempts = r.try_get::<i64, _>("explanation_attempts").unwrap_or(0);
            serde_json::json!({
                "explanation_status": crate::llm::explanation_status(explanation.is_some(), attempts, llm_enabled),
                "explanation_error": r.try_get::<Option<String>,_>("explanation_error").ok().flatten(),
                "id":            r.try_get::<i64,_>("id").ok(),
                "exporter_ip":   r.try_get::<String,_>("exporter_ip").unwrap_or_default(),
                "src_ip":        r.try_get::<String,_>("src_ip").unwrap_or_default(),
                "severity":      r.try_get::<String,_>("severity").unwrap_or_default(),
                "message":       r.try_get::<String,_>("message").unwrap_or_default(),
                "pps":           r.try_get::<f64,_>("pps").ok(),
                "avg_pkt_bytes": r.try_get::<f64,_>("avg_pkt_bytes").ok(),
                "upload_bytes":  r.try_get::<i64,_>("upload_bytes").ok(),
                "download_bytes":r.try_get::<i64,_>("download_bytes").ok(),
                "explanation":   explanation,
                "feedback":      r.try_get::<Option<String>,_>("feedback").ok().flatten(),
                "created_at":    r.try_get::<Option<String>,_>("created_at").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(
        serde_json::json!({ "total": total, "events": events }),
    ))
}

#[derive(serde::Deserialize)]
pub struct FeedbackBody {
    /// "false_positive" | "confirmed" | null (limpa)
    pub feedback: Option<String>,
}

/// PATCH /api/ml/events/:id/feedback — feedback do operador (task 15.2).
/// Falso positivo eleva o threshold daquele IP no detector (ml_runner).
pub async fn set_event_feedback(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(body): Json<FeedbackBody>,
) -> Result<StatusCode, StatusCode> {
    match body.feedback.as_deref() {
        None | Some("false_positive") | Some("confirmed") => {}
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
    let res = sqlx::query("UPDATE alert_events SET feedback = ? WHERE id = ?")
        .bind(&body.feedback)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
