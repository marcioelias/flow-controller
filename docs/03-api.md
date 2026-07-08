# API Reference

Base URL (production): `http://localhost:3000`  
Via nginx proxy: `http://localhost:8080/api/`

All protected endpoints require `Authorization: Bearer <jwt>` header.  
All responses are `application/json`.

---

## Auth

### POST /api/auth/login
Public. No auth required.

**Request:**
```json
{ "username": "admin", "password": "admin123" }
```

**Response 200:**
```json
{
  "token": "<jwt>",
  "user": { "id": 1, "username": "admin", "is_admin": true }
}
```

**Response 401:** `{ "error": "Invalid credentials" }`

---

## Users  (Admin only)

### GET /api/users
Returns all users. No pagination.

**Response 200:** `[{ "id": 1, "username": "admin", "is_admin": true, "created_at": "..." }, ...]`

### POST /api/users
**Request:** `{ "username": "string", "password": "string", "is_admin": false }`  
**Response 201:** Created user object  
**Response 400:** `{ "error": "Username already exists" }`

### PUT /api/users/:id
All fields optional. Password only updated if non-null.  
**Request:** `{ "username"?: "string", "password"?: "string", "is_admin"?: bool }`  
**Response 200:** Updated user object  
**Response 404:** Not found

### DELETE /api/users/:id
**Response 200:** `{ "message": "User deleted" }`  
**Response 404:** Not found

---

## Exporters

### GET /api/exporters  (Admin)
Returns all exporters.

### GET /api/exporters/enabled  (Auth — non-admin)
Returns only enabled exporters. Used by Dashboard device selector.
**Response:** `[{ "id": 1, "ip_address": "10.0.0.1", "name": "Core Router", ... }]`

### GET /api/exporters/:id  (Admin)
**Response 200:** Single exporter object  
**Response 404:** Not found

### POST /api/exporters  (Admin)
**Request:**
```json
{
  "ip_address": "10.0.0.1",
  "name": "Core Router",
  "description": "optional",
  "location": "optional",
  "enabled": true
}
```
**Response 201:** Created exporter  
**Response 400:** Invalid IP or missing fields  
**Response 409:** IP already exists

### PUT /api/exporters/:id  (Admin)
All fields optional (partial update).  
**Response 200:** Updated exporter

### DELETE /api/exporters/:id  (Admin)
**Response 200:** `{ "message": "Exporter deleted" }`

---

## Stats / Analytics  (Auth — non-admin)

All analytics endpoints accept these query parameters:
- `exporter_ip` — filter by specific device (optional; omit for global aggregate)
- `minutes` — lookback window in minutes (default 5, max 1440)

### GET /api/stats/protocols
Returns protocol byte distribution.

**Response 200:**
```json
{ "tcp": 104857600, "udp": 52428800, "icmp": 1048576, "other": 0 }
```

---

### GET /api/stats/top-talkers  ← NEW (task 2.1)
Top source IPs by byte volume.

**Query params:** `exporter_ip?`, `minutes?` (default 5), `limit?` (default 20, max 100)

**Response 200:**
```json
[
  { "src_ip": "192.168.1.100", "total_bytes": 104857600, "total_packets": 75000, "flow_count": 250 },
  ...
]
```

---

### GET /api/stats/asn  ← NEW (task 2.2)
ASN traffic breakdown.

**Query params:** `exporter_ip?`, `minutes?` (default 60), `limit?` (default 20), `direction?` (src|dst|both, default both)

**Response 200:**
```json
[
  { "asn": 15169, "label": "AS15169", "total_bytes": 209715200, "total_packets": 150000 },
  ...
]
```

Note: ASN labels are static strings ("AS{number}") until a GeoIP/ASN database is integrated.

---

### GET /api/stats/ports  ← NEW (task 2.3)
Top destination ports by byte volume.

**Query params:** `exporter_ip?`, `minutes?` (default 5), `limit?` (default 20)

**Response 200:**
```json
[
  { "dst_port": 443, "service": "HTTPS", "total_bytes": 52428800, "total_packets": 40000 },
  { "dst_port": 80,  "service": "HTTP",  "total_bytes": 10485760, "total_packets": 8000 },
  ...
]
```

Port-to-service mapping is a static lookup table in Rust (see task 2.3).

---

### GET /api/stats/timeline  ← NEW (task 2.4)
Traffic volume over time, bucketed by minute.

**Query params:** `exporter_ip?`, `hours?` (default 1, max 24)

**Response 200:**
```json
[
  { "minute": "2024-07-06T10:00:00Z", "total_bytes": 1048576, "total_packets": 750 },
  { "minute": "2024-07-06T10:01:00Z", "total_bytes": 2097152, "total_packets": 1500 },
  ...
]
```

---

### GET /api/stats/exporters  ← NEW (task 2.5)
Per-exporter summary for the current window.

**Query params:** `minutes?` (default 5)

**Response 200:**
```json
[
  {
    "exporter_ip": "10.0.0.1",
    "total_bytes": 104857600,
    "flow_count": 5000,
    "unique_sources": 120
  },
  ...
]
```

---

## WebSocket

### GET /ws
Upgrade to WebSocket. No auth enforced (currently public).

**Push message (every ~1 second):**
```json
{
  "timestamp_sec": 1720000000,
  "total_bytes": 1048576,
  "per_device": {
    "10.0.0.1": 786432,
    "10.0.0.2": 262144
  }
}
```

---

### GET /ws/debug
Upgrade to WebSocket. No auth enforced (internal tool). Streams individual parsed flows
before aggregation. Usado pelo Flow Debug Console.

**Query params:**
- `src_ip` — filter by source IP string (optional; omit for all flows)

**Push message (cada flow recebido):**
```json
{
  "timestamp_sec": 1720000000,
  "exporter_ip":   "10.0.0.1",
  "src_ip":        "192.168.1.100",
  "dst_ip":        "8.8.8.8",
  "src_port":      54321,
  "dst_port":      443,
  "protocol":      6,
  "bytes":         1500,
  "packets":       1,
  "src_asn":       0,
  "dst_asn":       15169,
  "ingress_if":    1,
  "egress_if":     2,
  "tcp_flags":     24,
  "flow_count":    1
}
```

**Comportamento:**
- Rate limit de 10ms por conexão (flows excedentes são descartados silenciosamente)
- Canal broadcast com capacidade 2048; lag detectado via `RecvError::Lagged` (aviso no log)
- O canal só é populado quando há pelo menos 1 cliente conectado (`receiver_count() > 0`)
- Reconectar com novo `src_ip` (ou sem) aplica o filtro da nova conexão

---

## Prometheus

### GET /metrics  ← NEW (task 3.2)
Prometheus text format. No auth (scrape by Prometheus server).

```
# HELP flows_received_total Total UDP packets received
# TYPE flows_received_total counter
flows_received_total 1234567

# HELP flows_decoded_total Flows successfully decoded
# TYPE flows_decoded_total counter
flows_decoded_total 1230000

# HELP packets_dropped_total Packets dropped (queue full or parse error)
# TYPE packets_dropped_total counter
packets_dropped_total 4567

# HELP template_cache_size Current number of cached NetFlow templates
# TYPE template_cache_size gauge
template_cache_size 42
```

---

## Version

### GET /api/version
Public. No auth required.

**Response 200:**
```json
{
  "version":    "1.1.42",
  "name":       "FlowVision",
  "vendor":     "Hahn Tech Desenvolvimento e Consultoria Ltda",
  "rustc":      "rustc 1.82.0 (f6e511eec 2024-10-15)",
  "clickhouse": "24.3.2.23",
  "rust_deps": [
    { "name": "Axum",    "version": "0.7.9" },
    { "name": "Tokio",   "version": "1.51.0" }
  ]
}
```

`rust_deps` é gerado em compile time via `build.rs` parseando `Cargo.lock`.  
`clickhouse` é consultado em runtime (`SELECT version()`) com timeout de 2s; retorna `"unavailable"` se o ClickHouse não responder.

---

## Settings  (Auth — não-admin pode ler; admin pode escrever)

A tabela `settings` no SQLite armazena configurações globais do sistema com keys pré-definidas.
O usuário só pode alterar o `value` — labels, descrições e grupos são definidos em código.

### GET /api/settings
Requer auth (qualquer role).

**Response 200:**
```json
[
  {
    "key":        "OWN_ASN_LIST",
    "value":      "12345,67890",
    "label":      "ASNs Próprios",
    "description": "ASNs da sua rede, separados por vírgula...",
    "group_name": "Rede"
  }
]
```

Retorna todas as settings ordenadas por `group_name, key`.  
Grupos existentes: `Rede`, `Organização`, `Análise`, `Alertas`, `Sessão`, `Coletor`.

### PUT /api/settings/:key
Requer admin.

**Request:** `{ "value": "12345,67890" }`  
**Response 200:** Setting atualizada (mesmo formato do item acima)  
**Response 404:** Key não existe na lista de keys permitidas

> **Importante:** `COLLECTOR_WORKERS` é lido apenas no startup do processo.
> Alterar via API requer reinício do serviço para ter efeito.
> `FLOW_RETENTION_DAYS` também requer reinício para recriar o TTL no ClickHouse.

---

## License

### GET /api/license
Public. No auth required.

**Response 200:**
```json
{
  "valid":        true,
  "licensee":     "Provedor Internet Ltda",
  "tier_label":   "Pro — 10 Gbps",
  "max_bps":      10000000000,
  "max_talkers":  100,
  "expires_at":   "2025-12-31T00:00:00Z",
  "fingerprint":  "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
  "message":      null
}
```

Se nenhuma licença estiver presente, retorna free tier (`max_bps: 100_000_000`, `max_talkers: 1`).

### POST /api/license
Public. No auth required. Valida e aplica uma nova licença.

**Request:** `{ "license": "<base64url_payload>.<base64url_signature>" }`  
**Response 200:** LicenseStatus atualizado  
**Response 422:** Licença inválida (assinatura incorreta, expirada, fingerprint errado)

---

## Alerts  (Admin only — tasks 6.1–6.4)

### GET /api/alerts/rules
Returns all alert rules.

**Response 200:**
```json
[
  {
    "id": 1,
    "name": "BNG PPPoE sustained abuse",
    "exporter_id": 2,
    "rule_type": "sustained",
    "enabled": true,
    "params": {
      "short_window_min": 5,
      "long_window_min": 60,
      "multiplier": 4.0,
      "min_duration_min": 10,
      "min_long_avg_mbps": 1
    },
    "created_at": "2024-07-06T10:00:00Z"
  }
]
```

### POST /api/alerts/rules
**Request:** Rule object without `id`/`created_at`.  
**Response 201:** Created rule  
**Response 400:** Unknown `rule_type` or missing required params

### PUT /api/alerts/rules/:id
Full replace of all fields.  
**Response 200:** Updated rule

### DELETE /api/alerts/rules/:id
**Response 200:** `{ "message": "Rule deleted" }`

### PATCH /api/alerts/rules/:id/toggle
Flips `enabled`. No request body.  
**Response 200:** `{ "id": 1, "enabled": false }`

---

### GET /api/alerts/events
Recent alert events.

**Query params:** `limit?` (default 50, max 500), `offset?` (default 0), `severity?`, `notified?`

**Response 200:**
```json
{
  "total": 142,
  "events": [
    {
      "id": 99,
      "rule_id": 1,
      "exporter_ip": "10.0.1.1",
      "src_ip": "192.168.50.33",
      "alert_type": "sustained",
      "severity": "critical",
      "message": "Tráfego sustentado anômalo: 450 Mbps por 15 min (base: 12 Mbps, ratio 37×)",
      "bytes_short": 2812500000,
      "bytes_long": 75000000,
      "notified": true,
      "created_at": "2024-07-06T03:42:00Z"
    }
  ]
}
```

### DELETE /api/alerts/events
Clears all events.  
**Response 200:** `{ "message": "Events cleared" }`

---

### GET /api/alerts/telegram
Returns Telegram config with token masked.  
**Response 200:**
```json
{
  "bot_token": "***************abcd",
  "chat_id": "-100123456789",
  "enabled": true,
  "min_severity": "warning"
}
```

### PUT /api/alerts/telegram
**Request:**
```json
{
  "bot_token": "7123456789:AAF...",
  "chat_id": "-100123456789",
  "enabled": true,
  "min_severity": "warning"
}
```
All fields required.  
**Response 200:** Masked config (same as GET)

### POST /api/alerts/telegram/test
Sends a test message with current configuration.  
**Response 200:** `{ "ok": true, "message": "Test message sent successfully" }`  
**Response 400:** `{ "error": "Bot token not configured" }`  
**Response 502:** `{ "error": "Telegram API error: ..." }`

---

## BGP  (Admin only — tasks 8.1–8.5)

Todos os endpoints requerem admin. O controle do ExaBGP é feito via named pipe FIFO.
Mudanças de peer (sessão) requerem `POST /api/bgp/apply`; anúncios/withdrawals são imediatos.

### GET /api/bgp/sessions
Estado das sessões BGP (lido do cache em memória — sem I/O de disco).

**Response 200:**
```json
[
  {
    "peer_id": 1, "peer_name": "Upstream Tier-1", "neighbor_ip": "10.0.0.1",
    "state": "up", "last_up": "2026-07-08T09:00:00", "last_down": null, "updated_at": "..."
  }
]
```
Estados: `"up"` | `"down"` | `"unknown"`

---

### GET /api/bgp/peers
Lista peers. `has_md5: true` indica que MD5 está configurado — o valor nunca é exposto.

### POST /api/bgp/peers
```json
{ "name": "string", "neighbor_ip": "10.0.0.1", "local_ip": "10.0.0.2",
  "local_as": 65001, "peer_as": 65000, "hold_time": 90,
  "md5_password": "opcional-ou-null", "enabled": true }
```
**Response 201:** Peer criado  
**Response 409:** `neighbor_ip` já existe

### PUT /api/bgp/peers/:id
Atualização completa.  
**Response 200:** Peer atualizado

### DELETE /api/bgp/peers/:id
**Response 204:** Removido  
**Response 409:** Peer tem anúncios ativos

---

### GET /api/bgp/communities
### POST /api/bgp/communities
```json
{ "name": "Blackhole RTBH", "community": "65000:9999", "description": "..." }
```
`community` aceita múltiplos valores separados por espaço: `"65000:100 no-export"`.  
**Response 201:** Criada

### PUT /api/bgp/communities/:id  |  DELETE /api/bgp/communities/:id
DELETE retorna 409 se há anúncios ativos usando a community.

---

### GET /api/bgp/prefixes
Catálogo de prefixos gerenciáveis (não implica que estão sendo anunciados).

### POST /api/bgp/prefixes
```json
{ "prefix": "203.0.113.0/24", "description": "..." }
```

### PUT /api/bgp/prefixes/:id  |  DELETE /api/bgp/prefixes/:id

---

### GET /api/bgp/announcements
Query params: `active=true` (padrão) | `active=false` (histórico completo).

**Response 200:**
```json
[
  {
    "id": 5, "prefix": "203.0.113.0/24", "next_hop": "self",
    "community_name": "Blackhole RTBH", "community_value": "65000:9999",
    "peer_name": null, "peer_neighbor_ip": null,
    "origin": "manual", "origin_detail": null,
    "announced_at": "2026-07-08T10:00:00", "withdrawn_at": null
  }
]
```
`origin` pode ser `"manual"` ou `"anomaly_detector"` (anúncio automático por alerta).

### POST /api/bgp/announcements
Anuncia imediatamente via FIFO + persiste no banco.

```json
{ "prefix": "203.0.113.0/24", "next_hop": "self", "community_id": 1, "peer_id": null }
```
**Response 201:**
```json
{ "id": 5, "command": "announce route 203.0.113.0/24 next-hop self community [65000:9999]" }
```
**Response 503:** FIFO indisponível (ExaBGP offline)

### DELETE /api/bgp/announcements/:id
Withdraw imediato via FIFO + preenche `withdrawn_at`.  
**Response 200:** `{ "command": "withdraw route 203.0.113.0/24" }`  
**Response 409:** Anúncio já retirado

---

### POST /api/bgp/apply
Regenera `exabgp.conf` a partir do banco e envia `SIGUSR1` ao ExaBGP.
Necessário após criar/editar/deletar peers.

**Response 200:**
```json
{ "peers_configured": 2, "config_written": "/run/exabgp-config/exabgp.conf", "reload_signal": "SIGUSR1" }
```
**Response 400:** Nenhum peer enabled  
**Response 500:** Falha ao escrever arquivo ou sinalizar reload
