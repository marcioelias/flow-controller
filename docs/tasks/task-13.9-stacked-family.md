# Task 13.9 — Espelho empilhado por família + stats mín/máx/méd/95%

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 2h
**Files:** `collector-core/src/main.rs`, `collector-core/src/stats.rs`, `frontend/src/lib/chartTheme.ts`, `frontend/src/views/Dashboard.vue`, `frontend/src/views/TrafficHistory.vue`

## Pedido (Marcio)

Um gráfico só: áreas translúcidas de IPv4 e IPv6 **empilhadas** somando o total de
cada lado do espelho (40M v4 + 60M v6 = 100M de entrada), seletor
**Todos | IPv4 | IPv6**, e barra de stats embaixo com mín/máx/méd/95% de entrada e
saída. Cores pedidas: verde/azul (entrada v4/v6), vermelho/amarelo (saída v4/v6).

## Implementação

### Backend
- `LiveSlice`: `{sec, v4_in, v4_out, v6_in, v6_out}` (família via `AggregationKey.src_ip`)
- `TimelinePoint`: + `v4_in_bytes / v4_out_bytes / v6_in_bytes / v6_out_bytes`
  (coluna `fam` no UNION; direção 255 conta como entrada)

### Frontend
- `chartTheme.ts`: `stackedMirrorDatasets()` (Chart.js `stack: 'in'/'out'`, fill
  origin/-1, y.stacked), `seriesStats()` (p95 por ordenação), tipo `FamFilter`
- Cores validadas no dataviz: entrada verde `#00a870` + azul `#3987e5`
  (ΔE 20.8); saída vermelho `#e5484d` + **amarelo-ouro `#a16207`** (ΔE 6.1, na
  faixa 6–8 — legal com legenda; amarelo vivo reprova banda de luminosidade no
  dark). Entre lados do eixo, a posição desambigua.
- Seletor Todos/IPv4/IPv6 nos dois gráficos; modo família única volta ao par
  Entrada/Saída clássico
- Barra de stats sob cada gráfico (janela visível): mín · máx · méd · 95%
- Filtro por dispositivo no ao vivo: sem split de família (payload per-device é
  agregado) — cai para par único arrival-based

## Aceite

- Topo da pilha de entrada = total de entrada (v4+v6 somam, não sobrepõem)
- Seletor alterna sem recarregar dados
- Stats batem com o período visível do gráfico
