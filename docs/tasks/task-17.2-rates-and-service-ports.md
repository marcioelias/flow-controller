# Task 17.2 — Taxas em bps nas views analíticas + portas por serviço

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/stats.rs`, `frontend/src/utils/format.ts`,
`frontend/src/views/PortBreakdown.vue`, `frontend/src/views/TopTalkers.vue`,
`frontend/src/views/Dashboard.vue`, `frontend/src/views/TalkerDetail.vue`,
`frontend/src/views/TrafficHistory.vue`

## Problema

Relato do Marcio (03/10/2026) na tela Aplicações / Portas:

1. **SSH com quase o mesmo tráfego de HTTPS.** O endpoint agrupa por `dst_port`.
   Do HTTPS só entra o lado cliente→servidor (requisições/ACKs). O download volta
   com `src_port = 443` para a porta efêmera do cliente e aparece pulverizado como
   "51824 (Other)", "43590 (Other)"… Já o SSH (scp/rsync para a 22) entra inteiro.
2. **Pouco tráfego.** Pelo mesmo motivo, o grosso do volume está em milhares de
   portas efêmeras fora do top N. E o "% Total" é relativo à soma das linhas
   exibidas, não ao total da janela.
3. **MB onde se espera Mbps.** Várias telas mostram volume (bytes) onde a pergunta
   do operador é vazão.

## Regras

- **R-01 — Vazão em bps.** Onde a tela compara tráfego (rankings, tabelas,
  barras, tooltips), o número é taxa em bps/kbps/Mbps/Gbps (`formatBps`). Volume em
  bytes só aparece quando o rótulo diz explicitamente "Volume"/"Total" ou é
  armazenamento/memória/disco.
- **R-02 — Média e p95.** `avg_bps = bytes * 8 / (minutes * 60)`.
  `p95_bps` = nearest-rank p95 dos buckets de 1 minuto com zeros preenchidos —
  mesma definição da task 2.2. Rankings usam p95 (desempate por volume).
- **R-03 — Minutos completos.** Endpoints com taxa usam a janela
  `[toStartOfMinute(now()) - N min, toStartOfMinute(now()))`, para que média e p95
  não sejam puxados para baixo pelo minuto em aberto.
- **R-04 — Porta de serviço.** Portas agrupam por `least(src_port, dst_port)`
  (mesma heurística da task 17.1): os dois sentidos de uma conversa HTTPS caem
  na 443. Porta 0 (ICMP e afins) → serviço "Sem porta".
- **R-05 — % do total real.** O percentual de cada porta é sobre o volume total da
  janela (v4 + v6, mesmo filtro), não sobre a soma das linhas exibidas.

## API

### `GET /api/stats/ports`
Linha passa a ser:
```json
{ "port": 443, "service": "HTTPS", "p95_bps": 812000000, "avg_bps": 640000000,
  "total_bytes": 24000000000, "total_packets": 18000000, "share_pct": 61.4 }
```
`dst_port` é renomeado para `port` (o único consumidor é `PortBreakdown.vue`).

### `GET /api/stats/top-talkers`
Cada linha ganha `p95_bps` e `avg_bps`; ordenação por `p95_bps` desc, desempate
por `total_bytes`. A linha "outros" (licença) recebe só `avg_bps`.

## Telas

| Tela | Antes | Depois |
|------|-------|--------|
| Aplicações / Portas | barras e tabela em MB, `% Total` das linhas | barras p95; tabela Porta · Serviço · p95 · Média · % do total |
| Top Talkers | barras e tabela em MB | barras p95; tabela IP · p95 · Média · Pacotes · Flows |
| NOC — card Top Talkers | barra e rótulo em MB | rótulo em bps (média 5 min); tooltips entrada/saída em bps |
| NOC — tabela de exporters | Entrada/Saída/Volume em MB | Entrada/Saída/Total em bps (média 5 min) |
| NOC — heatmap 7 dias | tooltip em MB | tooltip em bps (média da hora); subtítulo "Taxa média por hora" |
| Talker — conversas e portas | ↓/↑ em MB | ↓/↑ em bps (média da janela) |
| Talker — tiles de volume | "Volume download/upload" | mantém (rótulo explícito, R-01) |
| Histórico — tile Total | "Total" | "Volume total" (mantém bytes) |

Helper no frontend: `bytesToBps(bytes, seconds)` em `utils/format.ts`.

## Aceite

- AC-01: uma conversa `cliente:51000 ↔ servidor:443` soma os dois sentidos na linha 443.
- AC-02: `% do total` das portas exibidas soma ≤ 100% e bate com o volume da janela.
- AC-03: porta presente em 1 de 60 minutos → `p95_bps = 0`, `avg_bps > 0`.
- AC-04: nenhuma tela de tráfego mostra MB/GB sem rótulo "Volume"/"Total".
