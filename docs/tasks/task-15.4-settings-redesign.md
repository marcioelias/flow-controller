# Task 15.4 — Reforma da tela de Configurações 🔲

**Phase:** 15 (Feedback & Explicabilidade)
**Files:** `frontend/src/views/Settings.vue`, `collector-core/src/llm.rs`, `collector-core/src/main.rs`

Pedido do Marcio (01/08/2026):
- Tela full-width com **abas por grupo** (Rede, Organização, Análise, Alertas,
  Coletor, IA, Sessão)
- **CRUD de prefixos e ASNs**: chips add/remove individuais no lugar do campo
  texto separado por vírgula (storage continua nas mesmas keys)
- **Config de IA guiada**: toggle habilitar, endpoint com botão "testar conexão",
  **dropdown de modelos** detectados no Ollama (`GET /api/llm/models` proxy de
  `/api/tags`), idioma como select

Backend novo: `GET /api/llm/models` e `POST /api/llm/test` (admin).
