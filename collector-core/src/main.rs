mod alert_api;
mod alerts;
mod auth;
mod backup;
mod bgp;
mod bgp_config;
mod bgp_control;
mod bgp_session_monitor;
mod detector;
mod features;
mod license;
mod llm;
mod middleware;
mod ml_api;
mod ml_model;
mod ml_runner;
mod exporters;
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type AllowedSet = Arc<DashSet<Ipv4Addr>>;

async fn refresh_whitelist(pool: sqlx::SqlitePool, set: AllowedSet) {
    loop {
        match exporters::fetch_enabled_ips(&pool).await {
            Ok(ips) => {
                set.clear();
                for ip in ips {
                    set.insert(ip);
                }
            }
            Err(e) => tracing::error!("whitelist refresh error: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    http::StatusCode,
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use serde::Serialize;

use aggregator::{AggregatedMetrics, AggregationKey, ThreadLocalAggregator};
use netflow_parser::parse_packet;
use template_cache::ThreadLocalTemplateCache;

#[derive(Serialize, Clone, Debug)]
pub struct LiveFlowStats {
    pub timestamp_sec: u32,
    pub total_bytes: u64,
    pub per_device: std::collections::HashMap<String, u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct DebugFlow {
    pub timestamp_sec: u32,
    pub exporter_ip:   String,
    pub src_ip:        String,
    pub dst_ip:        String,
    pub src_port:      u16,
    pub dst_port:      u16,
    pub protocol:      u8,
    pub bytes:         u64,
    pub packets:       u64,
    pub src_asn:       u32,
    pub dst_asn:       u32,
    pub ingress_if:    u32,
    pub egress_if:     u32,
    pub tcp_flags:     u8,
    pub flow_count:    u64,
}

// Prometheus metrics handler (no auth — scraped externally)
async fn metrics_handler(
    State(state): State<Arc<auth::AppState>>,
) -> impl IntoResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = state.metrics.registry.gather();
    match encoder.encode_to_string(&metric_families) {
        Ok(text) => (StatusCode::OK, [("Content-Type", prometheus::TEXT_FORMAT)], text.into_bytes()),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, [("Content-Type", "text/plain")], Vec::new()),
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
const CARGO_DEPS: &str    = env!("CARGO_DEPS");

async fn version_handler(
    State(state): State<Arc<auth::AppState>>,
) -> axum::Json<serde_json::Value> {
    // Query ClickHouse for its runtime version (best-effort, non-blocking)
    let ch_version = fetch_clickhouse_version(&state.clickhouse_url).await
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
const QUEUE_CAPACITY: usize = 1_000_000;
const FLUSH_INTERVAL_SECS: u64 = 1;

struct PacketPayload {
    pub exporter_ip: std::net::IpAddr,
    pub data: Vec<u8>,
}

fn init_tracing() {
    let json = std::env::var("LOG_FORMAT").map(|v| v == "json").unwrap_or(false);
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
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    
    let clickhouse_url = std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string());
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
    let exabgp_pipe = std::env::var("EXABGP_PIPE_PATH")
        .unwrap_or_else(|_| "/run/exabgp/exabgp.in".to_string());
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
    let worker_count: usize = rt.block_on(settings::get_value(&auth_db, "COLLECTOR_WORKERS"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 64);
    tracing::info!("Collector worker threads: {}", worker_count);

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
            bgp_session_monitor::bgp_session_monitor(
                monitor_pool,
                monitor_sessions,
                monitor_pipe,
            )
            .await;
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
        let ml_state  = app_state.clone();
        let ml_status = ml_shared_status.clone();
        rt.spawn(async move {
            ml_runner::run_ml(ml_state, ml_rx, ml_status).await;
        });
    }

    // Spawn LLM explainer (optional — only when LLM_ENABLED=true in settings or env)
    {
        let llm_pool = auth_db.clone();
        rt.spawn(async move {
            if let Some(llm_client) = llm::LlmClient::from_settings(&llm_pool).await {
                tracing::info!("LLM explainer: using model '{}' at {}", llm_client.model_name(), "configured endpoint");
                llm::run_llm_explainer(llm_pool, llm_client).await;
            }
        });
    }

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
            .route("/api/users/:id", axum::routing::put(auth::update_user_handler))
            .route("/api/users/:id", delete(auth::delete_user_handler))
            .route("/api/exporters", get(exporters::list_exporters_handler))
            .route("/api/exporters", post(exporters::create_exporter_handler))
            .route("/api/exporters/:id", get(exporters::get_exporter_handler))
            .route("/api/exporters/:id", axum::routing::put(exporters::update_exporter_handler))
            .route("/api/exporters/:id", delete(exporters::delete_exporter_handler))
            .route("/api/settings/:key", axum::routing::put(settings::update_setting_handler))
            .route("/api/backup/config", get(backup::backup_config_handler))
            .route("/api/backup/flows/size", get(backup::flows_size_handler))
            .route("/api/backup/flows", get(backup::backup_flows_handler))
            .route("/api/restore/config", post(backup::restore_config_handler))
            // BGP Peers
            .route("/api/bgp/peers", get(bgp::list_peers_handler))
            .route("/api/bgp/peers", post(bgp::create_peer_handler))
            .route("/api/bgp/peers/:id", axum::routing::put(bgp::update_peer_handler))
            .route("/api/bgp/peers/:id", delete(bgp::delete_peer_handler))
            // BGP Communities
            .route("/api/bgp/communities", get(bgp::list_communities_handler))
            .route("/api/bgp/communities", post(bgp::create_community_handler))
            .route("/api/bgp/communities/:id", axum::routing::put(bgp::update_community_handler))
            .route("/api/bgp/communities/:id", delete(bgp::delete_community_handler))
            // BGP Prefixes
            .route("/api/bgp/prefixes", get(bgp::list_prefixes_handler))
            .route("/api/bgp/prefixes", post(bgp::create_prefix_handler))
            .route("/api/bgp/prefixes/:id", axum::routing::put(bgp::update_prefix_handler))
            .route("/api/bgp/prefixes/:id", delete(bgp::delete_prefix_handler))
            // BGP Announcements
            .route("/api/bgp/announcements", get(bgp::list_announcements_handler))
            .route("/api/bgp/announcements", post(bgp::announce_route_handler))
            .route("/api/bgp/announcements/:id", delete(bgp::withdraw_route_handler))
            // BGP Apply & Sessions
            .route("/api/bgp/apply", post(bgp::apply_config_handler))
            .route("/api/bgp/sessions", get(bgp::get_sessions_handler))
            // Alert rules
            .route("/api/alerts/rules", get(alert_api::list_rules).post(alert_api::create_rule))
            .route("/api/alerts/rules/:id", axum::routing::put(alert_api::update_rule).delete(alert_api::delete_rule))
            .route("/api/alerts/rules/:id/toggle", axum::routing::patch(alert_api::toggle_rule))
            .route("/api/alerts/events", get(alert_api::list_events).delete(alert_api::clear_events))
            .route("/api/alerts/telegram", get(alert_api::get_telegram).put(alert_api::update_telegram))
            .route("/api/alerts/telegram/test", post(alert_api::test_telegram))
            .layer(axum_middleware::from_fn(middleware::require_admin));

        // Semi-protected routes (require auth but not admin)
        let user_routes = Router::new()
            .route("/api/settings", get(settings::list_settings_handler))
            .route("/api/exporters/enabled", get(exporters::list_enabled_exporters_handler))
            .route("/api/stats/protocols", get(stats::get_protocol_stats_handler))
            .route("/api/stats/top-talkers", get(stats::get_top_talkers_handler))
            .route("/api/stats/asn", get(stats::get_asn_stats_handler))
            .route("/api/stats/ports", get(stats::get_port_breakdown_handler))
            .route("/api/stats/timeline", get(stats::get_timeline_handler))
            .route("/api/stats/exporters", get(stats::get_exporter_summary_handler))
            .route("/api/system/health", get(system_health::get_health_handler))
            // ML / AI routes
            .route("/api/ml/status", get(ml_api::get_ml_status))
            .route("/api/ml/stats",  get(ml_api::get_ml_stats))
            .route("/api/ml/events", get(ml_api::get_ml_events))
            .layer(axum_middleware::from_fn(middleware::require_auth));

        let app = Router::new()
            .merge(public_routes)
            .merge(protected_routes)
            .merge(user_routes)
            .with_state(app_state_clone.clone())
            .layer(axum::Extension(app_state_clone.ml_status.clone()))
            .layer(cors);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        tracing::info!("WebSocket & API Server listening on 0.0.0.0:3000");
        axum::serve(listener, app).await.unwrap();
    });

    let (export_tx, export_rx) = flume::bounded(100);

    // Spawn async ClickHouse Exporter Task
    let exporter_clone = Arc::clone(&exporter_client);
    let ml_tx_ch = ml_tx.clone();
    rt.spawn(async move {
        tracing::info!("Clickhouse Exporter background task started.");
        while let Ok(map) = export_rx.recv_async().await {
            let mut v4_batch = Vec::new();
            let mut v6_batch = Vec::new();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32;

            let map: std::collections::HashMap<AggregationKey, AggregatedMetrics, ahash::RandomState> = map;

            // Extract ML features before consuming the map
            let ml_features = features::extract(now, &map);
            if !ml_features.is_empty() {
                let _ = ml_tx_ch.try_send(ml_features);
            }

            let mut batch_total_bytes = 0;
            let mut per_device_bytes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

            for (key, metrics) in map {
                batch_total_bytes += metrics.bytes;

                // Aggregate bytes per exporter_ip
                *per_device_bytes.entry(key.exporter_ip.to_string()).or_insert(0) += metrics.bytes;

                match (key.src_ip, key.dst_ip) {
                    (flow_types::IpAddrType::V4(src_ip), flow_types::IpAddrType::V4(dst_ip)) => {
                        v4_batch.push(NetworkFlowV4Row {
                            timestamp: now,
                            exporter_ip: key.exporter_ip.to_string(),
                            src_ip: src_ip.to_string(),
                            dst_ip: dst_ip.to_string(),
                            src_port: key.src_port,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            src_asn: key.src_asn,
                            dst_asn: key.dst_asn,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                        });
                    }
                    (flow_types::IpAddrType::V6(src_ip), flow_types::IpAddrType::V6(dst_ip)) => {
                        v6_batch.push(NetworkFlowV6Row {
                            timestamp: now,
                            exporter_ip: key.exporter_ip.to_string(),
                            src_ip: src_ip.to_string(),
                            dst_ip: dst_ip.to_string(),
                            src_port: key.src_port,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            src_asn: key.src_asn,
                            dst_asn: key.dst_asn,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                        });
                    }
                    _ => {} // Mixed IPs (impossible organically)
                }
            }

            if let Err(e) = exporter_clone.insert_batch(&v4_batch, &v6_batch).await {
                tracing::error!("Failed to insert batch to ClickHouse: {}", e);
            } else {
                tracing::debug!("Inserted batches (v4: {}, v6: {}) to ClickHouse.", v4_batch.len(), v6_batch.len());
            }

            // Broadcast to connected websockets via ignoring send errors (e.g. no clients)
            let _ = ws_tx.send(LiveFlowStats {
                timestamp_sec: now,
                total_bytes: batch_total_bytes,
                per_device: per_device_bytes,
            });
        }
    });

    // Pre-allocate channels for N workers
    let mut worker_senders = Vec::new();
    let mut worker_threads = Vec::new();

    // Spawn Worker Threads
    for worker_id in 0..worker_count {
        let (tx, rx) = flume::bounded(QUEUE_CAPACITY);
        worker_senders.push(tx);

        let exp_tx = export_tx.clone();
        let dbg_tx = debug_tx.clone();
        let worker_metrics = collector_metrics.clone();
        let handle = thread::Builder::new()
            .name(format!("worker-{}", worker_id))
            .spawn(move || worker_loop(worker_id, rx, exp_tx, dbg_tx, worker_metrics))?;

        worker_threads.push(handle);
    }

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    
    if let Err(e) = socket.set_recv_buffer_size(32 * 1024 * 1024) { 
        tracing::warn!("Could not set optimal UDP recv buffer size: {}", e);
    }
    
    let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 2055);
    socket.bind(&addr.into())?;
    let udp_socket: UdpSocket = socket.into();
    
    tracing::info!("Listening for NetFlow/IPFIX on UDP port 2055 with {} workers", worker_count);

    let dropped_packets = Arc::new(AtomicUsize::new(0));
    let blocked_packets = Arc::new(AtomicUsize::new(0));

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
                    let count = blocked_packets.fetch_add(1, Ordering::Relaxed);
                    if count % 1000 == 0 {
                        tracing::warn!("Blocked {} packets from unauthorized exporter: {}", count + 1, src_addr.ip());
                    }
                    continue;
                }

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

                collector_metrics.flows_received.inc();

                if let Err(flume::TrySendError::Full(_)) = worker_senders[worker_idx].try_send(payload) {
                    dropped_packets.fetch_add(1, Ordering::Relaxed);
                    collector_metrics.packets_dropped.inc();
                }
            }
            Err(e) => tracing::error!("UDP recv error: {}", e),
        }
    }
}

fn worker_loop(
    id: usize,
    rx: Receiver<PacketPayload>,
    export_tx: Sender<std::collections::HashMap<AggregationKey, AggregatedMetrics, ahash::RandomState>>,
    debug_tx: broadcast::Sender<DebugFlow>,
    worker_metrics: Arc<metrics::CollectorMetrics>,
) {
    tracing::info!("Worker {} started", id);

    let mut templates = ThreadLocalTemplateCache::new();
    let mut aggregator = ThreadLocalAggregator::new();
    let mut last_flush = Instant::now();

    while let Ok(payload) = rx.recv() {
        match parse_packet(&payload.data, &mut templates, payload.exporter_ip) {
            Ok(parsed_flows) => {
                worker_metrics.flows_decoded.inc_by(parsed_flows.len() as u64);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u32;

                for flow in parsed_flows {
                    // Emit to debug channel before aggregation (send = never blocks workers)
                    if debug_tx.receiver_count() > 0 {
                        let src_ip_str = match flow.src_ip {
                            flow_types::IpAddrType::V4(a) => a.to_string(),
                            flow_types::IpAddrType::V6(a) => a.to_string(),
                        };
                        let dst_ip_str = match flow.dst_ip {
                            flow_types::IpAddrType::V4(a) => a.to_string(),
                            flow_types::IpAddrType::V6(a) => a.to_string(),
                        };
                        let _ = debug_tx.send(DebugFlow {
                            timestamp_sec: now,
                            exporter_ip:   payload.exporter_ip.to_string(),
                            src_ip:        src_ip_str,
                            dst_ip:        dst_ip_str,
                            src_port:      flow.src_port,
                            dst_port:      flow.dst_port,
                            protocol:      flow.protocol,
                            bytes:         flow.bytes,
                            packets:       flow.packets,
                            src_asn:       flow.src_asn,
                            dst_asn:       flow.dst_asn,
                            ingress_if:    flow.ingress_interface,
                            egress_if:     flow.egress_interface,
                            tcp_flags:     flow.tcp_flags,
                            flow_count:    1,
                        });
                    }
                    aggregator.aggregate(&flow);
                }
            }
            Err(_) => {
                worker_metrics.packets_dropped.inc();
            }
        }

        if last_flush.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) {
            worker_metrics.template_cache_size.set(templates.len() as i64);
            let map = aggregator.flush_window();
            if !map.is_empty() {
                let _ = export_tx.try_send(map);
            }
            last_flush = Instant::now();
        }
    }
}
