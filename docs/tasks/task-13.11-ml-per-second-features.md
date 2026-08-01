# Task 13.11 — Features do ML por segundo real

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 30m
**Files:** `collector-core/src/features.rs`, `collector-core/src/main.rs`

## Problema (visto em produção de lab)

O extrator tratava o lote drenado inteiro como "janela de 1s". Quando o exporter
descarrega flows acumulados (expiração em rajada), pps/bps das features saíam
inflados na proporção do acúmulo — anomalias registradas com **292k pps num link
de 100 Mbps** (fisicamente impossível, teto ~37 kpps). O gráfico foi corrigido na
13.2/13.8; o extrator ficou para trás (anotado na própria spec da 13.2).

## Implementação

- `features::extract` agrupa por `(slice_sec, exporter, src_ip)` — a janela do
  vetor é o segundo real da fatia, `window_ts = sec`
- pps/bps viram taxas verdadeiras; warm-up coleta mais vetores por lote (um por
  segundo ativo), acelerando o preenchimento das 5.000 amostras

## Consequência

- Anomalias novas carregam PPS real; as antigas mantêm os valores inflados da
  época (dado histórico, não reescrever)
- O modelo aprende sobre distribuição correta — scores de anomalia deixam de
  premiar o artefato de rajada do exporter

## Aceite

- Feature nunca reporta pps acima da capacidade física do enlace em regime
- `cargo test` e clippy limpos
