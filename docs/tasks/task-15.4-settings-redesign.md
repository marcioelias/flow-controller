# Task 15.4 — Reforma da tela de Configurações ✅

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

## Implementado (01/08/2026)

- Tela full-width, 7 abas por grupo com ícones
- Rede: chips add/remove para ASNs (valida numérico) e prefixos (valida CIDR),
  dois cards lado a lado; storage inalterado (mesmas keys CSV)
- IA: toggle liga/desliga, endpoint com botão Conectar (descobre modelos via
  `POST /api/llm/models`), modelos como botões selecionáveis, idioma como
  seleção (pt-BR/en/es), botão "Testar geração" (`POST /api/llm/test`) exibindo
  a resposta do modelo, dicas de endpoint local vs Docker
- Demais grupos: listagem genérica preservada dentro da aba
