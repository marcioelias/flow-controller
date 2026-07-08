use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceRequest {
    pub prefix: String,
    pub next_hop: String,
    pub community_id: Option<i64>,
    pub peer_id: Option<i64>,
    pub origin: String,
    pub origin_detail: Option<String>,
}

pub async fn announce(
    pool: &SqlitePool,
    pipe_path: &str,
    req: AnnounceRequest,
) -> anyhow::Result<(i64, String)> {
    // 1. Resolve community
    let community_value: Option<String> = if let Some(cid) = req.community_id {
        sqlx::query_scalar("SELECT community FROM bgp_communities WHERE id = ?")
            .bind(cid)
            .fetch_optional(pool)
            .await?
    } else {
        None
    };

    // 2. Resolve peer neighbor_ip
    let neighbor_ip: Option<String> = if let Some(pid) = req.peer_id {
        sqlx::query_scalar("SELECT neighbor_ip FROM bgp_peers WHERE id = ?")
            .bind(pid)
            .fetch_optional(pool)
            .await?
    } else {
        None
    };

    // 3. Withdraw previous active announcements for this prefix
    sqlx::query(
        "UPDATE bgp_announcements SET withdrawn_at = datetime('now') \
         WHERE prefix = ? AND withdrawn_at IS NULL",
    )
    .bind(&req.prefix)
    .execute(pool)
    .await?;

    // 4. Build command
    let command = build_announce_command(
        &req.prefix,
        &req.next_hop,
        community_value.as_deref(),
        neighbor_ip.as_deref(),
    );

    // 5. Write to pipe (non-fatal)
    let pipe_path_owned = pipe_path.to_string();
    let command_owned = command.clone();
    let result = tokio::task::spawn_blocking(move || write_to_pipe(&pipe_path_owned, &command_owned)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!("BGP pipe write failed: {}", e),
        Err(e) => tracing::warn!("spawn_blocking failed for BGP pipe: {}", e),
    }

    // 6. Insert announcement
    let db_result = sqlx::query(
        "INSERT INTO bgp_announcements \
         (prefix, next_hop, community_id, peer_id, origin, origin_detail) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&req.prefix)
    .bind(&req.next_hop)
    .bind(req.community_id)
    .bind(req.peer_id)
    .bind(&req.origin)
    .bind(&req.origin_detail)
    .execute(pool)
    .await?;

    Ok((db_result.last_insert_rowid(), command))
}

pub async fn withdraw(
    pool: &SqlitePool,
    pipe_path: &str,
    announcement_id: i64,
) -> anyhow::Result<String> {
    // 1. Fetch announcement
    let ann: Option<crate::bgp::BgpAnnouncement> =
        sqlx::query_as("SELECT * FROM bgp_announcements WHERE id = ?")
            .bind(announcement_id)
            .fetch_optional(pool)
            .await?;

    let ann = ann.ok_or_else(|| anyhow::anyhow!("Announcement {} not found", announcement_id))?;

    if ann.withdrawn_at.is_some() {
        anyhow::bail!("Announcement {} already withdrawn", announcement_id);
    }

    // 2. Resolve peer neighbor_ip
    let neighbor_ip: Option<String> = if let Some(pid) = ann.peer_id {
        sqlx::query_scalar("SELECT neighbor_ip FROM bgp_peers WHERE id = ?")
            .bind(pid)
            .fetch_optional(pool)
            .await?
    } else {
        None
    };

    // 3. Build withdraw command
    let command = match &neighbor_ip {
        Some(ip) => format!(
            "neighbor {} withdraw route {} next-hop {}",
            ip, ann.prefix, ann.next_hop
        ),
        None => format!("withdraw route {} next-hop {}", ann.prefix, ann.next_hop),
    };

    // 4. Write to pipe (non-fatal)
    let pipe_path_owned = pipe_path.to_string();
    let command_owned = command.clone();
    let result = tokio::task::spawn_blocking(move || write_to_pipe(&pipe_path_owned, &command_owned)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!("BGP pipe write failed: {}", e),
        Err(e) => tracing::warn!("spawn_blocking failed for BGP pipe: {}", e),
    }

    // 5. Mark withdrawn
    sqlx::query(
        "UPDATE bgp_announcements SET withdrawn_at = datetime('now') WHERE id = ?",
    )
    .bind(announcement_id)
    .execute(pool)
    .await?;

    Ok(command)
}

pub async fn reannounce_all(pool: &SqlitePool, pipe_path: &str) -> anyhow::Result<usize> {
    let active: Vec<crate::bgp::BgpAnnouncement> = sqlx::query_as(
        "SELECT * FROM bgp_announcements WHERE withdrawn_at IS NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0;

    for ann in active {
        let community_value: Option<String> = if let Some(cid) = ann.community_id {
            sqlx::query_scalar("SELECT community FROM bgp_communities WHERE id = ?")
                .bind(cid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        let neighbor_ip: Option<String> = if let Some(pid) = ann.peer_id {
            sqlx::query_scalar("SELECT neighbor_ip FROM bgp_peers WHERE id = ?")
                .bind(pid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
        } else {
            None
        };

        let command = build_announce_command(
            &ann.prefix,
            &ann.next_hop,
            community_value.as_deref(),
            neighbor_ip.as_deref(),
        );

        let pipe_path_owned = pipe_path.to_string();
        let command_owned = command.clone();
        let result =
            tokio::task::spawn_blocking(move || write_to_pipe(&pipe_path_owned, &command_owned))
                .await;
        match result {
            Ok(Ok(())) => count += 1,
            Ok(Err(e)) => tracing::warn!("BGP reannounce pipe write failed for {}: {}", ann.prefix, e),
            Err(e) => tracing::warn!("spawn_blocking failed for BGP reannounce: {}", e),
        }
    }

    tracing::info!("BGP reannounce_all: sent {} commands", count);
    Ok(count)
}

fn build_announce_command(
    prefix: &str,
    next_hop: &str,
    community: Option<&str>,
    neighbor_ip: Option<&str>,
) -> String {
    let community_part = match community {
        Some(c) => format!(" community [{}]", c),
        None => String::new(),
    };

    match neighbor_ip {
        Some(ip) => format!(
            "neighbor {} announce route {} next-hop {}{}",
            ip, prefix, next_hop, community_part
        ),
        None => format!(
            "announce route {} next-hop {}{}",
            prefix, next_hop, community_part
        ),
    }
}

fn write_to_pipe(pipe_path: &str, command: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).open(pipe_path)?;
    writeln!(f, "{}", command)?;
    Ok(())
}
