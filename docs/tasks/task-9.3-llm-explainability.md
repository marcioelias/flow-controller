# Task 9.3 — LLM Explainability (Ollama / qwen2.5)

## Goal

Para cada alerta gerado (qualquer `alert_type`), gerar uma explicação em linguagem natural
usando um LLM local via Ollama. A explicação descreve o que o padrão de tráfego indica,
por que é suspeito, e o que o operador deve verificar — tudo em 2-3 frases curtas.

A geração é **async e não-bloqueante**: se o LLM estiver lento ou indisponível,
o sistema continua funcionando normalmente. A explicação é salva na tabela de eventos
quando disponível e exibida no frontend.

---

## Hardware requirements

| Componente | RAM | CPU |
|------------|-----|-----|
| ClickHouse | ~4 GB | variável |
| flow-collector | ~512 MB | 2 cores |
| **Ollama + qwen2.5:3b Q4_K_M** | **~2.3 GB** | **4 cores** |
| **Total** | **~7 GB** | **6 cores** |

Funciona em uma VM de 8GB / 8 vCPUs. Sem GPU necessária.
Throughput: ~15-30 tokens/s em CPU, latência de resposta ~5-15s por explicação.

---

## Environment variables

```env
# .env / docker-compose
LLM_ENDPOINT=http://ollama:11434     # URL base do Ollama (ou de qualquer API OpenAI-compat.)
LLM_MODEL=qwen2.5:3b                 # modelo a usar
LLM_ENABLED=true                     # desabilita a feature completamente se false/ausente
```

`LLM_ENDPOINT` aceita tanto Ollama local quanto APIs externas com API OpenAI-compatible
(Groq, Together, local vLLM). A requisição usa o endpoint `/api/generate` (formato Ollama).
Para APIs OpenAI-compatible, task 9.3 deve detectar pelo hostname e usar `/v1/chat/completions`.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/llm.rs` | CREATE — Ollama client + prompt builder |
| `collector-core/src/main.rs` | MODIFY — add LLM env vars, spawn llm explainer |
| `collector-core/src/alerts.rs` | MODIFY — add `explanation TEXT` column to `alert_events` |
| `docker-compose.yml` | MODIFY — add `ollama` service |
| `frontend/src/views/AlertEvents.vue` | MODIFY — show explanation column |

---

## Database migration

```sql
ALTER TABLE alert_events ADD COLUMN IF NOT EXISTS explanation TEXT;
```

Added to `alerts::init_tables()` alongside the `bgp_announced` migration.

---

## `llm.rs`

```rust
use crate::alerts::AlertEvent;

const MAX_EXPLANATION_TOKENS: u32 = 120;
const REQUEST_TIMEOUT_SECS:   u64 = 30;

pub struct LlmClient {
    endpoint: String,
    model:    String,
    client:   reqwest::Client,
}

impl LlmClient {
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("LLM_ENDPOINT").ok()?;
        let enabled  = std::env::var("LLM_ENABLED").unwrap_or_default();
        if enabled != "true" { return None; }
        Some(Self {
            endpoint,
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "qwen2.5:3b".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()
                .expect("reqwest client"),
        })
    }

    pub async fn explain(&self, event: &AlertEvent) -> anyhow::Result<String> {
        let prompt = build_prompt(event);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": { "num_predict": MAX_EXPLANATION_TOKENS, "temperature": 0.3 }
        });

        let resp: serde_json::Value = self.client
            .post(format!("{}/api/generate", self.endpoint))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        resp["response"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("no response field in LLM output"))
    }
}

fn build_prompt(ev: &AlertEvent) -> String {
    let ctx = match ev.alert_type.as_str() {
        "upload_inversion" => format!(
            "alert: upload_inversion. IP {} uploaded {:.1} Mbps but historically downloads much more. Ratio indicates possible DDoS amplification or botnet C&C exfiltration.",
            ev.src_ip,
            ev.upload_bytes.unwrap_or(0) as f64 / 1e6
        ),
        "attack_signature" => format!(
            "alert: attack_signature. IP {} sent {:.0} PPS with avg packet size {:.0} bytes targeting ports [{}]. Indicative of volumetric DDoS.",
            ev.src_ip,
            ev.pps.unwrap_or(0.0),
            ev.avg_pkt_bytes.unwrap_or(0.0),
            ev.attack_ports.as_deref().unwrap_or("unknown")
        ),
        "ml_anomaly" => format!(
            "alert: ml_anomaly. IP {} shows statistical anomaly in traffic profile (ML Isolation Forest score above threshold). PPS: {:.0}, message: {}",
            ev.src_ip,
            ev.pps.unwrap_or(0.0),
            ev.message
        ),
        other => format!("alert: {other}. Source IP {}. {}", ev.src_ip, ev.message),
    };

    format!(
        "You are a network security analyst. Given this network flow alert, write 2-3 sentences: \
         (1) what this pattern suggests, (2) why it is suspicious, (3) what to verify next. \
         Be concise and technical. Do not repeat the raw numbers. Context: {ctx}"
    )
}
```

---

## `main.rs` — LLM explainer task

```rust
mod llm;

// After spawning ml_runner:
let llm_client = llm::LlmClient::from_env();
if let Some(client) = llm_client {
    let pool = state.db.clone();
    tokio::spawn(async move {
        run_llm_explainer(pool, client).await;
    });
}
```

```rust
async fn run_llm_explainer(pool: sqlx::SqlitePool, client: llm::LlmClient) {
    tracing::info!("LLM explainer started");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        // Find events without explanation
        let rows = sqlx::query(
            "SELECT id, rule_id, exporter_ip, src_ip, alert_type, severity, message,
                    upload_bytes, download_bytes, pps, avg_pkt_bytes, attack_ports,
                    notified, bgp_announced, created_at
             FROM alert_events
             WHERE explanation IS NULL OR explanation = ''
             ORDER BY id DESC
             LIMIT 5"
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        for row in rows {
            let event = /* map row to AlertEvent */ ;
            match client.explain(&event).await {
                Ok(text) => {
                    let _ = sqlx::query(
                        "UPDATE alert_events SET explanation = ? WHERE id = ?"
                    )
                    .bind(&text)
                    .bind(event.id)
                    .execute(&pool)
                    .await;
                    tracing::debug!("LLM explained alert #{}", event.id.unwrap_or(0));
                }
                Err(e) => tracing::warn!("LLM explain failed for alert: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}
```

---

## `docker-compose.yml` — Ollama service

```yaml
  ollama:
    image: ollama/ollama:latest
    container_name: flow-ollama
    restart: unless-stopped
    environment:
      - OLLAMA_KEEP_ALIVE=24h
    volumes:
      - ollama_data:/root/.ollama
    ports:
      - "11434:11434"
    networks:
      - flow-net

volumes:
  ollama_data:
```

First run — pull model:
```bash
docker exec flow-ollama ollama pull qwen2.5:3b
```

The `collector` service gets:
```yaml
environment:
  - LLM_ENDPOINT=http://ollama:11434
  - LLM_MODEL=qwen2.5:3b
  - LLM_ENABLED=true
```

---

## Frontend: explanation column

`AlertEvents.vue` — add after the BGP column:
```html
<th class="px-4 py-3">Explicação IA</th>
...
<td class="px-4 py-2.5 text-xs text-zinc-400 max-w-sm">
  <span v-if="ev.explanation" :title="ev.explanation" class="line-clamp-2">
    {{ ev.explanation }}
  </span>
  <span v-else class="text-zinc-700 italic">gerando...</span>
</td>
```

The `AlertEvent` TypeScript interface gains `explanation: string | null`.

---

## Acceptance criteria

- [ ] `LLM_ENABLED=false` (or absent) → explainer task not spawned, zero overhead
- [ ] `LLM_ENABLED=true` with Ollama down → explainer logs warns, keeps retrying, no panic
- [ ] Explanations appear in `alert_events.explanation` within ~15s of alert creation
- [ ] Prompt ≤ 300 tokens input; response ≤ 120 tokens — fits in qwen2.5:3b context
- [ ] `cargo build` passes
- [ ] No `unsafe` code
