use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::net::Ipv4Addr;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Exporter {
    pub id: i64,
    pub ip_address: String,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateExporterRequest {
    pub ip_address: String,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateExporterRequest {
    pub ip_address: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ExporterResponse {
    pub id: i64,
    pub ip_address: String,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Exporter> for ExporterResponse {
    fn from(exporter: Exporter) -> Self {
        Self {
            id: exporter.id,
            ip_address: exporter.ip_address,
            name: exporter.name,
            description: exporter.description,
            location: exporter.location,
            enabled: exporter.enabled,
            created_at: exporter.created_at,
            updated_at: exporter.updated_at,
        }
    }
}


// Initialize exporters table
pub async fn init_exporters_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS exporters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ip_address TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            description TEXT,
            location TEXT,
            enabled BOOLEAN NOT NULL DEFAULT 1,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// Fetch all enabled exporter IPs for the in-memory whitelist cache
pub async fn fetch_enabled_ips(pool: &SqlitePool) -> Result<Vec<Ipv4Addr>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT ip_address FROM exporters WHERE enabled = 1"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(ip,)| ip.parse::<Ipv4Addr>().ok())
        .collect())
}

// List all exporters
pub async fn list_exporters_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<ExporterResponse>>, StatusCode> {
    let exporters = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch exporters: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response: Vec<ExporterResponse> = exporters.into_iter().map(Into::into).collect();
    Ok(Json(response))
}

// Get single exporter
pub async fn get_exporter_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ExporterResponse>, StatusCode> {
    let exporter = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch exporter: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(exporter.into()))
}

// Create exporter
pub async fn create_exporter_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(payload): Json<CreateExporterRequest>,
) -> Result<(StatusCode, Json<ExporterResponse>), StatusCode> {
    // Validate IP address format
    if payload.ip_address.parse::<std::net::IpAddr>().is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let enabled = payload.enabled.unwrap_or(true);

    let result = sqlx::query(
        r#"
        INSERT INTO exporters (ip_address, name, description, location, enabled)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&payload.ip_address)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.location)
    .bind(enabled)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create exporter: {}", e);
        if e.to_string().contains("UNIQUE constraint failed") {
            return StatusCode::CONFLICT;
        }
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let id = result.last_insert_rowid();

    let exporter = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters WHERE id = ?"
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(exporter.into())))
}

// Update exporter
pub async fn update_exporter_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateExporterRequest>,
) -> Result<Json<ExporterResponse>, StatusCode> {
    // Validate IP if provided
    if let Some(ref ip) = payload.ip_address {
        if ip.parse::<std::net::IpAddr>().is_err() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Check if exporter exists
    let _existing = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Build update query dynamically
    let mut query = String::from("UPDATE exporters SET updated_at = CURRENT_TIMESTAMP");
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ip) = &payload.ip_address {
        query.push_str(", ip_address = ?");
        bind_values.push(ip.clone());
    }
    if let Some(name) = &payload.name {
        query.push_str(", name = ?");
        bind_values.push(name.clone());
    }
    if let Some(desc) = &payload.description {
        query.push_str(", description = ?");
        bind_values.push(desc.clone());
    }
    if let Some(loc) = &payload.location {
        query.push_str(", location = ?");
        bind_values.push(loc.clone());
    }
    if let Some(enabled) = payload.enabled {
        query.push_str(", enabled = ?");
        bind_values.push(if enabled { "1".to_string() } else { "0".to_string() });
    }

    query.push_str(" WHERE id = ?");

    let mut q = sqlx::query(&query);
    for val in &bind_values {
        q = q.bind(val);
    }
    q = q.bind(id);

    q.execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update exporter: {}", e);
            if e.to_string().contains("UNIQUE constraint failed") {
                return StatusCode::CONFLICT;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let updated = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters WHERE id = ?"
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(updated.into()))
}

// Delete exporter
pub async fn delete_exporter_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM exporters WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete exporter: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// Get list of enabled exporters (for dashboard dropdown)
pub async fn list_enabled_exporters_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<ExporterResponse>>, StatusCode> {
    let exporters = sqlx::query_as::<_, Exporter>(
        "SELECT * FROM exporters WHERE enabled = 1 ORDER BY name ASC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch enabled exporters: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response: Vec<ExporterResponse> = exporters.into_iter().map(Into::into).collect();
    Ok(Json(response))
}
