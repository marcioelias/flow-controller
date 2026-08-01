# Task 13.2 — Distribuição temporal dos bytes (time spreading)

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 2h
**Files:** `aggregator/src/lib.rs`, `collector-core/src/main.rs`, `collector-core/src/features.rs`

## Problema

Com 13.1 sabemos o intervalo real do flow, mas a agregação joga tudo no segundo da
chegada. Resultado: gráficos com espigões artificiais e "atraso" aparente — o tráfego
só aparece quando o exporter fecha o flow, concentrado num ponto.

Distribuir os bytes pelo intervalo real é o que separa um gráfico fiel de um gráfico
de chegada. É o comportamento esperado da melhor ferramenta da categoria.

## Implementação

### 1. Aggregator passa a ser indexado por (segundo, chave)

```rust
map: HashMap<(u32, AggregationKey), AggregatedMetrics, RandomState>
```

### 2. `aggregate(flow, now_secs)` fatia o flow

- Sem timestamps (`start_ms == 0`): tudo no segundo atual (comportamento antigo).
- Com timestamps: fatia `[start_ms/1000 ..= end_ms/1000]`, bytes e packets divididos
  proporcionalmente (resto na última fatia), `flow_count` contado uma única vez na
  primeira fatia.
- Clamps de sanidade (relógio do exporter pode estar errado):
  - fim no futuro → clampa em `now`
  - início mais antigo que `now - MAX_SPREAD_SECS` (300s) → clampa
  - máximo de fatias = MAX_SPREAD_SECS (bound de trabalho por flow)

### 3. `FlowWindow` carrega o mapa fatiado

O exporter task já mescla por `(window_ts, key)` — passa a usar o segundo da fatia
como timestamp da linha. O ClickHouse recebe linhas retroativas de até 300s, o que é
transparente para MergeTree.

- `LiveFlowStats` (WS): mantém semântica de chegada (não dá pra reescrever o passado
  num stream) — soma tudo que foi drenado no tick.
- `features::extract`: itera o mapa ignorando a fatia (janela ML segue 1s por batch).

## Consequência visível

Upload de 134 MiB em 20s deixa de ser um espigão de 1s e vira uma rampa de ~20s nos
gráficos de timeline/histórico, retroativa ao momento em que o tráfego de fato passou.

## Aceite

- Flow com `start/end` conhecidos aparece espalhado nos segundos corretos no ClickHouse
- Soma de bytes preservada (nenhum byte criado/perdido na divisão)
- Flow sem timestamps: comportamento idêntico ao atual
- Testes: divisão exata, resto, clamp de futuro, clamp de passado, flow de 1s
