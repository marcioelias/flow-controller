use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;

use crate::auth::AppState;
use crate::ml_runner::{MlModelStatus, SharedMlStatus};

#[derive(Serialize)]
pub struct MlStatus {
    pub llm_enabled: bool,
    pub llm_model:   String,
    pub exporters:   Vec<ExporterStatusOut>,
}

#[derive(Serialize)]
pub struct ExporterStatusOut {
    pub exporter_ip:       String,
    pub status:            &'static str,
    pub samples_collected: usize,
    pub samples_needed:    usize,
    pub n_scored:          u64,
    pub anomalies_total:   i64,
}

#[derive(Serialize)]
pub struct MlStats {
    pub total_ml_anomalies: i64,
    pub anomalies_last_24h: i64,
    pub top_offenders:      Vec<TopOffender>,
    pub severity_breakdown: SeverityBreakdown,
}

#[derive(Serialize)]
pub struct TopOffender {
    pub src_ip: String,
    pub count:  i64,
}

#[derive(Serialize)]
pub struct SeverityBreakdown {
    pub warning:  i64,
    pub critical: i64,
}

pub async fn get_ml_status(
    State(state): State<Arc<AppState>>,
    axum::Extension(shared_status): axum::Extension<SharedMlStatus>,
) -> Result<Json<MlStatus>, StatusCode> {
    let llm_enabled = std::env::var("LLM_ENABLED").as_deref() == Ok("true");
    let llm_model   = std::env::var("LLM_MODEL").unwrap_or_else(|_| "—".to_string());

    let snapshot: Vec<MlModelStatus> = shared_status
        .read()
        .map(|g| g.clone())
        .unwrap_or_default();

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
            exporter_ip:       s.exporter_ip,
            status:            s.status,
            samples_collected: s.samples_collected,
            samples_needed:    s.samples_needed,
            n_scored:          s.n_scored,
            anomalies_total,
        });
    }

    Ok(Json(MlStatus {
        llm_enabled,
        llm_model,
        exporters: exporters_out,
    }))
}

pub async fn get_ml_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MlStats>, StatusCode> {
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
            count:  r.try_get("cnt").unwrap_or(0),
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
    let limit:  i64 = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let offset: i64 = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM alert_events WHERE alert_type = 'ml_anomaly'",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query(
        "SELECT id, exporter_ip, src_ip, severity, message,
                pps, avg_pkt_bytes, upload_bytes, download_bytes,
                explanation, created_at
         FROM alert_events
         WHERE alert_type = 'ml_anomaly'
         ORDER BY id DESC
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let events: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id":            r.try_get::<i64,_>("id").ok(),
                "exporter_ip":   r.try_get::<String,_>("exporter_ip").unwrap_or_default(),
                "src_ip":        r.try_get::<String,_>("src_ip").unwrap_or_default(),
                "severity":      r.try_get::<String,_>("severity").unwrap_or_default(),
                "message":       r.try_get::<String,_>("message").unwrap_or_default(),
                "pps":           r.try_get::<f64,_>("pps").ok(),
                "avg_pkt_bytes": r.try_get::<f64,_>("avg_pkt_bytes").ok(),
                "upload_bytes":  r.try_get::<i64,_>("upload_bytes").ok(),
                "download_bytes":r.try_get::<i64,_>("download_bytes").ok(),
                "explanation":   r.try_get::<Option<String>,_>("explanation").ok().flatten(),
                "created_at":    r.try_get::<Option<String>,_>("created_at").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "total": total, "events": events })))
}
