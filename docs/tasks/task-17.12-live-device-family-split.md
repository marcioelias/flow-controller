# Task 17.12 — Tempo real por dispositivo com IPv4/IPv6 e preenchimento retroativo

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/main.rs`, `frontend/src/views/Dashboard.vue`

## Problema

Relato do Marcio (04/10/2026): "no realtime está lendo só IPv4". Com o BNG marcado como
`bng` (task 17.9), o gráfico ao vivo usa o modo por dispositivo ("BNG — todos" ou uma
caixa). Nesse modo o WebSocket manda só `per_device_in/out` — totais por caixa, sem
família e por hora de chegada — e o frontend coloca tudo na série "IPv4". O IPv6 fica
somado ao IPv4 com rótulo errado, e os segundos não preenchem retroativamente
(limitação aceita na task 13.6 que a 17.9 tornou o modo comum).

## Regras

- **R-01** — O WebSocket passa a mandar `device_slices: { "<exporter_ip>": [{ sec, v4_in,
  v4_out, v6_in, v6_out }] }`: as mesmas fatias por segundo real do total global, por
  caixa. Calculadas só quando há cliente conectado (como as globais).
- **R-02** — No modo por caixa ou por papel, o gráfico soma `device_slices` das caixas
  selecionadas, por segundo real (preenche retroativamente), com a divisão IPv4/IPv6 e
  o seletor Todos · IPv4 · IPv6 disponíveis, como no total da borda.
- **R-03** — `per_device`, `per_device_in`, `per_device_out` continuam no payload
  (compatibilidade); o gráfico deixa de usá-los.

## Aceite

- AC-01: com só o BNG (papel `bng`), "BNG — todos" mostra IPv4 e IPv6 separados, e a soma
  bate com o total da caixa.
- AC-02: os últimos segundos sobem retroativamente conforme os flows expiram, como no
  modo borda.
