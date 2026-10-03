# Task 17.7 — Classificação de tráfego: interno, CGNAT e internet

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/netclass.rs` (NEW), `collector-core/src/stats.rs`,
`collector-core/src/settings.rs`, `frontend/src/views/TalkerDetail.vue`,
`frontend/src/views/TopTalkers.vue`, `frontend/src/components/NetClassBadge.vue` (NEW)

## Problema

Caso real do Marcio (03/10/2026): o talker `100.68.29.12` (assinante atrás de CGNAT)
mostra ~120 Mbps de upload para `170.231.6.78:22` — o IP público do próprio CGNAT,
dentro do ASN do provedor. O exportador fica **antes** do CGNAT, então o flow traz o
destino que o assinante endereçou; o destino final só é decidido dentro do CGNAT
(port-forward/hairpin). Não há campo no flow que revele isso.

O que o coletor pode fazer é deixar explícito que esse tráfego é **interno** e que o
talker é **assinante CGNAT**, em vez de parecer tráfego de internet.
`OWN_ASN_LIST` / `INTERNAL_PREFIXES` existem em Configurações desde a task 7.5, mas
nunca foram usados (dívida registrada).

## Regras

- **R-01 — Classes de endereço**, nesta precedência:
  1. `cgnat` — dentro de `100.64.0.0/10` (RFC 6598).
  2. `internal` — ASN do flow em `OWN_ASN_LIST`, **ou** IP em `INTERNAL_PREFIXES`,
     **ou** espaço privado/local (RFC 1918, `fc00::/7`, link-local, loopback).
  3. `internet` — o resto.
- **R-02 — Mesma regra em Rust e SQL.** O classificador Rust (rótulos) e a expressão
  ClickHouse (filtros) são gerados a partir das mesmas configurações. CIDRs e ASNs são
  validados no Rust antes de entrar no SQL (entrada inválida é descartada com aviso).
- **R-03 — Talker** (`/api/stats/talker`):
  - `ip_class` do próprio talker.
  - `peer_class` em cada conversa.
  - parâmetro `scope=all|internet|internal` (padrão `all`) filtra série, conversas e
    portas pela classe do **peer** (`internal` inclui `cgnat`).
- **R-04 — Top Talkers** (`/api/stats/top-talkers`): cada linha ganha `ip_class`.
- **R-05 — UI.**
  - Selo ao lado do IP: **CGNAT** (âmbar), **Interno** (azul); internet sem selo.
  - Talker: seletor **Todos · Internet · Interno** acima do gráfico; selo por peer nas
    conversas; com o talker `cgnat` e peer `internal`, dica: "destino interno — o
    destino final é decidido no CGNAT/roteador e não aparece neste flow".
  - Top Talkers: selo na coluna IP.

## Aceite

- AC-01: `100.68.29.12` → `cgnat`; `170.231.6.78` com ASN 52977 em `OWN_ASN_LIST` → `internal`;
  `8.8.8.8` → `internet`.
- AC-02: `scope=internet` no talker acima remove a conversa com `170.231.6.78`.
- AC-03: `INTERNAL_PREFIXES = "10.0.0.0/8, lixo"` → usa `10.0.0.0/8`, ignora `lixo`,
  sem erro de SQL.
- AC-04: sem nenhuma configuração, `100.64/10` ainda vira `cgnat` e RFC 1918 `internal`.
