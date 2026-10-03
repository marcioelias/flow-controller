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
use std::collections::HashMap;
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
        let roles = crate::exporters::role_map(&state.db).await;
        let current_bps = measure_avg_bps(&state.clickhouse_url, &roles)
            .await
            .unwrap_or(0);
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

/// Média de bps dos últimos 5 min (v4 + v6), medida como a **maior soma entre
/// os papéis** (task 17.9 R-06): o mesmo tráfego coletado em borda, CGNAT e BNG
/// não paga 3×, e cadastrar só BNG/CGNAT não deixa de contar.
async fn measure_avg_bps(ch_url: &str, roles: &HashMap<String, String>) -> Option<u64> {
    let sql = format!(
        "SELECT exporter, sum(b) FROM ( \
           SELECT toString(exporter_ip) AS exporter, sum(bytes) AS b FROM network_flows_v4 \
             WHERE timestamp > now() - INTERVAL {AVG_WINDOW_SECS} SECOND GROUP BY exporter \
           UNION ALL \
           SELECT toString(exporter_ip) AS exporter, sum(bytes) AS b FROM network_flows_v6 \
             WHERE timestamp > now() - INTERVAL {AVG_WINDOW_SECS} SECOND GROUP BY exporter) \
         GROUP BY exporter FORMAT TSV"
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
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let rows = text.lines().filter_map(|line| {
        let (exporter, bytes) = line.split_once('\t')?;
        Some((exporter.to_string(), bytes.trim().parse::<u64>().ok()?))
    });
    let bytes = max_role_volume(rows, roles);
    Some(bytes * 8 / AVG_WINDOW_SECS)
}

/// Sums volume per role and returns the largest; unknown exporters count as border
fn max_role_volume(
    per_exporter: impl Iterator<Item = (String, u64)>,
    roles: &HashMap<String, String>,
) -> u64 {
    let mut per_role: HashMap<&str, u64> = HashMap::new();
    for (exporter, bytes) in per_exporter {
        let role = roles
            .get(&exporter)
            .map(String::as_str)
            .unwrap_or(crate::exporters::DEFAULT_ROLE);
        *per_role.entry(role).or_default() += bytes;
    }
    per_role.into_values().max().unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn roles() -> HashMap<String, String> {
        [
            ("10.0.0.1", "borda"),
            ("10.0.0.2", "bng"),
            ("10.0.0.3", "cgnat"),
        ]
        .into_iter()
        .map(|(ip, r)| (ip.to_string(), r.to_string()))
        .collect()
    }

    // AC-03 (task 17.9)
    #[test]
    fn license_takes_the_largest_role_sum() {
        let rows = vec![
            ("10.0.0.1".to_string(), 500),
            ("10.0.0.2".to_string(), 400),
            ("10.0.0.3".to_string(), 400),
        ];
        assert_eq!(max_role_volume(rows.into_iter(), &roles()), 500);
    }

    // AC-02 (task 17.9)
    #[test]
    fn bng_only_still_counts() {
        let rows = vec![("10.0.0.2".to_string(), 400)];
        assert_eq!(max_role_volume(rows.into_iter(), &roles()), 400);
    }

    #[test]
    fn same_role_boxes_add_up_and_unknown_is_border() {
        let rows = vec![
            ("10.0.0.1".to_string(), 300),
            ("10.9.9.9".to_string(), 300),
            ("10.0.0.2".to_string(), 500),
        ];
        assert_eq!(max_role_volume(rows.into_iter(), &roles()), 600);
    }
}
