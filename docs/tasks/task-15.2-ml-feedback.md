# Task 15.2 — Feedback → aprendizado do detector

**Phase:** 15 (Feedback & Explicabilidade)
**Files:** `collector-core/src/ml_api.rs`, `collector-core/src/ml_runner.rs`, `collector-core/src/alerts.rs`

- Coluna `feedback` em `alert_events` (`false_positive` | `confirmed` | NULL)
- `PATCH /api/ml/events/:id/feedback` (admin)
- `ml_runner`: threshold por IP = base + 0.05 × min(FPs, 4) — cada falso positivo
  marcado exige score maior para aquele IP alertar de novo (teto +0.20); contagem
  recarregada do SQLite a cada 10 lotes. O vetor FP permanece no ring buffer de
  treino (ele É tráfego normal — reforça a baseline no retrain).
