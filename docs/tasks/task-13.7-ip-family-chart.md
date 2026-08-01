# Task 13.7 — Banda por versão de IP (IPv4 × IPv6)

**Phase:** 13 (Fidelidade Temporal)
**Effort:** 45m
**Files:** `collector-core/src/stats.rs`, `frontend/src/views/TrafficHistory.vue`, `frontend/src/lib/chartTheme.ts`

## Motivação

O dado já vive separado (`network_flows_v4` / `network_flows_v6`), mas a API fundia
tudo. Para ISP, a proporção v4×v6 é métrica de planejamento (CGNAT, peering, custo
de transporte v4).

## Implementação

- `GET /api/stats/timeline`: coluna literal `fam` em cada braço do UNION marca a
  tabela de origem; `TimelinePoint` ganha `v4_bytes` / `v6_bytes`
- Histórico: toggle **Direção | Versão IP** — modo família plota duas áreas
  positivas (não espelhado; família não tem eixo natural de cima/baixo)
- Cores: IPv4 `#00a870` (verde), IPv6 `#8b5cf6` (violeta) — par validado
  (deutan ΔE 23+, todos os checks)

## Aceite

- Toggle alterna os modos sem recarregar dados
- Soma v4+v6 = total por bucket
- Par de cores validado pelo script do dataviz
