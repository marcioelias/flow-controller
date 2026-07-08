use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::sync::Arc;

// ─── Domain Structs ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpPeer {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub neighbor_ip: String,
    pub local_ip: String,
    pub local_as: i64,
    pub peer_as: i64,
    pub hold_time: i64,
    pub md5_password: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpCommunity {
    pub id: i64,
    pub name: String,
    pub community: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpPrefix {
    pub id: i64,
    pub prefix: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpAnnouncement {
    pub id: i64,
    pub prefix: String,
    pub next_hop: String,
    pub community_id: Option<i64>,
    pub peer_id: Option<i64>,
    pub origin: String,
    pub origin_detail: Option<String>,
    pub announced_at: String,
    pub withdrawn_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpAnnouncementView {
    pub id: i64,
    pub prefix: String,
    pub next_hop: String,
    pub community_id: Option<i64>,
    pub peer_id: Option<i64>,
    pub origin: String,
    pub origin_detail: Option<String>,
    pub announced_at: String,
    pub withdrawn_at: Option<String>,
    pub community_name: Option<String>,
    pub community_value: Option<String>,
    pub peer_name: Option<String>,
    pub peer_neighbor_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BgpSessionState {
    pub peer_id: i64,
    pub peer_name: String,
    pub neighbor_ip: String,
    pub state: String,
    pub last_up: Option<String>,
    pub last_down: Option<String>,
    pub updated_at: String,
}

// ─── HTTP Request / Response Types ─────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PeerResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub neighbor_ip: String,
    pub local_ip: String,
    pub local_as: i64,
    pub peer_as: i64,
    pub hold_time: i64,
    pub has_md5: bool,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<BgpPeer> for PeerResponse {
    fn from(p: BgpPeer) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            neighbor_ip: p.neighbor_ip,
            local_ip: p.local_ip,
            local_as: p.local_as,
            peer_as: p.peer_as,
            hold_time: p.hold_time,
            has_md5: p.md5_password.is_some(),
            enabled: p.enabled,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreatePeerRequest {
    pub name: String,
    pub description: Option<String>,
    pub neighbor_ip: String,
    pub local_ip: String,
    pub local_as: i64,
    pub peer_as: i64,
    pub hold_time: Option<i64>,
    pub md5_password: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePeerRequest {
    pub name: String,
    pub description: Option<String>,
    pub neighbor_ip: String,
    pub local_ip: String,
    pub local_as: i64,
    pub peer_as: i64,
    pub hold_time: i64,
    pub md5_password: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommunityRequest {
    pub name: String,
    pub community: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommunityRequest {
    pub name: String,
    pub community: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePrefixRequest {
    pub prefix: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePrefixRequest {
    pub prefix: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnnouncementsQuery {
    pub active: Option<bool>,
}

// ─── Table Initialization ───────────────────────────────────────────────────

pub async fn init_bgp_tables(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS bgp_peers (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            description TEXT,
            neighbor_ip TEXT NOT NULL UNIQUE,
            local_ip    TEXT NOT NULL,
            local_as    INTEGER NOT NULL,
            peer_as     INTEGER NOT NULL,
            hold_time   INTEGER NOT NULL DEFAULT 90,
            md5_password TEXT,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS bgp_communities (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            community   TEXT NOT NULL UNIQUE,
            description TEXT,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS bgp_prefixes (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            prefix      TEXT NOT NULL UNIQUE,
            description TEXT,
            created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS bgp_announcements (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            prefix       TEXT NOT NULL,
            next_hop     TEXT NOT NULL,
            community_id INTEGER REFERENCES bgp_communities(id) ON DELETE SET NULL,
            peer_id      INTEGER REFERENCES bgp_peers(id) ON DELETE CASCADE,
            origin       TEXT NOT NULL DEFAULT 'manual',
            origin_detail TEXT,
            announced_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            withdrawn_at DATETIME
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS bgp_sessions (
            peer_id    INTEGER PRIMARY KEY REFERENCES bgp_peers(id) ON DELETE CASCADE,
            state      TEXT NOT NULL DEFAULT 'unknown',
            last_up    DATETIME,
            last_down  DATETIME,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ─── Peer Handlers ─────────────────────────────────────────────────────────

pub async fn list_peers_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<PeerResponse>>, StatusCode> {
    let peers: Vec<BgpPeer> = sqlx::query_as("SELECT * FROM bgp_peers ORDER BY id")
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("list_peers error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(peers.into_iter().map(PeerResponse::from).collect()))
}

pub async fn create_peer_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(payload): Json<CreatePeerRequest>,
) -> Result<(StatusCode, Json<PeerResponse>), StatusCode> {
    let hold_time = payload.hold_time.unwrap_or(90);
    let enabled = payload.enabled.unwrap_or(true);

    let result = sqlx::query(
        "INSERT INTO bgp_peers \
         (name, description, neighbor_ip, local_ip, local_as, peer_as, hold_time, md5_password, enabled) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.neighbor_ip)
    .bind(&payload.local_ip)
    .bind(payload.local_as)
    .bind(payload.peer_as)
    .bind(hold_time)
    .bind(&payload.md5_password)
    .bind(enabled)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            StatusCode::CONFLICT
        } else {
            tracing::error!("create_peer error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let id = result.last_insert_rowid();
    let peer: BgpPeer = sqlx::query_as("SELECT * FROM bgp_peers WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(peer.into())))
}

pub async fn update_peer_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePeerRequest>,
) -> Result<Json<PeerResponse>, StatusCode> {
    sqlx::query(
        "UPDATE bgp_peers SET \
         name=?, description=?, neighbor_ip=?, local_ip=?, local_as=?, peer_as=?, \
         hold_time=?, md5_password=?, enabled=?, updated_at=datetime('now') \
         WHERE id=?",
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.neighbor_ip)
    .bind(&payload.local_ip)
    .bind(payload.local_as)
    .bind(payload.peer_as)
    .bind(payload.hold_time)
    .bind(&payload.md5_password)
    .bind(payload.enabled)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            StatusCode::CONFLICT
        } else {
            tracing::error!("update_peer error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let peer: BgpPeer = sqlx::query_as("SELECT * FROM bgp_peers WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(peer.into()))
}

pub async fn delete_peer_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    // Check for active announcements
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bgp_announcements WHERE peer_id = ? AND withdrawn_at IS NULL",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if active > 0 {
        return Err(StatusCode::CONFLICT);
    }

    let result = sqlx::query("DELETE FROM bgp_peers WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ─── Community Handlers ─────────────────────────────────────────────────────

pub async fn list_communities_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<BgpCommunity>>, StatusCode> {
    let rows: Vec<BgpCommunity> =
        sqlx::query_as("SELECT * FROM bgp_communities ORDER BY id")
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("list_communities error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(Json(rows))
}

pub async fn create_community_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(payload): Json<CreateCommunityRequest>,
) -> Result<(StatusCode, Json<BgpCommunity>), StatusCode> {
    let result = sqlx::query(
        "INSERT INTO bgp_communities (name, community, description) VALUES (?, ?, ?)",
    )
    .bind(&payload.name)
    .bind(&payload.community)
    .bind(&payload.description)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            StatusCode::CONFLICT
        } else {
            tracing::error!("create_community error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let id = result.last_insert_rowid();
    let row: BgpCommunity = sqlx::query_as("SELECT * FROM bgp_communities WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update_community_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateCommunityRequest>,
) -> Result<Json<BgpCommunity>, StatusCode> {
    sqlx::query(
        "UPDATE bgp_communities SET name=?, community=?, description=? WHERE id=?",
    )
    .bind(&payload.name)
    .bind(&payload.community)
    .bind(&payload.description)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            StatusCode::CONFLICT
        } else {
            tracing::error!("update_community error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let row: BgpCommunity = sqlx::query_as("SELECT * FROM bgp_communities WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_community_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bgp_announcements WHERE community_id = ? AND withdrawn_at IS NULL",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if active > 0 {
        return Err(StatusCode::CONFLICT);
    }

    let result = sqlx::query("DELETE FROM bgp_communities WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ─── Prefix Handlers ────────────────────────────────────────────────────────

pub async fn list_prefixes_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<BgpPrefix>>, StatusCode> {
    let rows: Vec<BgpPrefix> =
        sqlx::query_as("SELECT * FROM bgp_prefixes ORDER BY id")
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("list_prefixes error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(Json(rows))
}

pub async fn create_prefix_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(payload): Json<CreatePrefixRequest>,
) -> Result<(StatusCode, Json<BgpPrefix>), StatusCode> {
    // Basic CIDR validation
    if !payload.prefix.contains('/') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = sqlx::query(
        "INSERT INTO bgp_prefixes (prefix, description) VALUES (?, ?)",
    )
    .bind(&payload.prefix)
    .bind(&payload.description)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            StatusCode::CONFLICT
        } else {
            tracing::error!("create_prefix error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let id = result.last_insert_rowid();
    let row: BgpPrefix = sqlx::query_as("SELECT * FROM bgp_prefixes WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update_prefix_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePrefixRequest>,
) -> Result<Json<BgpPrefix>, StatusCode> {
    if !payload.prefix.contains('/') {
        return Err(StatusCode::BAD_REQUEST);
    }

    sqlx::query("UPDATE bgp_prefixes SET prefix=?, description=? WHERE id=?")
        .bind(&payload.prefix)
        .bind(&payload.description)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                StatusCode::CONFLICT
            } else {
                tracing::error!("update_prefix error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    let row: BgpPrefix = sqlx::query_as("SELECT * FROM bgp_prefixes WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_prefix_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM bgp_prefixes WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ─── Announcement Handlers ──────────────────────────────────────────────────

pub async fn list_announcements_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<AnnouncementsQuery>,
) -> Result<Json<Vec<BgpAnnouncementView>>, StatusCode> {
    let active = params.active.unwrap_or(true);

    let base = "SELECT ba.id, ba.prefix, ba.next_hop, ba.community_id, ba.peer_id, \
                ba.origin, ba.origin_detail, ba.announced_at, ba.withdrawn_at, \
                bc.name as community_name, bc.community as community_value, \
                bp.name as peer_name, bp.neighbor_ip as peer_neighbor_ip \
                FROM bgp_announcements ba \
                LEFT JOIN bgp_communities bc ON bc.id = ba.community_id \
                LEFT JOIN bgp_peers bp ON bp.id = ba.peer_id";

    let sql = if active {
        format!("{} WHERE ba.withdrawn_at IS NULL ORDER BY ba.announced_at DESC", base)
    } else {
        format!("{} ORDER BY ba.announced_at DESC", base)
    };

    let rows: Vec<BgpAnnouncementView> = sqlx::query_as(&sql)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("list_announcements error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(rows))
}

pub async fn announce_route_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Json(payload): Json<crate::bgp_control::AnnounceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let (id, command) =
        crate::bgp_control::announce(&state.db, &state.exabgp_pipe, payload)
            .await
            .map_err(|e| {
                tracing::error!("BGP announce error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "command": command })),
    ))
}

pub async fn withdraw_route_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let command = crate::bgp_control::withdraw(&state.db, &state.exabgp_pipe, id)
        .await
        .map_err(|e| {
            tracing::error!("BGP withdraw error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({ "command": command })))
}

// ─── Apply Config Handler ───────────────────────────────────────────────────

pub async fn apply_config_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let peer_count =
        crate::bgp_config::apply_config(&state.db, &state.exabgp_config_path)
            .await
            .map_err(|e| {
                tracing::error!("apply_config error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    Ok(Json(serde_json::json!({
        "peers_configured": peer_count,
        "config_written": true,
        "reload_signal": true,
    })))
}

// ─── Session Handler ────────────────────────────────────────────────────────

pub async fn get_sessions_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<BgpSessionState>>, StatusCode> {
    let guard = state
        .bgp_sessions
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let sessions: Vec<BgpSessionState> = guard.values().cloned().collect();
    Ok(Json(sessions))
}
