//! Enforcement da licença (task 14.1).
//!
//! Regras decididas em 31/07/2026:
//! - Nunca descartar dado: coleta segue completa em qualquer situação
//! - Acima de `max_bps` (média de 5 min): banner + métrica
//! - Excedente sustentado por 7 dias corridos: views analíticas bloqueadas
//!   (HTTP 402) até licenciar ou voltar ao limite
//! - `max_talkers` limita as linhas das views analíticas (aplicado em stats.rs)
//! - Licença expirada em runtime rebaixa para free tier sem reiniciar

use crate::auth::AppState;
use crate::license;
use crate::settings;
use std::sync::Arc;
use std::time::Duration;

const CHECK_SECS: u64 = 60;
const AVG_WINDOW_SECS: u64 = 300;
const DEGRADE_AFTER_SECS: i64 = 7 * 86_400;
/// Persistido em settings para o excedente sobreviver a restart
const OVER_SINCE_KEY: &str = "LICENSE_OVER_BPS_SINCE";

pub async fn run_enforcer(state: Arc<AppState>) {
    let mut over_since: Option<i64> = settings::get_value(&state.db, OVER_SINCE_KEY)
        .await
        .and_then(|v| v.parse().ok());
    tracing::info!(
        "License enforcer started (avg {}s, check {}s, degrade after {}d)",
        AVG_WINDOW_SECS,
        CHECK_SECS,
        DEGRADE_AFTER_SECS / 86_400
    );

    loop {
        tokio::time::sleep(Duration::from_secs(CHECK_SECS)).await;

        // O arquivo é a fonte de verdade: remoção, troca ou expiração passam a
        // valer em runtime (≤60s), sem restart. Efeito colateral aceito: uma
        // licença aplicada via API cujo write em disco falhou será revertida
        // aqui — o POST já loga warn nesse caso.
        let fresh = license::load_and_validate("/etc/flow-collector/license.key");
        {
            let mut lic = state.license.write().unwrap();
            if lic.tier_label != fresh.tier_label || lic.valid != fresh.valid {
                tracing::warn!("License state changed on disk: {}", fresh.tier_label);
                *lic = fresh;
            }
        }

        let max_bps = { state.license.read().unwrap().max_bps };
        let current_bps = measure_avg_bps(&state.clickhouse_url).await.unwrap_or(0);
        let now = chrono_now_secs();

        let over = max_bps.map(|m| current_bps > m).unwrap_or(false);
        match (over, over_since) {
            (true, None) => {
                over_since = Some(now);
                persist_over_since(&state.db, Some(now)).await;
                tracing::warn!(
                    current_bps,
                    max_bps = max_bps.unwrap_or(0),
                    "Traffic above license limit"
                );
            }
            (false, Some(_)) => {
                over_since = None;
                persist_over_since(&state.db, None).await;
                tracing::info!("Traffic back under license limit");
            }
            _ => {}
        }

        let degraded = over_since
            .map(|t| now - t >= DEGRADE_AFTER_SECS)
            .unwrap_or(false);

        {
            let mut lic = state.license.write().unwrap();
            lic.over_limit = over;
            lic.degraded = degraded;
            lic.current_bps = current_bps;
            lic.over_since = over_since;
        }
        state.metrics.license_over_bps.set(over as i64);
        state.metrics.license_degraded.set(degraded as i64);
    }
}

fn chrono_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Média de bps dos últimos 5 min medida direto no ClickHouse (v4 + v6)
async fn measure_avg_bps(ch_url: &str) -> Option<u64> {
    let sql = format!(
        "SELECT sum(b) FROM ( \
           SELECT sum(bytes) AS b FROM network_flows_v4 \
             WHERE timestamp > now() - INTERVAL {AVG_WINDOW_SECS} SECOND \
           UNION ALL \
           SELECT sum(bytes) AS b FROM network_flows_v6 \
             WHERE timestamp > now() - INTERVAL {AVG_WINDOW_SECS} SECOND)"
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let text = client
        .get(ch_url)
        .query(&[("query", sql.as_str())])
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let bytes: u64 = text.trim().parse().ok()?;
    Some(bytes * 8 / AVG_WINDOW_SECS)
}

async fn persist_over_since(pool: &sqlx::SqlitePool, value: Option<i64>) {
    let v = value.map(|t| t.to_string()).unwrap_or_default();
    let _ = sqlx::query(
        "INSERT INTO settings (key, value, label, description, group_name) \
         VALUES (?, ?, 'Excedente de licença desde', 'interno — não editar', 'Sistema') \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(OVER_SINCE_KEY)
    .bind(v)
    .execute(pool)
    .await;
}
