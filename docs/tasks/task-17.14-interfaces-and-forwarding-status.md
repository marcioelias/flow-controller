# Task 17.14 — Interfaces, próximo salto e status de encaminhamento nos flows

**Phase:** 17 (Drill-down por IP)
**Files:** `flow-types/src/lib.rs`, `netflow-parser/src/lib.rs`, `aggregator/src/lib.rs`,
`clickhouse-exporter/src/lib.rs`, `collector-core/src/main.rs`, `collector-core/src/stats.rs`,
`frontend/src/views/TalkerDetail.vue`, `frontend/src/views/DebugConsole.vue`

## Problema

Investigação de 19/10/2026 (alertas ML em IPs de CGNAT offline, BNG `IMA-A01-BNG-001`):
para descobrir que um ICMP *Time Exceeded* entrava e saía pela **mesma interface**
rumo ao próximo salto `172.18.1.41` foi preciso capturar os pacotes NetStream na VM e
decodificá-los à mão. O coletor já lê `ingressInterface` (IE 10) e `egressInterface`
(IE 14) mas os descarta antes de gravar; próximo salto (IE 15/62) e
`forwardingStatus` (IE 89) nem são lidos.

Nota de operação: com índice de 16 bits, o NetStream do Huawei exporta uma numeração
de interfaces própria, diferente do ifIndex do `display interface` (o tráfego normal do
BNG aparece com 6, 7, 10, 11, 27, 29). A opção de índice de 32 bits no NetStream alinha
as duas.

## Regras

- **R-01** — Gravar por linha: `in_if UInt32`, `out_if UInt32`, `next_hop` (IPv4/IPv6
  conforme a tabela; IE 15 ou 62, zero se ausente) e `fwd_status UInt8` (IE 89; 0 se
  ausente). Colunas novas com `ADD COLUMN IF NOT EXISTS … DEFAULT`.
- **R-02** — Os quatro campos entram na chave de agregação (tráfego por interfaces ou
  status diferentes não se funde).
- **R-03** — `fwd_status` segue a RFC 7270: 2 bits altos = 0 desconhecido, 1
  encaminhado, 2 descartado, 3 consumido. Descartados e consumidos continuam gravados
  (são evidência), mas **não entram** nos totais de tráfego, rankings e licença
  (filtro `fwd_status < 128`).
- **R-04** — Talker: conversas mostram interface de entrada → saída e próximo salto.
  Debug Console mostra próximo salto e status decodificado.

## Aceite

- AC-01: registro com IE 10/14/15/89 → colunas preenchidas com os valores do template.
- AC-02: registro com `fwd_status = 0x80` (descartado) não soma no total do NOC.
- AC-03: exportador sem esses campos → colunas zeradas; consultas inalteradas.
