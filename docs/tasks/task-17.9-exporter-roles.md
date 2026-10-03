# Task 17.9 — Papel do exportador (borda, BNG, CGNAT)

**Phase:** 17 (Drill-down por IP)
**Files:** `collector-core/src/exporters.rs`, `collector-core/src/backup.rs`, `collector-core/src/stats.rs`,
`collector-core/src/enforcement.rs`, `collector-core/src/main.rs`,
`frontend/src/components/DeviceSelect.vue` (NEW), `frontend/src/utils/device.ts` (NEW),
`frontend/src/views/ExporterForm.vue`, `frontend/src/views/Exporters.vue`, views com seletor de dispositivo

## Problema

Discussão com o Marcio (03/10/2026). Com flows de várias caixas no mesmo caminho
(EDGE de borda/CDN, CGNAT/A10, BNG), o mesmo pacote é exportado por mais de uma
caixa. Hoje "Todos os Dispositivos" soma tudo: totais do NOC, Histórico, rankings,
gráfico ao vivo e licença inflam 2–3×.

Ao mesmo tempo, cada caixa enxerga uma identidade que as outras não veem: a borda vê
os IPs públicos e o tráfego que cruza o AS; o BNG vê o assinante (100.64); o CGNAT liga
público ↔ privado. A solução **não** é deduplicar flow a flow (com amostragem os pacotes
sorteados não casam), e sim **escolher o ponto de medição** por pergunta.

Decisões do Marcio:
- Papel **por exportador** (não por interface, por enquanto).
- **Licença = maior soma entre os papéis** — cadastrar só BNG ou só CGNAT não usa de graça;
  coletar o mesmo tráfego em 3 caixas não paga 3×.

## Regras

- **R-01 — Papel.** `exporters.role ∈ {borda, bng, cgnat}`, padrão `borda` (instalações
  existentes não mudam de comportamento até a classificação). Editável no cadastro,
  exibido na lista, preservado no backup/restore.
- **R-02 — Escopo de dispositivo.** Todo endpoint de estatística aceita **um** de:
  `exporter_ip` (uma caixa) ou `role` (todas as caixas ativas daquele papel). Sem nenhum
  dos dois → `role=borda`. Papel sem exportador → resultado vazio (não erro).
  Endpoints: protocols, top-talkers, asn, ports, timeline, overview, talker.
  `/api/stats/exporters` continua por exportador e ganha `role` em cada linha.
- **R-03 — Nunca somar papéis diferentes.** Somar caixas do **mesmo** papel é válido
  (duas bordas com links diferentes; dois BNGs com assinantes diferentes).
- **R-04 — Talker.** Sem `exporter_ip`/`role`, o endpoint escolhe o papel em que o IP tem
  mais bytes na janela e responde `role` (escolhido) e
  `seen_on: [{ role, bytes }]` (todos os papéis onde o IP aparece). Um 100.64 abre no BNG.
- **R-05 — Ao vivo.** Totais e fatias globais do WebSocket contam só exportadores
  `borda`. `per_device*` continua por caixa; o frontend soma as caixas de um papel para
  "BNG (todos)" / "CGNAT (todos)" (modo por chegada, como o modo por dispositivo).
- **R-06 — Licença.** `current_bps` = maior entre as somas por papel na janela de
  medição. Exportador sem cadastro conta como `borda`.
- **R-07 — Seletor de dispositivo** único (`DeviceSelect`) em todas as telas:
  "Borda — total do AS" (padrão), "BNG — todos", "CGNAT — todos" (só papéis com
  exportador), depois cada caixa agrupada por papel. Valor: `role:<papel>` ou o IP.
- **R-08 — NOC.** Tiles, top talkers, top ASNs, heatmap e protocolos seguem o seletor.

## API

- `GET /api/exporters`, `/api/exporters/enabled`, `/api/exporters/:id` → `role`.
- `POST/PUT /api/exporters` aceitam `role` (`borda|bng|cgnat`; outro → 400).
- Stats: param `role` (lista branca) ao lado de `exporter_ip`.
- `GET /api/stats/talker` → `role`, `seen_on`.

## Fora de escopo

- Papel por interface / interfaces externas (pode vir depois, com SNMP).
- Correlação de alertas entre caixas; regras continuam por exportador.
- Mapeamento público → 100.64 por log de NAT do A10 (flows com NAT já são lidos — task 17.8).

## Aceite

- AC-01: com EDGE (borda) e BNG exportando o mesmo tráfego, "Borda — total do AS" mostra
  só o volume do EDGE.
- AC-02: só um BNG cadastrado → licença mede o tráfego do BNG.
- AC-03: borda 5 Gbps + BNG 4 Gbps + CGNAT 4 Gbps → licença mede 5 Gbps.
- AC-04: talker de um 100.64 visto só no BNG abre com `role=bng`.
- AC-05: `role=xyz` → tratado como ausente (`borda`), sem erro de SQL.
