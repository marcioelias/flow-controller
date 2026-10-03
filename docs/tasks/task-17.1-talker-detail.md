# Task 17.1 — Análise detalhada de um talker

**Phase:** 17 (Drill-down por IP)
**Files:** `clickhouse-exporter/src/lib.rs`, `collector-core/src/stats.rs`, `collector-core/src/main.rs`,
`frontend/src/views/TalkerDetail.vue` (NEW), `frontend/src/router.ts`,
`frontend/src/views/TopTalkers.vue`, `frontend/src/views/Dashboard.vue`

## Problema

O NOC mostra "Talkers 8.1k", mas não há como escolher um IP e ver o que ele está
fazendo. Pedido do Marcio (03/10/2026): acompanhar o tráfego de um IP "em tempo real"
(upload e download), com quem ele conversa e em quais portas.

Nenhum endpoint consulta `network_flows_*` por IP hoje — todos filtram só por exporter.

## Decisões

- **Upload/download é relativo ao IP**, não ao roteador:
  upload = linhas com `src_ip = IP`; download = linhas com `dst_ip = IP`.
  (Mesma definição de `features.rs`. O `direction` do roteador não é usado aqui.)
- **"Tempo real" = polling no ClickHouse**, não WebSocket. Os flows só chegam quando
  expiram no roteador (~60 s de lag + até 5 s de batch de insert), então os últimos
  segundos preenchem retroativamente — mesma limitação do gráfico ao vivo (task 13.6).
- **Conversas agregadas**, não feed de flows individuais. Uma conversa é
  `(peer, protocolo, porta de serviço)`.
- **Porta de serviço = `least(src_port, dst_port)`**. Heurística: o lado com a menor
  porta é o serviço; a efêmera é descartada para não explodir a cardinalidade.
  Erra quando os dois lados usam portas altas (ex.: P2P) — aceito.
- **Índices de pulo `bloom_filter` em `src_ip` e `dst_ip`** nas duas tabelas. O
  `ORDER BY (exporter_ip, timestamp, src_ip, …)` quase não ajuda filtro por IP;
  sem o índice, janelas de 6–24 h varrem tudo. Partes antigas ficam sem o índice
  até o merge/TTL — não rodamos `MATERIALIZE INDEX` no boot (custo alto, opcional manual).

## API

`GET /api/stats/talker` — auth, mesmo `license_gate` das views analíticas (402 se degradado).

| Param | Tipo | Default | Notas |
|-------|------|---------|-------|
| `ip` | string | — | obrigatório; IPv4 ou IPv6 válido, senão 400. A família escolhe a tabela |
| `minutes` | u32 | 5 | 1–1440 |
| `exporter_ip` | string | — | filtra o dispositivo (evita dupla contagem entre exporters) |

**Bucket da série:** 1 s se `minutes <= 15`, senão 60 s.

Três queries na tabela da família, todas com
`WHERE timestamp >= now() - INTERVAL {minutes} MINUTE AND (src_ip = X OR dst_ip = X)`:

1. **Série:** `toStartOfInterval(timestamp, INTERVAL {bucket} SECOND)` →
   `sumIf(bytes, src_ip = X)` (up) e `sumIf(bytes, dst_ip = X)` (down).
2. **Conversas:** `GROUP BY peer, peer_asn, protocol, port`, onde
   `peer = if(src_ip = X, dst_ip, src_ip)`, `peer_asn` idem com os ASNs,
   `port = least(src_port, dst_port)`. Top 50 por `up + down`.
3. **Portas:** `GROUP BY protocol, port`, com `uniq(peer)`. Top 20 por `up + down`.

Resposta:

```json
{
  "ip": "10.0.0.5",
  "bucket_secs": 1,
  "from": 1791029261,
  "to": 1791029561,
  "up_bytes": 1000, "down_bytes": 35000,
  "up_p95_bps": 0, "down_p95_bps": 0,
  "series": [{ "t": 1791029561, "up_bps": 8000, "down_bps": 240000 }],
  "conversations": [{
    "peer": "8.8.8.8", "peer_asn": 15169, "protocol": 6, "port": 443,
    "up_bytes": 1000, "down_bytes": 35000, "packets": 35,
    "first_seen": 1791029561, "last_seen": 1791029566
  }],
  "ports": [{ "protocol": 6, "port": 443, "up_bytes": 1000, "down_bytes": 35000, "peers": 1 }]
}
```

- `series` é esparsa (só buckets com tráfego); o frontend preenche zeros de `from` a `to`.
- `*_bps = bytes * 8 / bucket_secs`.
- `up_bytes`/`down_bytes` = soma da série.
- `*_p95_bps` = nearest-rank p95 dos buckets da janela com zeros preenchidos
  (mesma regra da task 2.2), `K = minutes * 60 / bucket_secs`.

## Tela

Rota `/talkers/:ip` (`TalkerDetail`, `requiresAuth`), dentro do grupo "Análise".

```
┌───────────────────────────────────────────────────────────────────┐
│ ← 10.0.0.5            [Device ▼] [Últimos 5m ▼]  [↺]            │
├──────────────┬──────────────┬──────────────┬──────────────────────┤
│ Download p95 │ Upload p95   │ Volume down  │ Volume up            │
├──────────────┴──────────────┴──────────────┴──────────────────────┤
│ Gráfico espelho: download (+, COLOR_IN) / upload (−, COLOR_OUT)   │
│ em Mbps; respeita o ⇅ (flip) do operador                          │
├───────────────────────────────────────────┬───────────────────────┤
│ Conversas: Peer · ASN · Proto/Porta ·     │ Portas: Proto/Porta · │
│ ↓ · ↑ · Pacotes · Visto por último        │ ↓ · ↑ · Peers         │
└───────────────────────────────────────────┴───────────────────────┘
```

- Refresh: a cada 5 s com `minutes <= 15`, a cada 60 s acima disso. Subtítulo avisa
  que os últimos ~60 s preenchem retroativamente.
- Peer de conversa é clicável → `/talkers/:peer` (navegação lateral entre IPs).
- Estado vazio: "Nenhum tráfego deste IP na janela."

**Entradas:**
- Linha da tabela de Top Talkers → `/talkers/:ip` (exceto a linha "outros").
- Card "Top Talkers" do NOC: cada IP → `/talkers/:ip`.
- Tile "Talkers" do NOC → `/top-talkers`.
- Campo "Analisar IP" no header de Top Talkers → `/talkers/:ip`.

## Aceite

- AC-01: IP inválido → 400; IPv6 válido consulta `network_flows_v6`.
- AC-02: um flow `X → Y` conta como upload de X; `Y → X` como download de X.
- AC-03: conversas `X:51000 → Y:443` e `X:51001 → Y:443` viram uma conversa `(Y, TCP, 443)`.
- AC-04: série preenchida com zeros no frontend; p95 conta os buckets vazios como 0.
- AC-05: índices `idx_src_ip`/`idx_dst_ip` criados em instalações novas e existentes
  (`ADD INDEX IF NOT EXISTS`, falha não fatal).
- AC-06: licença degradada → 402 e a tela mostra o paywall padrão.

## Fora de escopo

- Feed de flows individuais (já existe o Debug Console por `src_ip`).
- Push via WebSocket por IP.
- Resolução de nome de ASN/rDNS dos peers.
