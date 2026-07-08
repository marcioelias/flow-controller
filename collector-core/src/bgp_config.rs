use sqlx::SqlitePool;

pub async fn generate_config(pool: &SqlitePool) -> anyhow::Result<String> {
    let peers: Vec<crate::bgp::BgpPeer> =
        sqlx::query_as("SELECT * FROM bgp_peers WHERE enabled = 1 ORDER BY id")
            .fetch_all(pool)
            .await?;

    if peers.is_empty() {
        anyhow::bail!("No enabled BGP peers configured");
    }

    // router-id from the first peer (ordered by id)
    let router_id = &peers[0].local_ip;

    let mut config = String::new();
    config.push_str("# GERADO AUTOMATICAMENTE — NÃO EDITE\n");
    config.push_str("process flowvision-controller {\n");
    config.push_str(
        "    run /bin/sh -c 'while true; do cat /run/exabgp/exabgp.in; done';\n",
    );
    config.push_str("    encoder text;\n");
    config.push_str("}\n\n");

    for peer in &peers {
        config.push_str(&format!(
            "neighbor {} {{\n",
            peer.neighbor_ip
        ));
        config.push_str(&format!("    description \"{}\";\n", peer.name));
        config.push_str(&format!("    router-id {};\n", router_id));
        config.push_str(&format!("    local-address {};\n", peer.local_ip));
        config.push_str(&format!("    local-as {};\n", peer.local_as));
        config.push_str(&format!("    peer-as {};\n", peer.peer_as));
        config.push_str(&format!("    hold-time {};\n", peer.hold_time));
        if let Some(ref pwd) = peer.md5_password {
            config.push_str(&format!("    md5-password \"{}\";\n", pwd));
        }
        config.push_str("    family { ipv4 unicast; }\n");
        config.push_str("    api { processes [ flowvision-controller ]; }\n");
        config.push_str("}\n\n");
    }

    Ok(config)
}

pub async fn apply_config(pool: &SqlitePool, config_path: &str) -> anyhow::Result<usize> {
    let config = generate_config(pool).await?;

    // Count enabled peers
    let peer_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bgp_peers WHERE enabled = 1")
            .fetch_one(pool)
            .await?;

    // Write config file
    tokio::fs::write(config_path, &config).await?;
    tracing::info!("BGP config written to {}", config_path);

    // Try to signal exabgp reload via pipe
    let pipe_path = std::env::var("EXABGP_PIPE_PATH")
        .unwrap_or_else(|_| "/run/exabgp/exabgp.in".to_string());

    let pipe_path_owned = pipe_path.clone();
    let result =
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .open(&pipe_path_owned)?;
            writeln!(f, "restart")?;
            Ok(())
        })
        .await;

    match result {
        Ok(Ok(())) => tracing::info!("ExaBGP reload signal sent"),
        Ok(Err(e)) => tracing::warn!("Could not send ExaBGP reload signal: {}", e),
        Err(e) => tracing::warn!("spawn_blocking failed for ExaBGP reload: {}", e),
    }

    Ok(peer_count as usize)
}
