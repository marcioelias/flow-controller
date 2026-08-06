use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

pub async fn bgp_session_monitor(
    pool: sqlx::SqlitePool,
    sessions: Arc<std::sync::RwLock<HashMap<String, crate::bgp::BgpSessionState>>>,
    out_pipe_path: String,
) {
    // Sem ExaBGP (dev local) o pipe não existe: backoff exponencial até 60s
    // e log só na transição de estado — warn a cada 5s era puro ruído
    let mut delay_secs: u64 = 5;
    let mut was_failing = false;
    loop {
        match try_monitor(&pool, &sessions, &out_pipe_path).await {
            Ok(()) => {
                tracing::info!("BGP session monitor: pipe EOF, restarting...");
                delay_secs = 5;
                was_failing = false;
            }
            Err(e) => {
                if !was_failing {
                    tracing::warn!(
                        "BGP session monitor unavailable: {} — backing off (up to 60s)",
                        e
                    );
                    was_failing = true;
                }
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                delay_secs = (delay_secs * 2).min(60);
            }
        }
    }
}

async fn try_monitor(
    pool: &sqlx::SqlitePool,
    sessions: &Arc<std::sync::RwLock<HashMap<String, crate::bgp::BgpSessionState>>>,
    path: &str,
) -> anyhow::Result<()> {
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        // Expected: "neighbor <ip> <state>"
        let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
        if parts.len() != 3 || parts[0] != "neighbor" {
            continue;
        }

        let neighbor_ip = parts[1];
        let state_str = parts[2];

        if !matches!(state_str, "up" | "down" | "connected") {
            continue;
        }

        // Look up peer_id from DB
        let peer_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM bgp_peers WHERE neighbor_ip = ?")
                .bind(neighbor_ip)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);

        let peer_id = match peer_id {
            Some(id) => id,
            None => {
                tracing::debug!("BGP session monitor: unknown neighbor {}", neighbor_ip);
                continue;
            }
        };

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        match state_str {
            "up" | "connected" => {
                // Update DB
                let _ = sqlx::query(
                    "UPDATE bgp_sessions SET state='up', last_up=datetime('now'), updated_at=datetime('now') WHERE peer_id = ?",
                )
                .bind(peer_id)
                .execute(pool)
                .await;

                // Update in-memory cache
                if let Ok(mut guard) = sessions.write() {
                    if let Some(entry) = guard.get_mut(neighbor_ip) {
                        entry.state = "up".to_string();
                        entry.last_up = Some(now.clone());
                        entry.updated_at = now;
                    }
                }
            }
            "down" => {
                // Update DB
                let _ = sqlx::query(
                    "UPDATE bgp_sessions SET state='down', last_down=datetime('now'), updated_at=datetime('now') WHERE peer_id = ?",
                )
                .bind(peer_id)
                .execute(pool)
                .await;

                // Update in-memory cache
                if let Ok(mut guard) = sessions.write() {
                    if let Some(entry) = guard.get_mut(neighbor_ip) {
                        entry.state = "down".to_string();
                        entry.last_down = Some(now.clone());
                        entry.updated_at = now;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}
