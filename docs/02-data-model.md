# Data Model

---

## ClickHouse Tables

### `network_flows_v4` / `network_flows_v6`

Schema atual (pós fases 1.3 + 3.3 + 5.1):

```sql
CREATE TABLE IF NOT EXISTS network_flows_v4 (
    timestamp    DateTime,
    exporter_ip  String,
    src_ip       String,
    dst_ip       String,
    src_port     UInt16,
    dst_port     UInt16,
    protocol     UInt8,
    src_asn      UInt32,
    dst_asn      UInt32,
    packets      UInt64,
    bytes        UInt64,
    flow_count   UInt64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (timestamp, exporter_ip, src_ip, dst_ip, protocol)
SETTINGS index_granularity = 8192
TTL timestamp + INTERVAL 30 DAY   -- valor via FLOW_RETENTION_DAYS (setting)
```

`network_flows_v6` tem o mesmo schema (src_ip/dst_ip armazenam IPv6 como string).

### `flows_hourly_mv` — Materialized View (Fase 6)

Usada pelo anomaly detector para baseline de 24h sem varrer a tabela raw.

```sql
CREATE MATERIALIZED VIEW IF NOT EXISTS flows_hourly_mv
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (hour, exporter_ip, src_ip, dst_ip)
TTL hour + INTERVAL 30 DAY    -- mesmo TTL que tabela raw
AS SELECT
    toStartOfHour(timestamp) AS hour,
    exporter_ip,
    src_ip,
    dst_ip,
    sum(bytes)   AS bytes,
    sum(packets) AS packets
FROM network_flows_v4
GROUP BY hour, exporter_ip, src_ip, dst_ip
```

**Custo:** ~360 MB/mês para 25k assinantes (< 0.1% da tabela raw). Cada ciclo de detecção
(60s) escaneia ~3.6M linhas e conclui em ~30ms.

### Migration Strategy

- DDL auto-aplicado no startup via `setup_tables()` no clickhouse-exporter
- Novas colunas: `ALTER TABLE ... ADD COLUMN IF NOT EXISTS ...`
- Para dev: `DROP TABLE` + restart recria o schema

---

## SQLite Tables (`auth.db`)

### `users`
```sql
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    is_admin      BOOLEAN NOT NULL DEFAULT 0,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
Seed: usuário `admin` com senha `admin123` (bcrypt) criado na primeira execução.

### `exporters`
```sql
CREATE TABLE IF NOT EXISTS exporters (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ip_address  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    location    TEXT,
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)
```

### `settings`
```sql
CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL DEFAULT '',
    label       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    group_name  TEXT NOT NULL DEFAULT ''
)
```
Keys pré-definidas: `OWN_ASN_LIST`, `INTERNAL_PREFIXES`, `ORG_NAME`, `TIMEZONE`,
`FLOW_RETENTION_DAYS`, `MAX_TOP_TALKERS`, `DEFAULT_WINDOW_MIN`, `ALERT_COOLDOWN_MIN`,
`UPLOAD_INVERSION_FACTOR`, `ATTACK_PPS_THRESHOLD`, `ATTACK_PKT_SIZE_MAX`,
`SESSION_TIMEOUT_HOURS`, `COLLECTOR_WORKERS`.

Seed via `INSERT OR IGNORE` — valores editados pelo operador são preservados entre reinícios.

### `alert_rules` (Fase 6)
```sql
CREATE TABLE IF NOT EXISTS alert_rules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    exporter_id INTEGER REFERENCES exporters(id) ON DELETE CASCADE,
                -- NULL = aplica a todos os exporters
    rule_type   TEXT NOT NULL CHECK(rule_type IN ('upload_inversion','attack_signature')),
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    params      TEXT NOT NULL DEFAULT '{}',  -- JSON com parâmetros da regra
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

Parâmetros JSON por `rule_type`:
- `upload_inversion`: `short_window_min`, `history_window_min`, `inversion_ratio`, `min_hist_download_ratio`, `min_upload_mbps`, `cooldown_min`
- `attack_signature`: `window_min`, `min_pps`, `max_avg_pkt_bytes`, `min_total_packets`, `attack_ports[]`, `cooldown_min`

### `telegram_config` (Fase 6)
```sql
CREATE TABLE IF NOT EXISTS telegram_config (
    id           INTEGER PRIMARY KEY CHECK(id = 1),  -- singleton
    bot_token    TEXT NOT NULL DEFAULT '',
    chat_id      TEXT NOT NULL DEFAULT '',
    enabled      BOOLEAN NOT NULL DEFAULT 0,
    min_severity TEXT NOT NULL DEFAULT 'warning'
                 CHECK(min_severity IN ('info','warning','critical')),
    updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
Seed: `INSERT OR IGNORE INTO telegram_config (id) VALUES (1)`.

### `alert_events` (Fase 6)
```sql
CREATE TABLE IF NOT EXISTS alert_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id       INTEGER REFERENCES alert_rules(id) ON DELETE SET NULL,
    exporter_ip   TEXT NOT NULL,
    src_ip        TEXT NOT NULL,
    alert_type    TEXT NOT NULL,
    severity      TEXT NOT NULL CHECK(severity IN ('warning','critical')),
    message       TEXT NOT NULL,
    -- campos upload_inversion
    upload_bytes  INTEGER,
    download_bytes INTEGER,
    -- campos attack_signature
    pps           REAL,
    avg_pkt_bytes REAL,
    attack_ports  TEXT,
    -- ação tomada
    bgp_announced BOOLEAN NOT NULL DEFAULT 0,  -- true se gerou anúncio BGP automático
    notified      BOOLEAN NOT NULL DEFAULT 0,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
Limpeza automática: eventos > 7 dias removidos no startup.

### `bgp_peers` (Fase 8)
```sql
CREATE TABLE IF NOT EXISTS bgp_peers (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    description  TEXT,
    neighbor_ip  TEXT NOT NULL UNIQUE,
    local_ip     TEXT NOT NULL,
    local_as     INTEGER NOT NULL,
    peer_as      INTEGER NOT NULL,
    hold_time    INTEGER NOT NULL DEFAULT 90,
    md5_password TEXT,            -- NULL = sem autenticação MD5
    enabled      BOOLEAN NOT NULL DEFAULT 1,
    created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
)
```

### `bgp_communities` (Fase 8)
```sql
CREATE TABLE IF NOT EXISTS bgp_communities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    community   TEXT NOT NULL UNIQUE,  -- ex: "65000:9999" ou "65000:100 no-export"
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
`community` aceita múltiplos valores separados por espaço — usado diretamente no comando ExaBGP.

### `bgp_prefixes` (Fase 8)
```sql
CREATE TABLE IF NOT EXISTS bgp_prefixes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    prefix      TEXT NOT NULL UNIQUE,  -- ex: "203.0.113.0/24"
    description TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
Catálogo de prefixos gerenciáveis. Não implica que estão sendo anunciados.

### `bgp_announcements` (Fase 8)
```sql
CREATE TABLE IF NOT EXISTS bgp_announcements (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    prefix         TEXT NOT NULL,
    next_hop       TEXT NOT NULL,           -- ex: "self" ou "192.0.2.254"
    community_id   INTEGER REFERENCES bgp_communities(id) ON DELETE SET NULL,
    peer_id        INTEGER REFERENCES bgp_peers(id) ON DELETE CASCADE,
                   -- NULL = anuncia para todos os peers
    origin         TEXT NOT NULL DEFAULT 'manual',  -- "manual" | "anomaly_detector"
    origin_detail  TEXT,                    -- ex: "alert_event#42"
    announced_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    withdrawn_at   DATETIME                -- NULL = ativo; preenchido ao retirar
)
```
Fonte de verdade do estado de anúncios. Linhas nunca são deletadas — `withdrawn_at`
é preenchido ao retirar. Na startup, todos com `withdrawn_at IS NULL` são re-anunciados.

### `bgp_sessions` (Fase 8)
```sql
CREATE TABLE IF NOT EXISTS bgp_sessions (
    peer_id    INTEGER PRIMARY KEY REFERENCES bgp_peers(id) ON DELETE CASCADE,
    state      TEXT NOT NULL DEFAULT 'unknown',  -- 'up' | 'down' | 'unknown'
    last_up    DATETIME,
    last_down  DATETIME,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
)
```
Populada automaticamente pelo session monitor ao detectar eventos no FIFO `exabgp.out`.

---

## Core Rust Types

### `NormalizedFlow` (`flow-types/src/lib.rs`)

```rust
pub struct NormalizedFlow {
    pub timestamp:          u64,
    pub exporter_ip:        Ipv4Addr,
    pub src_ip:             IpAddrType,
    pub dst_ip:             IpAddrType,
    pub src_port:           u16,
    pub dst_port:           u16,
    pub protocol:           u8,
    pub bytes:              u64,
    pub packets:            u64,
    pub src_asn:            u32,
    pub dst_asn:            u32,
    pub ingress_interface:  u32,
    pub egress_interface:   u32,
    pub tcp_flags:          u8,
}
```

### `DebugFlow` (main.rs — broadcast para `/ws/debug`)

```rust
pub struct DebugFlow {
    pub timestamp_sec: u32,
    pub exporter_ip:   String,
    pub src_ip:        String,
    pub dst_ip:        String,
    pub src_port:      u16,
    pub dst_port:      u16,
    pub protocol:      u8,
    pub bytes:         u64,
    pub packets:       u64,
    pub src_asn:       u32,
    pub dst_asn:       u32,
    pub ingress_if:    u32,
    pub egress_if:     u32,
    pub tcp_flags:     u8,
    pub flow_count:    u64,
}
```

### `AggregationKey` (`aggregator/src/lib.rs`)

```rust
pub struct AggregationKey {
    pub exporter_ip: Ipv4Addr,
    pub src_ip:      IpAddrType,
    pub dst_ip:      IpAddrType,
    pub src_port:    u16,
    pub dst_port:    u16,
    pub protocol:    u8,
    pub src_asn:     u32,
    pub dst_asn:     u32,
}
```

### `LiveFlowStats` (WebSocket `/ws`)

```json
{
  "timestamp_sec": 1720000000,
  "total_bytes": 1048576,
  "per_device": { "10.0.0.1": 786432, "10.0.0.2": 262144 }
}
```

### `AppState` (auth.rs)

```rust
pub struct AppState {
    pub db:               SqlitePool,
    pub ws_tx:            broadcast::Sender<LiveFlowStats>,
    pub debug_tx:         broadcast::Sender<DebugFlow>,
    pub metrics:          Arc<CollectorMetrics>,
    pub license:          Arc<RwLock<LicenseStatus>>,
    pub clickhouse_url:   String,
    // Fase 8:
    // pub bgp_sessions:  Arc<RwLock<HashMap<String, BgpSessionState>>>,
    // pub exabgp_pipe:   String,
    // pub exabgp_config: String,
}
```

---

## Query Patterns (ClickHouse)

### Top-talkers
```sql
SELECT src_ip, sum(bytes) AS total_bytes, sum(packets) AS total_packets
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE
  AND exporter_ip = {exporter_ip}   -- opcional
GROUP BY src_ip ORDER BY total_bytes DESC
LIMIT {limit} FORMAT JSON
```

### ASN traffic
```sql
SELECT src_asn, sum(bytes) AS total_bytes
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE AND src_asn != 0
GROUP BY src_asn ORDER BY total_bytes DESC LIMIT 20 FORMAT JSON
```

### Traffic timeline (1-min buckets)
```sql
SELECT toStartOfMinute(timestamp) AS minute, sum(bytes) AS total_bytes
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {hours} HOUR
GROUP BY minute ORDER BY minute ASC FORMAT JSON
```

### Anomaly detector — upload/download ratio (24h via MV)
```sql
SELECT
    src_ip,
    sumIf(bytes, dst_asn IN ({own_asns})) AS upload_bytes,
    sumIf(bytes, src_asn IN ({own_asns})) AS download_bytes
FROM flows_hourly_mv
WHERE hour >= now() - INTERVAL 24 HOUR
  AND exporter_ip = {exporter_ip}
GROUP BY src_ip
HAVING upload_bytes > download_bytes * {inversion_ratio}
   AND download_bytes > 0
FORMAT JSON
```
