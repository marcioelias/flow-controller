# Task 17.8 — Campos de NAT (post-NAT) do NetFlow v9 / IPFIX

**Phase:** 17 (Drill-down por IP)
**Files:** `netflow-parser/src/lib.rs`, `flow-types/src/lib.rs`, `aggregator/src/lib.rs`,
`clickhouse-exporter/src/lib.rs`, `collector-core/src/main.rs`, `collector-core/src/stats.rs`,
`frontend/src/views/TalkerDetail.vue`, `frontend/src/views/DebugConsole.vue`

## Problema

Quando o exportador é o próprio equipamento de NAT/CGNAT, o template pode trazer os
endereços depois da tradução. Hoje o parser lê só IE 8/12/27/28 e descarta o resto,
então o "destino real" (ou o IP público usado) se perde. Além disso, templates v9
têm o tipo do campo mascarado com `& 0x7FFF` como se houvesse bit de enterprise — no
v9 não há, e isso corrompe os IDs Cisco NSEL ≥ 32768 (ex.: 40001 → 7233).

## Campos

| IE | Nome | Destino |
|----|------|---------|
| 225 | postNATSourceIPv4Address | `nat_src_ip` |
| 226 | postNATDestinationIPv4Address | `nat_dst_ip` |
| 227 | postNAPTSourceTransportPort | `nat_src_port` |
| 228 | postNAPTDestinationTransportPort | `nat_dst_port` |
| 281 | postNATSourceIPv6Address | `nat_src_ip` |
| 282 | postNATDestinationIPv6Address | `nat_dst_ip` |
| 40001 / 40002 | NSEL XLATE_SRC/DST_ADDR_IPV4 (v9) | `nat_src_ip` / `nat_dst_ip` |
| 40003 / 40004 | NSEL XLATE_SRC/DST_PORT (v9) | `nat_src_port` / `nat_dst_port` |

## Regras

- **R-01** — Template v9 guarda o tipo do campo **sem máscara**; IPFIX segue tratando
  o bit de enterprise como hoje.
- **R-02** — Post-NAT é opcional: ausente = zero (`0.0.0.0` / `::` / porta 0). Endereço
  post-NAT de família diferente do flow (NAT64) é ignorado nesta task.
- **R-03** — Os campos post-NAT entram na chave de agregação (traduções diferentes não
  se fundem) e em colunas novas das duas tabelas, criadas com
  `ADD COLUMN IF NOT EXISTS … DEFAULT` (instalações existentes não precisam recriar).
- **R-04 — Talker:** um IP casa como upload se for `src_ip` **ou** `nat_src_ip`, e como
  download se for `dst_ip` **ou** `nat_dst_ip`. Nas conversas:
  - `peer` = destino real: `nat_dst_ip` quando presente, senão `dst_ip` (upload);
    `src_ip` (download).
  - `translated` = o "outro lado" da tradução do talker (ex.: IP público usado pelo
    assinante, ou o assinante por trás de um IP público); vazio se não houver NAT.
  - UI mostra a coluna **Tradução** só quando alguma conversa tem valor.
- **R-05 — Debug Console** mostra os campos post-NAT quando presentes, para o operador
  verificar se o equipamento exporta NAT.

## Aceite

- AC-01: template IPFIX com 225–228 → `NormalizedFlow` com os quatro campos preenchidos.
- AC-02: template v9 com 40001–40004 → mesmos campos; um campo 40001 não vira 7233.
- AC-03: flow sem NAT → colunas com zero; consultas existentes inalteradas.
- AC-04: talker = IP público do CGNAT lista os assinantes por trás dele na coluna Tradução.
