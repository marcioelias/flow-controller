# Task 17.5 — Explicador IA: configuração em runtime e falhas visíveis

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/llm.rs`, `collector-core/src/main.rs`, `collector-core/src/alerts.rs`,
`collector-core/src/ml_api.rs`, `collector-core/src/alert_api.rs`,
`frontend/src/views/AiInsights.vue`, `frontend/src/views/AlertEvents.vue`, `frontend/src/stores/*`

## Problema

Relato do Marcio (03/10/2026): com o Ollama configurado e o **teste da tela de
Configurações OK**, todo alerta fica em "gerando…" para sempre e nenhum insight sai.

Causas encontradas:

1. **Explicador só nasce no boot.** `main.rs` só cria o `run_llm_explainer` se
   `LLM_ENABLED=true` no momento da subida. Ligar a IA pela UI com o coletor rodando
   não inicia nada; endpoint/modelo também ficam congelados. O teste da UI passa
   porque chama o Ollama direto com os valores do formulário.
2. **Falha silenciosa e eterna.** Erro do LLM (ex.: timeout de 30 s no primeiro load
   de um modelo em CPU) não é registrado: o evento volta à fila e é retentado a cada
   10 s para sempre; os 5 mais recentes que falham bloqueiam os demais.
3. **UI sem estado.** "gerando…" aparece igual para IA desligada, falha ou fila.
4. **`/api/ml/status` ignora o banco.** `llm_enabled`/`llm_model` vêm só de env.

## Regras

- **R-01** — O explicador sobe sempre e relê `LLM_ENABLED`, `LLM_ENDPOINT`,
  `LLM_MODEL`, `APP_LANGUAGE` (banco → env) a cada ciclo. Desligado → só dorme.
- **R-02** — Timeout de geração 120 s (modelo local em CPU, primeiro load).
- **R-03** — Cada falha incrementa `explanation_attempts` e grava `explanation_error`
  (texto curto). Após **3** falhas o evento sai da fila.
- **R-04** — Estado da explicação por evento (`explanation_status`):
  `done` (tem texto) · `failed` (3 falhas) · `pending` (IA ligada, na fila) ·
  `disabled` (IA desligada e sem texto).
- **R-05** — `POST /api/alerts/events/:id/explain` zera tentativas e erro do evento
  (volta à fila). Responde 204; 404 se o evento não existe.
- **R-06** — `/api/ml/status` lê `llm_enabled`/`llm_model` do banco (fallback env).
- **R-07** — UI: `pending` → "gerando…" com spinner; `failed` → "falhou: <erro>" +
  botão "Tentar de novo"; `disabled` → "IA desativada" com link para Configurações.
- **R-08 — Cota de CPU do LLM.** Na homologação (03/10/2026) o Ollama em CPU ocupou
  os 12 núcleos da VM continuamente (~25 s por explicação, ~4 anomalias/min chegando),
  disputando com coletor e ClickHouse. O container do Ollama tem `cpus` limitado por
  `OLLAMA_CPUS` (padrão 4) e o coletor pede `num_thread` igual (`LLM_NUM_THREADS`).

## Dados

`alert_events` ganha (migração idempotente):
`explanation_attempts INTEGER NOT NULL DEFAULT 0`, `explanation_error TEXT`.

## Aceite

- AC-01: ligar a IA pela UI com o coletor rodando → eventos sem explicação recebem
  texto em até ~1 ciclo, sem reiniciar.
- AC-02: endpoint inválido → após 3 ciclos o evento mostra "falhou" com o erro e não
  é mais tentado; "Tentar de novo" o recoloca na fila.
- AC-03: IA desligada → eventos mostram "IA desativada", não "gerando…".
