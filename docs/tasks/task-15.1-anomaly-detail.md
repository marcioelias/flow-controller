# Task 15.1 — Detalhe da anomalia + feedback do operador

**Phase:** 15 (Feedback & Explicabilidade)
**Files:** `frontend/src/views/AiInsights.vue`, `frontend/src/stores/ai.ts`

Clicar numa linha de anomalia abre modal com todos os campos (hora, exporter, IP,
score, PPS humanizado, pacote médio, upload/download da janela, detalhe técnico e
explicação IA completa) e botões **Falso positivo** / **Ameaça confirmada**.
Badge FP/confirmado na tabela. Feedback é toggle (clicar de novo limpa).
