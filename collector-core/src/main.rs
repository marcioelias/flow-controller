mod alert_api;
mod alerts;
mod auth;
mod backup;
mod bgp;
mod bgp_config;
mod bgp_control;
mod bgp_session_monitor;
mod detector;
mod enforcement;
mod exporters;
mod features;
mod license;
mod llm;
mod middleware;
mod ml_api;
mod ml_model;
mod ml_runner;
mod netclass;
mod settings;
mod stats;
mod system_health;
mod telegram;

const APP_VERSION: &str = env!("APP_VERSION");
const APP_NAME: &str = "FlowVision";
const APP_VENDOR: &str = "Hahn Tech Desenvolvimento e Consultoria Ltda";

use clickhouse_exporter::{ClickhouseExporter, NetworkFlowV4Row, NetworkFlowV6Row};
use dashmap::DashSet;
use flume::{Receiver, Sender};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type AllowedSet = Arc<DashSet<Ipv4Addr>>;

async fn refresh_whitelist(pool: sqlx::SqlitePool, set: AllowedSet) {
    loop {
        match exporters::fetch_enabled_ips(&pool).await {
            Ok(ips) => {
                // Diff, nunca clear+insert: entre o clear e o primeiro insert
                // os receptores viam o set vazio e bloqueavam exporter válido
                // por alguns micros a cada 30s
                let fresh: std::collections::HashSet<Ipv4Addr> = ips.into_iter().collect();
                for ip in &fresh {
                    set.insert(*ip);
                }
                set.retain(|ip| fresh.contains(ip));
            }
            Err(e) => tracing::error!("whitelist refresh error: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use serde::Serialize;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use aggregator::{AggregatedMetrics, AggregationKey, ThreadLocalAggregator};
use netflow_parser::parse_packet;
use rust_embed::RustEmbed;
use template_cache::ThreadLocalTemplateCache;

/// Volume de um segundo específico dentro da janela drenada — permite ao
/// frontend preencher o gráfico retroativamente com a taxa real (task 13.6)
#[derive(Serialize, Clone, Debug)]
pub struct LiveSlice {
    pub sec: u32,
    pub v4_in: u64,
    pub v4_out: u64,
    pub v6_in: u64,
    pub v6_out: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct LiveFlowStats {
    pub timestamp_sec: u32,
    pub total_bytes: u64,
    /// Bytes with direction = ingress; flows sem IE 61 caem aqui por convenção
    pub bytes_in: u64,
    /// Bytes with direction = egress
    pub bytes_out: u64,
    pub per_device: std::collections::HashMap<String, u64>,
    /// Split por exporter para o espelho por dispositivo no dashboard
    pub per_device_in: std::collections::HashMap<String, u64>,
    pub per_device_out: std::collections::HashMap<String, u64>,
    /// Fatias por segundo (globais) — taxa real, não arrival
    pub slices: Vec<LiveSlice>,
}

#[derive(Serialize, Clone, Debug)]
pub struct DebugFlow {
    pub timestamp_sec: u32,
    pub exporter_ip: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub bytes: u64,
    pub packets: u64,
    pub src_asn: u32,
    pub dst_asn: u32,
    pub ingress_if: u32,
    pub egress_if: u32,
    pub tcp_flags: u8,
    pub flow_count: u64,
    /// 0 = ingress, 1 = egress, 255 = not reported (IE 61)
    pub direction: u8,
    /// Post-NAT fields when the exporter sends them (task 17.8)
    pub nat_src_ip: Option<String>,
    pub nat_dst_ip: Option<String>,
    pub nat_src_port: u16,
    pub nat_dst_port: u16,
}

fn ip_type_to_string(ip: flow_types::IpAddrType) -> String {
    match ip {
        flow_types::IpAddrType::V4(a) => a.to_string(),
        flow_types::IpAddrType::V6(a) => a.to_string(),
    }
}

fn nat_v4(ip: Option<flow_types::IpAddrType>) -> u32 {
    match ip {
        Some(flow_types::IpAddrType::V4(a)) => u32::from(a),
        _ => 0,
    }
}

fn nat_v6(ip: Option<flow_types::IpAddrType>) -> [u8; 16] {
    match ip {
        Some(flow_types::IpAddrType::V6(a)) => a.octets(),
        _ => [0; 16],
    }
}

// ---------------------------------------------------------------------------
// SPA embutida (task 16.3): o binário serve o dashboard — sem nginx.
// Em release os arquivos do dist/ viram bytes do executável; em debug o
// rust-embed lê do disco, então o dev com vite segue igual.
// ---------------------------------------------------------------------------

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
struct UiAssets;

async fn spa_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::response::IntoResponse;

    let path = uri.path().trim_start_matches('/');
    // Rota de API desconhecida não deve virar index.html
    if path.starts_with("api/") || path.starts_with("ws") || path == "metrics" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    // SPA fallback: rota do router Vue cai no index.html
    let (name, asset) = match UiAssets::get(if path.is_empty() { "index.html" } else { path }) {
        Some(a) => (if path.is_empty() { "index.html" } else { path }, Some(a)),
        None => ("index.html", UiAssets::get("index.html")),
    };

    match asset {
        Some(content) => {
            let mime = mime_guess::from_path(name).first_or_octet_stream();
            (
                StatusCode::OK,
                [("Content-Type", mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "ui not bundled").into_response(),
    }
}

// Prometheus metrics handler (no auth — scraped externally)
async fn metrics_handler(State(state): State<Arc<auth::AppState>>) -> impl IntoResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = state.metrics.registry.gather();
    match encoder.encode_to_string(&metric_families) {
        Ok(text) => (
            StatusCode::OK,
            [("Content-Type", prometheus::TEXT_FORMAT)],
            text.into_bytes(),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("Content-Type", "text/plain")],
            Vec::new(),
        ),
    }
}

// Websocket Handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<auth::AppState>>,
) -> impl IntoResponse {
    let ws_tx = state.ws_tx.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, ws_tx))
}

async fn handle_socket(mut socket: WebSocket, tx: broadcast::Sender<LiveFlowStats>) {
    let mut rx = tx.subscribe();
    while let Ok(stats) = rx.recv().await {
        if let Ok(msg) = serde_json::to_string(&stats) {
            if socket.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    }
}

// Debug WebSocket: streams individual parsed flows, optional src_ip filter
async fn debug_ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<auth::AppState>>,
) -> impl IntoResponse {
    let src_ip_filter = params.get("src_ip").cloned();
    let debug_tx = state.debug_tx.clone();
    ws.on_upgrade(move |socket| handle_debug_socket(socket, debug_tx, src_ip_filter))
}

async fn handle_debug_socket(
    mut socket: WebSocket,
    tx: broadcast::Sender<DebugFlow>,
    src_ip_filter: Option<String>,
) {
    let mut rx = tx.subscribe();
    // Rate limit: min 10ms between sends
    let mut last_send = tokio::time::Instant::now();

    loop {
        // Drop lagging messages to avoid memory buildup on slow clients
        let flow = loop {
            match rx.recv().await {
                Ok(f) => break f,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("debug ws: dropped {} lagged flows", n);
                    continue;
                }
                Err(_) => return,
            }
        };

        // NOTE: the console is a sampled view, not a capture — workers emit at
        // most one flow per 10ms, so a filtered src_ip may appear sparsely.
        if let Some(ref filter) = src_ip_filter {
            if &flow.src_ip != filter {
                continue;
            }
        }

        // Rate limit
        let now = tokio::time::Instant::now();
        if now.duration_since(last_send).as_millis() < 10 {
            continue;
        }
        last_send = now;

        if let Ok(msg) = serde_json::to_string(&flow) {
            if socket.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// License API handlers
// ---------------------------------------------------------------------------

/// GET /api/license — returns the current LicenseStatus (public, no auth)
async fn get_license_handler(
    State(state): State<Arc<auth::AppState>>,
) -> axum::Json<license::LicenseStatus> {
    let status = state.license.read().unwrap().clone();
    axum::Json(status)
}

#[derive(serde::Deserialize)]
struct ApplyLicenseRequest {
    license: String,
}

/// POST /api/license — validates and applies a new license string (public, no auth)
async fn post_license_handler(
    State(state): State<Arc<auth::AppState>>,
    axum::Json(payload): axum::Json<ApplyLicenseRequest>,
) -> Result<axum::Json<license::LicenseStatus>, StatusCode> {
    let new_status = license::validate_license_string(&payload.license);

    if !new_status.valid {
        // Return the status (with error message) as 400 so the UI can display it
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Try to persist to /etc/flow-collector/license.key, fall back to ./license.key
    let saved = try_save_license("/etc/flow-collector/license.key", &payload.license)
        .or_else(|_| try_save_license("./license.key", &payload.license));

    if let Err(e) = saved {
        tracing::warn!("Could not persist license file: {}", e);
        // Non-fatal — still apply in memory
    }

    // Update in-memory state
    {
        let mut guard = state.license.write().unwrap();
        *guard = new_status.clone();
    }

    tracing::info!(
        "License applied: {} ({})",
        new_status.tier_label,
        new_status.licensee.as_deref().unwrap_or("unlicensed")
    );

    Ok(axum::Json(new_status))
}

fn try_save_license(path: &str, content: &str) -> std::io::Result<()> {
    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

// ---------------------------------------------------------------------------

const RUSTC_VERSION: &str = env!("RUSTC_VERSION");
const CARGO_DEPS: &str = env!("CARGO_DEPS");

async fn version_handler(
    State(state): State<Arc<auth::AppState>>,
) -> axum::Json<serde_json::Value> {
    // Query ClickHouse for its runtime version (best-effort, non-blocking)
    let ch_version = fetch_clickhouse_version(&state.clickhouse_url)
        .await
        .unwrap_or_else(|| "unavailable".to_string());

    // Parse CARGO_DEPS="Label=ver,Label=ver,..." into array of objects
    let rust_deps: Vec<serde_json::Value> = CARGO_DEPS
        .split(',')
        .filter(|s| s.contains('='))
        .map(|s| {
            let (k, v) = s.split_once('=').unwrap();
            serde_json::json!({ "name": k, "version": v })
        })
        .collect();

    axum::Json(serde_json::json!({
        "version":    APP_VERSION.trim(),
        "name":       APP_NAME,
        "vendor":     APP_VENDOR,
        "rustc":      RUSTC_VERSION,
        "clickhouse": ch_version,
        "rust_deps":  rust_deps,
    }))
}

async fn fetch_clickhouse_version(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    let resp = client
        .get(format!("{}/", url))
        .query(&[("query", "SELECT version()")])
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        let text = resp.text().await.ok()?;
        Some(text.trim().to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------

const UDP_BUFFER_SIZE: usize = 65536;
// Bounded by count, not bytes: 65k × ~1400 B ≈ 90 MB/worker worst case.
// Shedding early under overload beats growing RSS until the OOM killer acts.
const DEFAULT_QUEUE_CAPACITY: usize = 65_536;
const FLUSH_INTERVAL_SECS: u64 = 1;
// ClickHouse insert cadence: one part per insert, so batch several 1s windows
const EXPORT_FLUSH_SECS: u64 = 5;
const EXPORT_MAX_ROWS: usize = 500_000;
// Well beyond any standard template refresh interval (v9 default is minutes)
const TEMPLATE_MAX_AGE_SECS: u64 = 3600;
// Debug Console is a sampled view: matches the consumer-side rate limit so the
// worker never pays for strings the socket would discard anyway
const DEBUG_MIN_INTERVAL: Duration = Duration::from_millis(10);

struct PacketPayload {
    pub exporter_ip: std::net::IpAddr,
    pub data: Vec<u8>,
}

/// One closed 1-second aggregation window, stamped by the worker at flush time.
/// Carrying the timestamp with the map keeps rows correct even if the window
/// waits in the export queue before being drained.
pub struct FlowWindow {
    pub window_ts: u32,
    pub map: aggregator::SlicedMap,
}

fn unix_now_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

fn init_tracing() {
    let json = std::env::var("LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    if json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_current_span(true)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .compact()
            .init();
    }
}

fn main() -> anyhow::Result<()> {
    init_tracing();
    tracing::info!("Starting High-Performance Flow Collector");

    // Start Tokio runtime for the ClickHouse Exporter
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string());
    let exporter_client = Arc::new(ClickhouseExporter::new(&clickhouse_url));
    rt.block_on(exporter_client.setup_tables())?;
    tracing::info!("Clickhouse tables validated successfully!");

    // Load license (free tier if no file found)
    let lic = license::load_and_validate("/etc/flow-collector/license.key");
    tracing::info!(
        "License: {} ({})",
        lic.tier_label,
        lic.licensee.as_deref().unwrap_or("unlicensed")
    );

    // Initialize authentication database
    let auth_db = rt.block_on(auth::init_db())?;

    // Initialize exporters table
    rt.block_on(exporters::init_exporters_table(&auth_db))?;
    tracing::info!("Exporters table initialized successfully!");

    // Initialize BGP tables
    rt.block_on(bgp::init_bgp_tables(&auth_db))?;
    tracing::info!("BGP tables initialized successfully!");

    // Initialize alert tables
    rt.block_on(alerts::init_tables(&auth_db))?;
    tracing::info!("Alert tables initialized successfully!");

    // Read ExaBGP env vars
    let exabgp_pipe =
        std::env::var("EXABGP_PIPE_PATH").unwrap_or_else(|_| "/run/exabgp/exabgp.in".to_string());
    let exabgp_config_path = std::env::var("EXABGP_CONFIG_PATH")
        .unwrap_or_else(|_| "/run/exabgp-config/exabgp.conf".to_string());

    // Initialize bgp_sessions rows for existing peers (INSERT OR IGNORE)
    rt.block_on(async {
        let peers: Vec<(i64,)> = sqlx::query_as("SELECT id FROM bgp_peers")
            .fetch_all(&auth_db)
            .await
            .unwrap_or_default();
        for (peer_id,) in peers {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO bgp_sessions (peer_id, state) VALUES (?, 'unknown')",
            )
            .bind(peer_id)
            .execute(&auth_db)
            .await;
        }
    });

    // Load sessions from DB into in-memory HashMap
    let bgp_sessions_map: std::collections::HashMap<String, bgp::BgpSessionState> =
        rt.block_on(async {
            let rows: Vec<bgp::BgpSessionState> = sqlx::query_as(
                "SELECT bs.peer_id, bp.name as peer_name, bp.neighbor_ip, \
                 bs.state, bs.last_up, bs.last_down, bs.updated_at \
                 FROM bgp_sessions bs \
                 JOIN bgp_peers bp ON bp.id = bs.peer_id",
            )
            .fetch_all(&auth_db)
            .await
            .unwrap_or_default();
            rows.into_iter()
                .map(|s| (s.neighbor_ip.clone(), s))
                .collect()
        });
    let bgp_sessions = std::sync::Arc::new(std::sync::RwLock::new(bgp_sessions_map));

    // Initialize settings table
    rt.block_on(settings::init_settings_table(&auth_db))?;
    tracing::info!("Settings table initialized successfully!");

    // Read COLLECTOR_WORKERS from settings (clamp to 1–64)
    let worker_count: usize = rt
        .block_on(settings::get_value(&auth_db, "COLLECTOR_WORKERS"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 64);
    tracing::info!("Collector worker threads: {}", worker_count);

    // Per-worker packet queue depth (count-bounded; see DEFAULT_QUEUE_CAPACITY)
    let queue_capacity: usize = std::env::var("COLLECTOR_QUEUE_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_QUEUE_CAPACITY)
        .clamp(1024, 4_194_304);
    tracing::info!("Worker queue capacity: {} packets", queue_capacity);

    // Build in-memory whitelist cache (populated before UDP socket binds)
    let allowed_set: AllowedSet = Arc::new(DashSet::new());
    let ips = rt.block_on(exporters::fetch_enabled_ips(&auth_db))?;
    for ip in ips {
        allowed_set.insert(ip);
    }
    tracing::info!("Whitelist cache loaded ({} entries)", allowed_set.len());

    // Background refresh every 30 seconds
    let refresh_pool = auth_db.clone();
    let refresh_set = allowed_set.clone();
    rt.spawn(async move { refresh_whitelist(refresh_pool, refresh_set).await });

    // Setup broadcast channel for WebSockets
    let (ws_tx, _) = broadcast::channel::<LiveFlowStats>(100);

    // Debug channel: individual parsed flows — capacity 2048, drops on full (non-critical)
    let (debug_tx, _) = broadcast::channel::<DebugFlow>(2048);

    let collector_metrics = Arc::new(metrics::CollectorMetrics::new());

    let ml_shared_status = ml_runner::new_shared_status();

    let app_state = Arc::new(auth::AppState {
        db: auth_db.clone(),
        ws_tx: ws_tx.clone(),
        debug_tx: debug_tx.clone(),
        metrics: collector_metrics.clone(),
        license: Arc::new(std::sync::RwLock::new(lic)),
        clickhouse_url: clickhouse_url.clone(),
        bgp_sessions: bgp_sessions.clone(),
        exabgp_pipe: exabgp_pipe.clone(),
        exabgp_config_path: exabgp_config_path.clone(),
        ml_status: ml_shared_status.clone(),
    });

    // Spawn BGP session monitor
    {
        let monitor_pool = auth_db.clone();
        let monitor_sessions = bgp_sessions.clone();
        let monitor_pipe = std::env::var("EXABGP_OUT_PIPE_PATH")
            .unwrap_or_else(|_| "/run/exabgp/exabgp.out".to_string());
        rt.spawn(async move {
            bgp_session_monitor::bgp_session_monitor(monitor_pool, monitor_sessions, monitor_pipe)
                .await;
        });
    }

    // Spawn license enforcer (task 14.1)
    {
        let enf_state = app_state.clone();
        rt.spawn(async move {
            enforcement::run_enforcer(enf_state).await;
        });
    }

    // Spawn anomaly detector
    {
        let det_state = app_state.clone();
        rt.spawn(async move {
            detector::run_detector(det_state).await;
        });
    }

    // Spawn Telegram notifier
    {
        let tg_state = app_state.clone();
        rt.spawn(async move {
            telegram::run_notifier(tg_state).await;
        });
    }

    // Spawn ML anomaly detector
    let (ml_tx, ml_rx) = flume::bounded::<Vec<flow_types::FlowFeatures>>(100);
    {
        let ml_state = app_state.clone();
        let ml_status = ml_shared_status.clone();
        rt.spawn(async move {
            ml_runner::run_ml(ml_state, ml_rx, ml_status).await;
        });
    }

    // LLM explainer: always running, idles while LLM_ENABLED is off (task 17.5)
    rt.spawn(llm::run_llm_explainer(auth_db.clone()));

    // Reannounce all active BGP routes at startup
    {
        let reannounce_pool = auth_db.clone();
        let reannounce_pipe = exabgp_pipe.clone();
        rt.spawn(async move {
            match bgp_control::reannounce_all(&reannounce_pool, &reannounce_pipe).await {
                Ok(n) => tracing::info!("BGP startup reannounce: {} routes sent", n),
                Err(e) => tracing::warn!("BGP startup reannounce failed: {}", e),
            }
        });
    }

    // Spawn WebSocket & API Server
    let app_state_clone = app_state.clone();
    rt.spawn(async move {
        // Configure CORS
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Public routes (no auth required)
        let public_routes = Router::new()
            .route("/api/auth/login", post(auth::login_handler))
            .route("/ws", get(ws_handler))
            .route("/ws/debug", get(debug_ws_handler))
            .route("/metrics", get(metrics_handler))
            .route("/api/license", get(get_license_handler))
            .route("/api/license", post(post_license_handler))
            .route("/api/version", get(version_handler));

        // Protected routes (require authentication)
        let protected_routes = Router::new()
            .route("/api/users", get(auth::list_users_handler))
            .route("/api/users", post(auth::create_user_handler))
            .route(
                "/api/users/:id",
                axum::routing::put(auth::update_user_handler),
            )
            .route("/api/users/:id", delete(auth::delete_user_handler))
            .route("/api/exporters", get(exporters::list_exporters_handler))
            .route("/api/exporters", post(exporters::create_exporter_handler))
            .route("/api/exporters/:id", get(exporters::get_exporter_handler))
            .route(
                "/api/exporters/:id",
                axum::routing::put(exporters::update_exporter_handler),
            )
            .route(
                "/api/exporters/:id",
                delete(exporters::delete_exporter_handler),
            )
            .route(
                "/api/settings/:key",
                axum::routing::put(settings::update_setting_handler),
            )
            .route("/api/backup/config", get(backup::backup_config_handler))
            .route("/api/backup/flows/size", get(backup::flows_size_handler))
            .route("/api/backup/flows", get(backup::backup_flows_handler))
            .route("/api/restore/config", post(backup::restore_config_handler))
            // BGP Peers
            .route("/api/bgp/peers", get(bgp::list_peers_handler))
            .route("/api/bgp/peers", post(bgp::create_peer_handler))
            .route(
                "/api/bgp/peers/:id",
                axum::routing::put(bgp::update_peer_handler),
            )
            .route("/api/bgp/peers/:id", delete(bgp::delete_peer_handler))
            // BGP Communities
            .route("/api/bgp/communities", get(bgp::list_communities_handler))
            .route("/api/bgp/communities", post(bgp::create_community_handler))
            .route(
                "/api/bgp/communities/:id",
                axum::routing::put(bgp::update_community_handler),
            )
            .route(
                "/api/bgp/communities/:id",
                delete(bgp::delete_community_handler),
            )
            // BGP Prefixes
            .route("/api/bgp/prefixes", get(bgp::list_prefixes_handler))
            .route("/api/bgp/prefixes", post(bgp::create_prefix_handler))
            .route(
                "/api/bgp/prefixes/:id",
                axum::routing::put(bgp::update_prefix_handler),
            )
            .route("/api/bgp/prefixes/:id", delete(bgp::delete_prefix_handler))
            // BGP Announcements
            .route(
                "/api/bgp/announcements",
                get(bgp::list_announcements_handler),
            )
            .route("/api/bgp/announcements", post(bgp::announce_route_handler))
            .route(
                "/api/bgp/announcements/:id",
                delete(bgp::withdraw_route_handler),
            )
            // BGP Apply & Sessions
            .route("/api/bgp/apply", post(bgp::apply_config_handler))
            .route("/api/bgp/sessions", get(bgp::get_sessions_handler))
            // Alert rules
            .route(
                "/api/alerts/rules",
                get(alert_api::list_rules).post(alert_api::create_rule),
            )
            .route(
                "/api/alerts/rules/:id",
                axum::routing::put(alert_api::update_rule).delete(alert_api::delete_rule),
            )
            .route(
                "/api/alerts/rules/:id/toggle",
                axum::routing::patch(alert_api::toggle_rule),
            )
            .route(
                "/api/alerts/events",
                get(alert_api::list_events).delete(alert_api::clear_events),
            )
            .route("/api/alerts/events/:id", get(alert_api::get_event))
            .route(
                "/api/alerts/events/:id/explain",
                post(alert_api::retry_explanation),
            )
            .route(
                "/api/alerts/telegram",
                get(alert_api::get_telegram).put(alert_api::update_telegram),
            )
            .route("/api/alerts/telegram/test", post(alert_api::test_telegram))
            .route(
                "/api/ml/events/:id/feedback",
                axum::routing::patch(ml_api::set_event_feedback),
            )
            .route("/api/llm/models", post(llm::list_models_handler))
            .route("/api/llm/test", post(llm::test_llm_handler))
            .route_layer(axum_middleware::from_fn(middleware::require_admin));

        // Semi-protected routes (require auth but not admin)
        let user_routes = Router::new()
            .route("/api/settings", get(settings::list_settings_handler))
            .route(
                "/api/exporters/enabled",
                get(exporters::list_enabled_exporters_handler),
            )
            .route(
                "/api/stats/protocols",
                get(stats::get_protocol_stats_handler),
            )
            .route(
                "/api/stats/top-talkers",
                get(stats::get_top_talkers_handler),
            )
            .route("/api/stats/asn", get(stats::get_asn_stats_handler))
            .route("/api/stats/talker", get(stats::get_talker_handler))
            .route("/api/stats/ports", get(stats::get_port_breakdown_handler))
            .route("/api/stats/timeline", get(stats::get_timeline_handler))
            .route(
                "/api/stats/exporters",
                get(stats::get_exporter_summary_handler),
            )
            .route("/api/stats/overview", get(stats::get_overview_handler))
            .route("/api/system/health", get(system_health::get_health_handler))
            // ML / AI routes
            .route("/api/ml/status", get(ml_api::get_ml_status))
            .route("/api/ml/stats", get(ml_api::get_ml_stats))
            .route("/api/ml/events", get(ml_api::get_ml_events))
            .route_layer(axum_middleware::from_fn(middleware::require_auth));

        let app = Router::new()
            .merge(public_routes)
            .merge(protected_routes)
            .merge(user_routes)
            .fallback(spa_handler)
            .with_state(app_state_clone.clone())
            .layer(axum::Extension(app_state_clone.ml_status.clone()))
            .layer(cors);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        tracing::info!("WebSocket & API Server listening on 0.0.0.0:3000");
        axum::serve(listener, app).await.unwrap();
    });

    let (export_tx, export_rx) = flume::bounded::<FlowWindow>(100);

    // Spawn async ClickHouse Exporter Task
    let exporter_clone = Arc::clone(&exporter_client);
    let ml_tx_ch = ml_tx.clone();
    let exporter_metrics = collector_metrics.clone();
    rt.spawn(async move {
        tracing::info!("Clickhouse Exporter background task started.");
        let mut last_ml_drop_warn = Instant::now() - Duration::from_secs(10);

        // ClickHouse wants few, large inserts (one part per insert). Windows
        // are merged here and flushed on a time/size trigger instead of one
        // insert per worker per second. Keyed by (window_ts, key) so rows keep
        // the second they belong to.
        let mut merge: std::collections::HashMap<
            (u32, AggregationKey),
            AggregatedMetrics,
            ahash::RandomState,
        > = std::collections::HashMap::with_hasher(ahash::RandomState::new());
        let mut flush_tick = tokio::time::interval(Duration::from_secs(EXPORT_FLUSH_SECS));
        flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                win = export_rx.recv_async() => {
                    let Ok(window) = win else { break };
                    // Timestamp of the window itself, not of the drain — a window
                    // may sit in the queue for a while when ClickHouse is slow.
                    let now = window.window_ts;
                    // Shared read-only: features and merging both walk it
                    let map = Arc::new(window.map);

                    // Feature extraction is CPU-bound (two full passes over the
                    // map) — run it on the blocking pool so this task stays free.
                    // The ML pipeline expects 1-second windows: per window, not
                    // per merged batch.
                    let feat_map = Arc::clone(&map);
                    let ml_features =
                        tokio::task::spawn_blocking(move || features::extract(&feat_map))
                            .await
                            .unwrap_or_default();
                    if !ml_features.is_empty() && ml_tx_ch.try_send(ml_features).is_err() {
                        exporter_metrics.ml_windows_dropped.inc();
                        if last_ml_drop_warn.elapsed() >= Duration::from_secs(10) {
                            tracing::warn!("ML queue full — dropping feature batch");
                            last_ml_drop_warn = Instant::now();
                        }
                    }

                    // Live dashboard stays at 1s cadence: broadcast per window,
                    // before merging. Sem cliente WS, os mapas/fatias seriam
                    // montados e jogados fora — só os escalares são de graça.
                    let live_wanted = ws_tx.receiver_count() > 0;
                    let mut window_total_bytes = 0;
                    let mut window_bytes_in = 0u64;
                    let mut window_bytes_out = 0u64;
                    let mut per_device_bytes: std::collections::HashMap<String, u64> =
                        std::collections::HashMap::new();
                    let mut per_device_in: std::collections::HashMap<String, u64> =
                        std::collections::HashMap::new();
                    let mut per_device_out: std::collections::HashMap<String, u64> =
                        std::collections::HashMap::new();
                    // [v4_in, v4_out, v6_in, v6_out] — sem direção conta como entrada
                    let mut slice_totals: std::collections::BTreeMap<u32, [u64; 4]> =
                        std::collections::BTreeMap::new();
                    // Which directions each exporter reported this window —
                    // both = totals would double-count if simply summed
                    let mut dir_seen: std::collections::HashMap<Ipv4Addr, (bool, bool)> =
                        std::collections::HashMap::new();
                    for ((slice_sec, key), m) in map.iter() {
                        window_total_bytes += m.bytes;
                        if live_wanted {
                            let t = slice_totals.entry(*slice_sec).or_default();
                            let v6 = matches!(key.src_ip, flow_types::IpAddrType::V6(_));
                            let out = key.direction == flow_types::DIRECTION_EGRESS;
                            t[usize::from(v6) * 2 + usize::from(out)] += m.bytes;
                        }
                        if live_wanted {
                            *per_device_bytes
                                .entry(key.exporter_ip.to_string())
                                .or_insert(0) += m.bytes;
                        }

                        let seen = dir_seen.entry(key.exporter_ip).or_default();
                        match key.direction {
                            flow_types::DIRECTION_INGRESS => {
                                seen.0 = true;
                                window_bytes_in += m.bytes;
                                if live_wanted {
                                    *per_device_in
                                        .entry(key.exporter_ip.to_string())
                                        .or_insert(0) += m.bytes;
                                }
                            }
                            flow_types::DIRECTION_EGRESS => {
                                seen.1 = true;
                                window_bytes_out += m.bytes;
                                if live_wanted {
                                    *per_device_out
                                        .entry(key.exporter_ip.to_string())
                                        .or_insert(0) += m.bytes;
                                }
                            }
                            _ => {
                                window_bytes_in += m.bytes;
                                if live_wanted {
                                    *per_device_in
                                        .entry(key.exporter_ip.to_string())
                                        .or_insert(0) += m.bytes;
                                }
                            }
                        }

                        // Rows keep the second the traffic happened in, not
                        // the drain time — retroactive fill is the point.
                        let e = merge.entry((*slice_sec, *key)).or_default();
                        e.bytes += m.bytes;
                        e.packets += m.packets;
                        e.flow_count += m.flow_count;
                    }
                    for (exp_ip, (ing, egr)) in dir_seen {
                        exporter_metrics
                            .exporter_bidirectional
                            .with_label_values(&[&exp_ip.to_string()])
                            .set((ing && egr) as i64);
                    }
                    let _ = ws_tx.send(LiveFlowStats {
                        timestamp_sec: now,
                        total_bytes: window_total_bytes,
                        bytes_in: window_bytes_in,
                        bytes_out: window_bytes_out,
                        per_device: per_device_bytes,
                        per_device_in,
                        per_device_out,
                        slices: slice_totals
                            .into_iter()
                            .map(|(sec, t)| LiveSlice {
                                sec,
                                v4_in: t[0],
                                v4_out: t[1],
                                v6_in: t[2],
                                v6_out: t[3],
                            })
                            .collect(),
                    });

                    if merge.len() < EXPORT_MAX_ROWS {
                        continue;
                    }
                    // fall through to flush on size
                }
                _ = flush_tick.tick() => {}
            }

            if merge.is_empty() {
                continue;
            }

            let mut v4_batch = Vec::new();
            let mut v6_batch = Vec::new();
            for ((ts, key), metrics) in merge.drain() {
                match (key.src_ip, key.dst_ip) {
                    (flow_types::IpAddrType::V4(src_ip), flow_types::IpAddrType::V4(dst_ip)) => {
                        v4_batch.push(NetworkFlowV4Row {
                            timestamp: ts,
                            exporter_ip: u32::from(key.exporter_ip),
                            src_ip: u32::from(src_ip),
                            dst_ip: u32::from(dst_ip),
                            nat_src_ip: nat_v4(key.nat_src_ip),
                            nat_dst_ip: nat_v4(key.nat_dst_ip),
                            nat_src_port: key.nat_src_port,
                            nat_dst_port: key.nat_dst_port,
                            src_port: key.src_port,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            src_asn: key.src_asn,
                            dst_asn: key.dst_asn,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                            direction: key.direction,
                        });
                    }
                    (flow_types::IpAddrType::V6(src_ip), flow_types::IpAddrType::V6(dst_ip)) => {
                        v6_batch.push(NetworkFlowV6Row {
                            timestamp: ts,
                            exporter_ip: u32::from(key.exporter_ip),
                            src_ip: src_ip.octets(),
                            dst_ip: dst_ip.octets(),
                            nat_src_ip: nat_v6(key.nat_src_ip),
                            nat_dst_ip: nat_v6(key.nat_dst_ip),
                            nat_src_port: key.nat_src_port,
                            nat_dst_port: key.nat_dst_port,
                            src_port: key.src_port,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            src_asn: key.src_asn,
                            dst_asn: key.dst_asn,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                            direction: key.direction,
                        });
                    }
                    _ => {} // Mixed IPs (impossible organically)
                }
            }

            if let Err(e) = exporter_clone
                .insert_batch_with_retry(&v4_batch, &v6_batch, 4)
                .await
            {
                exporter_metrics.clickhouse_insert_errors.inc();
                tracing::error!("Failed to insert batch to ClickHouse: {}", e);
            } else {
                exporter_metrics
                    .clickhouse_rows_inserted
                    .inc_by((v4_batch.len() + v6_batch.len()) as u64);
                tracing::debug!(
                    "Inserted batches (v4: {}, v6: {}) to ClickHouse.",
                    v4_batch.len(),
                    v6_batch.len()
                );
            }
        }
    });

    // Pre-allocate channels for N workers
    let mut worker_senders = Vec::new();
    let mut worker_threads = Vec::new();

    // Spawn Worker Threads
    for worker_id in 0..worker_count {
        let (tx, rx) = flume::bounded(queue_capacity);
        worker_senders.push(tx);

        let exp_tx = export_tx.clone();
        let dbg_tx = debug_tx.clone();
        let worker_metrics = collector_metrics.clone();
        let handle = thread::Builder::new()
            .name(format!("worker-{}", worker_id))
            .spawn(move || worker_loop(worker_id, rx, exp_tx, dbg_tx, worker_metrics))?;

        worker_threads.push(handle);
    }

    // Receiver threads: N sockets bound to the same port via SO_REUSEPORT,
    // kernel load-balances datagrams across them by flow hash.
    let receiver_count: usize = rt
        .block_on(settings::get_value(&auth_db, "COLLECTOR_RECEIVERS"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| worker_count.min(4))
        .clamp(1, 16);

    tracing::info!(
        "Listening for NetFlow/IPFIX on UDP port 2055 with {} receivers and {} workers",
        receiver_count,
        worker_count
    );

    let mut receiver_threads = Vec::new();
    for receiver_id in 0..receiver_count {
        let udp_socket = bind_receiver(2055, 32 * 1024 * 1024, receiver_count > 1)?;
        let recv_allowed = allowed_set.clone();
        let recv_metrics = collector_metrics.clone();
        let recv_senders = worker_senders.clone();
        let handle = thread::Builder::new()
            .name(format!("receiver-{}", receiver_id))
            .spawn(move || {
                receiver_loop(udp_socket, recv_allowed, recv_metrics, recv_senders);
            })?;
        receiver_threads.push(handle);
    }

    // Receivers and workers run forever; park the main thread on them
    for handle in receiver_threads {
        let _ = handle.join();
    }
    Ok(())
}

fn bind_receiver(port: u16, recv_buf: usize, reuse_port: bool) -> anyhow::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    if reuse_port {
        socket.set_reuse_port(true)?;
    }
    if let Err(e) = socket.set_recv_buffer_size(recv_buf) {
        tracing::warn!("Could not set optimal UDP recv buffer size: {}", e);
    }
    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    socket.bind(&addr.into())?;
    Ok(socket.into())
}

fn receiver_loop(
    udp_socket: UdpSocket,
    allowed_set: AllowedSet,
    collector_metrics: Arc<metrics::CollectorMetrics>,
    worker_senders: Vec<Sender<PacketPayload>>,
) {
    let worker_count = worker_senders.len();
    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        match udp_socket.recv_from(&mut buf) {
            Ok((size, src_addr)) => {
                // Zero-copy, zero-async whitelist check via in-memory DashSet
                let is_allowed = match src_addr.ip() {
                    std::net::IpAddr::V4(ipv4) => allowed_set.contains(&ipv4),
                    std::net::IpAddr::V6(_) => false,
                };

                if !is_allowed {
                    let count = collector_metrics.packets_blocked.get();
                    collector_metrics.packets_blocked.inc();
                    if count.is_multiple_of(1000) {
                        tracing::warn!(
                            "Blocked {} packets from unauthorized exporter: {}",
                            count + 1,
                            src_addr.ip()
                        );
                    }
                    continue;
                }

                // Shard by exporter IP: the template cache is thread-local and
                // templates are per exporter, so all packets from one exporter
                // must land on the same worker regardless of which receiver
                // thread picked them up.
                let worker_idx = match src_addr.ip() {
                    std::net::IpAddr::V4(ipv4) => {
                        (u32::from_be_bytes(ipv4.octets()) as usize) % worker_count
                    }
                    std::net::IpAddr::V6(ipv6) => {
                        (u128::from_be_bytes(ipv6.octets()) as usize) % worker_count
                    }
                };

                let payload = PacketPayload {
                    exporter_ip: src_addr.ip(),
                    data: buf[..size].to_vec(),
                };

                collector_metrics.packets_received.inc();

                if let Err(flume::TrySendError::Full(_)) =
                    worker_senders[worker_idx].try_send(payload)
                {
                    collector_metrics.packets_dropped.inc();
                }
            }
            // EINTR is routine; anything else gets a backoff so a broken
            // socket (ENETDOWN, iface removed) can't hot-spin a core while
            // flooding the log.
            Err(e) => match e.kind() {
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock => continue,
                _ => {
                    tracing::error!("UDP recv error: {}", e);
                    thread::sleep(Duration::from_millis(100));
                }
            },
        }
    }
}

fn worker_loop(
    id: usize,
    rx: Receiver<PacketPayload>,
    export_tx: Sender<FlowWindow>,
    debug_tx: broadcast::Sender<DebugFlow>,
    worker_metrics: Arc<metrics::CollectorMetrics>,
) {
    tracing::info!("Worker {} started", id);

    let mut templates = ThreadLocalTemplateCache::new();
    let mut aggregator = ThreadLocalAggregator::new();
    // (soma_ms, n) do atraso flowEnd→chegada por exporter na janela do flush
    let mut lag_acc: std::collections::HashMap<Ipv4Addr, (u64, u32)> =
        std::collections::HashMap::new();
    let mut last_flush = Instant::now();
    let mut last_drop_warn = Instant::now() - Duration::from_secs(10);
    let mut last_debug_send = Instant::now() - DEBUG_MIN_INTERVAL;

    loop {
        // Timed receive so the flush below runs even when no packets arrive —
        // otherwise the last partial window of an idle exporter never ships.
        let received = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(payload) => Some(payload),
            Err(flume::RecvTimeoutError::Timeout) => None,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        };

        if let Some(payload) = received {
            match parse_packet(&payload.data, &mut templates, payload.exporter_ip) {
                Ok(parsed_flows) => {
                    worker_metrics
                        .flows_decoded
                        .inc_by(parsed_flows.len() as u64);
                    let now_secs = unix_now_secs();
                    let now_ms = now_secs as u64 * 1000;

                    for flow in parsed_flows {
                        // Debug Console gets a producer-side sample: without this,
                        // every flow pays 3 String allocs + a broadcast send that
                        // the consumer (rate-limited to 10ms) would throw away —
                        // opening the console degraded collection under load.
                        if debug_tx.receiver_count() > 0
                            && last_debug_send.elapsed() >= DEBUG_MIN_INTERVAL
                        {
                            last_debug_send = Instant::now();
                            let src_ip_str = match flow.src_ip {
                                flow_types::IpAddrType::V4(a) => a.to_string(),
                                flow_types::IpAddrType::V6(a) => a.to_string(),
                            };
                            let dst_ip_str = match flow.dst_ip {
                                flow_types::IpAddrType::V4(a) => a.to_string(),
                                flow_types::IpAddrType::V6(a) => a.to_string(),
                            };
                            let _ = debug_tx.send(DebugFlow {
                                timestamp_sec: unix_now_secs(),
                                exporter_ip: payload.exporter_ip.to_string(),
                                src_ip: src_ip_str,
                                dst_ip: dst_ip_str,
                                src_port: flow.src_port,
                                dst_port: flow.dst_port,
                                protocol: flow.protocol,
                                bytes: flow.bytes,
                                packets: flow.packets,
                                src_asn: flow.src_asn,
                                dst_asn: flow.dst_asn,
                                ingress_if: flow.ingress_interface,
                                egress_if: flow.egress_interface,
                                tcp_flags: flow.tcp_flags,
                                flow_count: 1,
                                direction: flow.direction,
                                nat_src_ip: flow.nat_src_ip.map(ip_type_to_string),
                                nat_dst_ip: flow.nat_dst_ip.map(ip_type_to_string),
                                nat_src_port: flow.nat_src_port,
                                nat_dst_port: flow.nat_dst_port,
                            });
                        }
                        if flow.end_ms > 0 {
                            let (sum, n) = lag_acc.entry(flow.exporter_ip).or_default();
                            *sum += now_ms.saturating_sub(flow.end_ms);
                            *n += 1;
                        }
                        aggregator.aggregate(&flow, now_secs);
                    }
                }
                Err(_) => {
                    worker_metrics.parse_errors.inc();
                }
            }
        }

        if last_flush.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) {
            // Live exporters resend templates periodically, refreshing their
            // insert timestamp — only abandoned entries age out here.
            templates.prune_old_templates(unix_now_secs() as u64, TEMPLATE_MAX_AGE_SECS);
            worker_metrics
                .template_cache_size
                .with_label_values(&[&id.to_string()])
                .set(templates.len() as i64);
            for ((exp_ip, domain), rate) in templates.sampling_rates() {
                worker_metrics
                    .exporter_sampling_rate
                    .with_label_values(&[&exp_ip.to_string(), &domain.to_string()])
                    .set(*rate as i64);
            }
            for (exp, (sum, n)) in lag_acc.drain() {
                worker_metrics
                    .exporter_telemetry_lag
                    .with_label_values(&[&exp.to_string()])
                    .set(sum as f64 / n.max(1) as f64 / 1000.0);
            }
            worker_metrics
                .export_queue_depth
                .set(export_tx.len() as i64);
            let map = aggregator.flush_window();
            if !map.is_empty()
                && export_tx
                    .try_send(FlowWindow {
                        window_ts: unix_now_secs(),
                        map,
                    })
                    .is_err()
            {
                // A full export queue means a whole second of aggregated
                // traffic from this worker is lost — make it visible.
                worker_metrics.export_windows_dropped.inc();
                if last_drop_warn.elapsed() >= Duration::from_secs(10) {
                    tracing::warn!("worker {}: export queue full — dropping window", id);
                    last_drop_warn = Instant::now();
                }
            }
            last_flush = Instant::now();
        }
    }
}
