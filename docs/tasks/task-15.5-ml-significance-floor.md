# Task 15.5 — Piso de significância para anomalias ML

**Phase:** 15 (Feedback & Explicabilidade)
**Files:** `collector-core/src/ml_runner.rs`, `collector-core/src/settings.rs`, `collector-core/src/llm.rs`

## Problema (lab, 01/08/2026)

35 warnings em poucas horas, a maioria com 5–96 pps — com baseline quase ociosa,
qualquer atividade vira outlier estatístico. Estatisticamente anômalo ≠
operacionalmente relevante; alerta de 5 pps é fadiga de alerta. Pior: o LLM
descrevia 5 pps como "alta taxa de pacotes" (papagueava o frame de anomalia).

## Implementação

- Settings `ML_MIN_PPS` (100) e `ML_MIN_BPS` (1 Mbps), grupo IA, recarregados
  pelo ml_runner a cada 10 lotes
- Abaixo dos DOIS pisos: o vetor continua alimentando o buffer (aprende), mas
  não gera alerta
- Prompt do LLM classifica a taxa (LOW/MODERATE/HIGH) e instrui explicitamente a
  nunca chamar taxa baixa de alta — anomalia de baixo volume é comportamental
- Eventos históricos de baixo volume purgados no lab

> **Nota (03/10/2026):** com amostragem 1:1000 estes pisos não filtram nada (um pacote
> sorteado já vale 1 000 pps). A task 17.10 adiciona o piso em pacotes amostrados
> (`ML_MIN_SAMPLES`) e passa a avaliar janelas de 1 minuto.
