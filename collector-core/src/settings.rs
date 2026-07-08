use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub label: String,
    pub description: String,
    pub group_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingRequest {
    pub value: String,
}

const KNOWN_KEYS: &[&str] = &[
    // Rede
    "OWN_ASN_LIST",
    "INTERNAL_PREFIXES",
    // Organização
    "TIMEZONE",
    // Análise
    "FLOW_RETENTION_DAYS",
    "MAX_TOP_TALKERS",
    "DEFAULT_WINDOW_MIN",
    // Alertas
    "ALERT_COOLDOWN_MIN",
    "UPLOAD_INVERSION_FACTOR",
    "ATTACK_PPS_THRESHOLD",
    "ATTACK_PKT_SIZE_MAX",
    // Sessão
    "SESSION_TIMEOUT_HOURS",
    // Coletor
    "COLLECTOR_WORKERS",
    // IA
    "LLM_ENABLED",
    "LLM_ENDPOINT",
    "LLM_MODEL",
];

pub async fn init_settings_table(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS settings (
            key        TEXT PRIMARY KEY,
            value      TEXT NOT NULL DEFAULT '',
            label      TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            group_name TEXT NOT NULL DEFAULT ''
        )"#,
    )
    .execute(pool)
    .await?;

    let defaults: &[(&str, &str, &str, &str, &str)] = &[
        // ── Rede ─────────────────────────────────────────────────────────
        (
            "OWN_ASN_LIST",
            "",
            "ASNs Próprios",
            "ASNs da sua rede, separados por vírgula (ex: 12345,67890). \
             Usado para classificar direção do tráfego e detecção de anomalias.",
            "Rede",
        ),
        (
            "INTERNAL_PREFIXES",
            "",
            "Prefixos Internos",
            "Blocos CIDR que pertencem à sua rede, separados por vírgula \
             (ex: 10.0.0.0/8,192.168.0.0/16,203.0.113.0/24). \
             Complementa a classificação por ASN para redes sem BGP.",
            "Rede",
        ),
        // ── Organização ──────────────────────────────────────────────────
        (
            "TIMEZONE",
            "America/Sao_Paulo",
            "Fuso Horário",
            "Fuso horário para exibição de datas e horas (IANA, ex: America/Sao_Paulo, UTC).",
            "Organização",
        ),
        // ── Análise ──────────────────────────────────────────────────────
        (
            "FLOW_RETENTION_DAYS",
            "30",
            "Retenção de Flows (dias)",
            "Número de dias de dados a manter no ClickHouse. \
             Requer reinício do serviço para recriar o TTL.",
            "Análise",
        ),
        (
            "MAX_TOP_TALKERS",
            "50",
            "Limite Top Talkers / ASN / Aplicações",
            "Número máximo de resultados retornados nas views analíticas.",
            "Análise",
        ),
        (
            "DEFAULT_WINDOW_MIN",
            "60",
            "Janela de Tempo Padrão (min)",
            "Janela de tempo pré-selecionada ao abrir as telas de análise (15, 60, 1440...).",
            "Análise",
        ),
        // ── Alertas ──────────────────────────────────────────────────────
        (
            "ALERT_COOLDOWN_MIN",
            "60",
            "Cooldown de Alertas (min)",
            "Tempo mínimo entre dois alertas consecutivos para o mesmo IP de origem.",
            "Alertas",
        ),
        (
            "UPLOAD_INVERSION_FACTOR",
            "2",
            "Fator de Inversão de Upload",
            "Upload considerado anômalo quando upload > download × fator. \
             Valor padrão 2 significa upload > 2× o download histórico.",
            "Alertas",
        ),
        (
            "ATTACK_PPS_THRESHOLD",
            "10000",
            "Threshold de PPS (ataque)",
            "Pacotes por segundo acima deste valor acionam a regra de assinatura de ataque.",
            "Alertas",
        ),
        (
            "ATTACK_PKT_SIZE_MAX",
            "200",
            "Tamanho Máximo de Pacote (ataque)",
            "Pacotes menores que este valor (bytes) + alto PPS são considerados assinatura de ataque.",
            "Alertas",
        ),
        // ── Sessão ───────────────────────────────────────────────────────
        (
            "SESSION_TIMEOUT_HOURS",
            "24",
            "Timeout de Sessão (horas)",
            "Duração do token JWT. Usuários precisam fazer login novamente após este período.",
            "Sessão",
        ),
        // ── Coletor ──────────────────────────────────────────────────────
        (
            "COLLECTOR_WORKERS",
            "4",
            "Worker Threads do Coletor",
            "Número de threads de parsing/agregação de flows. \
             Cada worker processa os pacotes de um subconjunto de exporters (hash do IP). \
             Recomendado: metade dos núcleos físicos disponíveis. Requer reinício do serviço.",
            "Coletor",
        ),
        // ── IA / ML ──────────────────────────────────────────────────────
        (
            "LLM_ENABLED",
            "false",
            "Explicações via IA (LLM)",
            "Ativa a geração automática de explicações para alertas usando um LLM. \
             Requer Ollama local ou endpoint compatível. \
             Defina como 'true' para ativar.",
            "IA",
        ),
        (
            "LLM_ENDPOINT",
            "http://ollama:11434",
            "Endpoint do LLM",
            "URL base do servidor Ollama ou API OpenAI-compatible \
             (ex: http://ollama:11434 para Ollama local no Docker, \
             https://api.groq.com/openai/v1 para Groq).",
            "IA",
        ),
        (
            "LLM_MODEL",
            "qwen2.5:3b",
            "Modelo LLM",
            "Modelo a usar para geração de explicações. \
             Exemplos: qwen2.5:3b (local, ~2.3 GB RAM), \
             llama3.2:3b, phi3:mini. Para Groq: llama3-8b-8192.",
            "IA",
        ),
    ];

    for (key, value, label, description, group) in defaults {
        sqlx::query(
            "INSERT OR IGNORE INTO settings (key, value, label, description, group_name) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(key)
        .bind(value)
        .bind(label)
        .bind(description)
        .bind(group)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// GET /api/settings — lista todas (requer auth)
pub async fn list_settings_handler(
    State(state): State<Arc<crate::auth::AppState>>,
) -> Result<Json<Vec<Setting>>, StatusCode> {
    let settings: Vec<Setting> = sqlx::query_as(
        "SELECT key, value, label, description, group_name \
         FROM settings ORDER BY group_name, key",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(settings))
}

/// PUT /api/settings/:key — atualiza value (requer admin)
pub async fn update_setting_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Path(key): Path<String>,
    Json(payload): Json<UpdateSettingRequest>,
) -> Result<Json<Setting>, StatusCode> {
    if !KNOWN_KEYS.contains(&key.as_str()) {
        return Err(StatusCode::NOT_FOUND);
    }

    sqlx::query("UPDATE settings SET value = ? WHERE key = ?")
        .bind(&payload.value)
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let setting: Setting = sqlx::query_as(
        "SELECT key, value, label, description, group_name FROM settings WHERE key = ?",
    )
    .bind(&key)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Setting updated: {} = {:?}", key, setting.value);
    Ok(Json(setting))
}

/// Helpers para outros módulos consultarem settings em runtime
pub async fn get_value(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .filter(|s: &String| !s.is_empty())
}

#[allow(dead_code)]
pub async fn get_own_asns(pool: &SqlitePool) -> Vec<u32> {
    let raw = get_value(pool, "OWN_ASN_LIST").await.unwrap_or_default();
    raw.split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect()
}
