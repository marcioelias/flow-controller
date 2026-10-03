# Task 17.6 — Escala automática de unidades + barras de rolagem estilizadas

**Phase:** 17 (Drill-down por IP)
**Files:** `frontend/src/utils/format.ts`, `frontend/src/lib/chartTheme.ts`,
`frontend/src/assets/index.css`, views com formatação local de unidade

## Problema

Relatos do Marcio (03/10/2026):

1. O gráfico ao vivo mostra "7700 Mbps" (eixo, tooltip, barra de stats) onde o
   esperado é "7.70 Gbps". Várias telas fixam a unidade em Mbps (`toFixed(1) + ' Mbps'`)
   ou têm formatadores locais duplicados (bytes, pps).
2. A barra de rolagem do menu lateral (e das demais áreas roláveis) usa o estilo
   padrão do sistema operacional — clara e larga sobre o tema escuro.

## Regras

- **R-01 — Formatadores únicos** em `utils/format.ts`, com escala automática:
  - `formatBps(bps)` → bps · kbps · Mbps · Gbps · Tbps (base 1000).
  - `formatMbps(mbps)` → mesma saída, para séries que já estão em Mbps (gráficos).
  - `formatPps(pps)` → pps · kpps · Mpps.
  - `formatBytes(bytes)` → B · KB · MB · GB · TB · PB (base 1024).
- **R-02 — Sem sufixo fixo.** Nenhuma tela escreve "Mbps"/"MB" literal ao lado de um
  número; a unidade vem do formatador. Exceção: rótulos de campo que definem a unidade
  de um input (ex.: "Upload mín (Mbps)" nas regras).
- **R-03 — Gráficos** continuam calculando em Mbps internamente; eixo, tooltip e barra
  de stats exibem com `formatMbps`.
- **R-04 — Barras de rolagem** finas e escuras em todo o app (menu lateral, conteúdo,
  modais, tabelas largas): trilho transparente, polegar zinc com hover mais claro,
  cantos arredondados. Firefox (`scrollbar-width`/`scrollbar-color`) e WebKit
  (`::-webkit-scrollbar`).

## Aceite

- AC-01: 7 700 Mbps aparece como "7.70 Gbps" no eixo, tooltip e stats do gráfico ao vivo.
- AC-02: `grep` por `' Mbps'` / `toFixed(...) + ' M` nas views retorna só rótulos de input.
- AC-03: o menu lateral rola com barra fina escura em Chrome e Firefox.
