use axum::{
    body::Body,
    extract::{Multipart, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Backup JSON schema (version 1)
// ---------------------------------------------------------------------------

const BACKUP_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct BackupFile {
    version: u32,
    exported_at: String,
    users: Vec<BackupUser>,
    exporters: Vec<BackupExporter>,
    settings: Vec<BackupSetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
}

#[derive(Serialize, Deserialize, FromRow)]
struct BackupUser {
    username: String,
    password_hash: String,
    is_admin: bool,
    created_at: String,
}

#[derive(Serialize, Deserialize, FromRow)]
struct BackupExporter {
    ip_address: String,
    name: String,
    description: Option<String>,
    location: Option<String>,
    enabled: bool,
}

#[derive(Serialize, Deserialize, FromRow)]
struct BackupSetting {
    key: String,
    value: String,
}

// ---------------------------------------------------------------------------
// GET /api/backup/config
// ---------------------------------------------------------------------------

pub async fn backup_config_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Response, StatusCode> {
    let users: Vec<BackupUser> =
        sqlx::query_as("SELECT username, password_hash, is_admin, created_at FROM users")
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let exporters: Vec<BackupExporter> =
        sqlx::query_as("SELECT ip_address, name, description, location, enabled FROM exporters")
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let settings: Vec<BackupSetting> =
        sqlx::query_as("SELECT key, value FROM settings ORDER BY key")
            .fetch_all(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let license = read_license_file();

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let date_slug = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let backup = BackupFile {
        version: BACKUP_VERSION,
        exported_at: now,
        users,
        exporters,
        settings,
        license,
    };

    let json = serde_json::to_vec_pretty(&backup)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let filename = format!("flow-collector-config-{}.json", date_slug);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from(json))
        .unwrap())
}

fn read_license_file() -> Option<String> {
    std::fs::read_to_string("/etc/flow-collector/license.key")
        .or_else(|_| std::fs::read_to_string("./license.key"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// GET /api/backup/flows/size
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct FlowsSizeEstimate {
    pub v4_rows: u64,
    pub v6_rows: u64,
    pub estimated_bytes_uncompressed: u64,
    pub estimated_bytes_compressed: u64,
    pub warning: String,
}

pub async fn flows_size_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<FlowsSizeEstimate>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let v4_rows = query_count(&client, &state.clickhouse_url, "network_flows_v4").await;
    let v6_rows = query_count(&client, &state.clickhouse_url, "network_flows_v6").await;

    let total_rows = v4_rows + v6_rows;
    // ~100 bytes per row uncompressed NDJSON, ~12 bytes compressed (gzip ~8:1 ratio on structured data)
    let estimated_uncompressed = total_rows * 100;
    let estimated_compressed   = total_rows * 12;

    let gb = estimated_compressed as f64 / 1_000_000_000.0;
    let warning = if gb >= 1.0 {
        format!("O export comprimido tem ~{:.1} GB. O processo pode levar vários minutos.", gb)
    } else {
        let mb = estimated_compressed as f64 / 1_000_000.0;
        format!("O export comprimido tem ~{:.0} MB.", mb)
    };

    Ok(Json(FlowsSizeEstimate {
        v4_rows,
        v6_rows,
        estimated_bytes_uncompressed: estimated_uncompressed,
        estimated_bytes_compressed:   estimated_compressed,
        warning,
    }))
}

async fn query_count(client: &reqwest::Client, url: &str, table: &str) -> u64 {
    let q = format!("SELECT count() FROM {}", table);
    let resp = client
        .get(format!("{}/", url))
        .query(&[("query", &q)])
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => {
            r.text().await.ok().and_then(|t| t.trim().parse::<u64>().ok()).unwrap_or(0)
        }
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// GET /api/backup/flows  (streaming gzip NDJSON)
// ---------------------------------------------------------------------------

pub async fn backup_flows_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Response, StatusCode> {
    let date_slug = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let filename = format!("flows-{}.ndjson", date_slug);

    // Stream v4 + v6 via ClickHouse HTTP with JSONEachRow — concatenated
    let ch_url = state.clickhouse_url.clone();
    let query_v4 = "SELECT * FROM network_flows_v4 FORMAT JSONEachRow".to_string();
    let query_v6 = "SELECT * FROM network_flows_v6 FORMAT JSONEachRow".to_string();

    // Build a combined stream: v4 rows then v6 rows
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stream = async_stream::stream! {
        for query in [query_v4, query_v6] {
            let res = client
                .get(format!("{}/", ch_url))
                .query(&[("query", &query)])
                .send()
                .await;
            match res {
                Ok(r) if r.status().is_success() => {
                    use futures::StreamExt;
                    let mut byte_stream = r.bytes_stream();
                    while let Some(chunk) = byte_stream.next().await {
                        match chunk {
                            Ok(bytes) => yield Ok::<_, std::io::Error>(bytes),
                            Err(e) => {
                                tracing::error!("flows stream error: {}", e);
                                return;
                            }
                        }
                    }
                }
                Ok(r) => {
                    tracing::error!("ClickHouse returned {}", r.status());
                }
                Err(e) => {
                    tracing::error!("ClickHouse request failed: {}", e);
                }
            }
        }
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from_stream(stream))
        .unwrap())
}

// ---------------------------------------------------------------------------
// POST /api/restore/config
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RestoreResult {
    pub users_imported: usize,
    pub users_skipped: usize,
    pub exporters_imported: usize,
    pub exporters_replaced: usize,
    pub settings_imported: usize,
    pub license: String,
}

pub async fn restore_config_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    mut multipart: Multipart,
) -> Result<Json<RestoreResult>, (StatusCode, Json<serde_json::Value>)> {
    let err = |msg: &str| -> (StatusCode, Json<serde_json::Value>) {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg })))
    };

    // Read multipart file field
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            file_bytes = field.bytes().await.ok().map(|b| b.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| err("Campo 'file' ausente"))?;

    let backup: BackupFile = serde_json::from_slice(&bytes)
        .map_err(|e| err(&format!("JSON inválido: {}", e)))?;

    if backup.version != BACKUP_VERSION {
        return Err(err(&format!(
            "Versão incompatível: esperado {}, recebido {}",
            BACKUP_VERSION, backup.version
        )));
    }

    // Restore users — INSERT OR IGNORE (never overwrite existing)
    let mut users_imported = 0;
    let mut users_skipped = 0;
    for u in &backup.users {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO users (username, password_hash, is_admin, created_at) VALUES (?, ?, ?, ?)"
        )
        .bind(&u.username)
        .bind(&u.password_hash)
        .bind(u.is_admin)
        .bind(&u.created_at)
        .execute(&state.db)
        .await
        .map_err(|e| err(&format!("Erro ao importar usuário: {}", e)))?;

        if result.rows_affected() > 0 {
            users_imported += 1;
        } else {
            users_skipped += 1;
        }
    }

    // Restore exporters — INSERT OR REPLACE by ip_address
    let mut exporters_imported = 0;
    let mut exporters_replaced = 0;
    for e in &backup.exporters {
        // Check if exists first to count replaced vs new
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM exporters WHERE ip_address = ?)")
            .bind(&e.ip_address)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false);

        sqlx::query(
            "INSERT OR REPLACE INTO exporters (ip_address, name, description, location, enabled) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&e.ip_address)
        .bind(&e.name)
        .bind(&e.description)
        .bind(&e.location)
        .bind(e.enabled)
        .execute(&state.db)
        .await
        .map_err(|e| err(&format!("Erro ao importar exporter: {}", e)))?;

        if exists { exporters_replaced += 1; } else { exporters_imported += 1; }
    }

    // Restore settings — INSERT OR IGNORE (never overwrite operator values)
    let mut settings_imported = 0;
    for s in &backup.settings {
        let result = sqlx::query(
            "UPDATE settings SET value = ? WHERE key = ? AND value = ''"
        )
        .bind(&s.value)
        .bind(&s.key)
        .execute(&state.db)
        .await
        .map_err(|e| err(&format!("Erro ao importar setting: {}", e)))?;
        if result.rows_affected() > 0 {
            settings_imported += 1;
        }
    }

    // License — save to disk for reference but don't apply (fingerprint mismatch)
    let license_msg = if let Some(lic) = &backup.license {
        let saved = std::fs::write("/etc/flow-collector/license.key.backup", lic)
            .or_else(|_| std::fs::write("./license.key.backup", lic));
        match saved {
            Ok(_) => "Licença salva em license.key.backup — não aplicada (vinculada ao hardware original)".to_string(),
            Err(_) => "Licença não salva (sem permissão de escrita) — não aplicada".to_string(),
        }
    } else {
        "Nenhuma licença no backup".to_string()
    };

    Ok(Json(RestoreResult {
        users_imported,
        users_skipped,
        exporters_imported,
        exporters_replaced,
        settings_imported,
        license: license_msg,
    }))
}
