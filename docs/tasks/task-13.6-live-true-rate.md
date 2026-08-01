# Task 13.6 — Taxa real no gráfico ao vivo (preenchimento retroativo)

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 1.5h
**Files:** `collector-core/src/main.rs`, `frontend/src/views/Dashboard.vue`, `frontend/src/views/TrafficHistory.vue`, `frontend/src/lib/chartTheme.ts`

## Problema

O gráfico "Tempo real" é arrival-based: cada broadcast do WS soma **todos** os bytes
drenados naquele tick, inclusive de flows que duraram 10–60s. Um download constante
de 100 Mbps aparece como espigões de ~1 Gbps (o volume de 10s comprimido num ponto
de 1s), intercalados com vales falsos de zero. O Histórico não sofre disso porque lê
o ClickHouse já corrigido pelo time spreading (13.2).

Diagnóstico confirmado em lab: fast.com a 100 Mbps → espigões de ~1 Gbps no ao vivo,
por segundo coerente no ClickHouse.

## Implementação

### 1. Backend — fatias por segundo no payload do WS

```rust
pub struct LiveSlice { pub sec: u32, pub bytes_in: u64, pub bytes_out: u64 }
// LiveFlowStats ganha: pub slices: Vec<LiveSlice>
```

Construídas do mapa fatiado da janela drenada (agregação por segundo, global).

### 2. Frontend — buckets por segundo epoch + preenchimento retroativo

- O gráfico mantém `Map<sec, {in, out}>` dos últimos 300s
- Cada slice recebida **soma no segundo a que pertence** — inclusive segundos já
  renderizados: o passado recente se completa conforme os flows expiram, igual ao
  comportamento do banco
- Re-render a cada 1s a partir dos buckets; janela fixa de 5 min
- "Mbps agora" = média dos últimos 10s de buckets
- Com dispositivo selecionado o gráfico cai para arrival-based (as fatias são
  globais) — limitação registrada

### 3. Consistência visual entre ao vivo e Histórico

- Cores num módulo único `lib/chartTheme.ts` — par validado pelo dataviz
  (`validate_palette`, surface dark): **entrada `#059669` (verde do sistema),
  saída `#d95926`**; deutan ΔE 10.2, todos os checks PASS
- Legenda idêntica nos dois gráficos: `Entrada` / `Saída` (+ `Sem direção` quando
  houver), usePointStyle, eixo Y com valor absoluto + "Mbps"

## Aceite

- Download constante de 100 Mbps aparece como ~100 Mbps no ao vivo (após a
  expiração dos flows), sem espigões de arrival
- Segundos recentes se preenchem retroativamente
- Mesmas cores e legenda no ao vivo e no Histórico
