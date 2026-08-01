# Task 15.3 — Idioma das explicações IA

**Phase:** 15 (Feedback & Explicabilidade)
**Files:** `collector-core/src/settings.rs`, `collector-core/src/llm.rs`

Setting `APP_LANGUAGE` (Organização, padrão `pt-BR`). O prompt instrui o modelo a
responder no idioma configurado (nome humano do idioma, não a tag — modelos 3B
seguem melhor). Explicações antigas em inglês: `UPDATE alert_events SET
explanation = NULL` regenera na língua nova.
