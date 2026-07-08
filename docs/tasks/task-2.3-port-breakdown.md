# Task 2.3 — Port/Application Breakdown Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 30 minutes  
**Depends on:** Task 1.3 (src_port column)  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/ports` endpoint. Returns top destination ports by byte volume,
with a human-readable service name.

## Query Parameters

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `exporter_ip` | string | — | Filter by device |
| `minutes` | u32 | 5 | max 1440 |
| `limit` | u32 | 20 | max 100 |

## ClickHouse Query

```sql
SELECT
    dst_port,
    sum(bytes)   AS total_bytes,
    sum(packets) AS total_packets
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE
  [AND exporter_ip = '{ip}']
GROUP BY dst_port
ORDER BY total_bytes DESC
LIMIT {limit}
FORMAT JSON
```

Run against `network_flows_v6` too; merge by `dst_port`, re-sort, re-limit.

## Response

```json
[
  { "dst_port": 443,  "service": "HTTPS",  "total_bytes": 52428800, "total_packets": 40000 },
  { "dst_port": 53,   "service": "DNS",    "total_bytes": 1048576,  "total_packets": 12000 },
  { "dst_port": 8888, "service": "Other",  "total_bytes": 524288,   "total_packets": 500 }
]
```

## Port-to-Service Lookup Table

Add this static function to `stats.rs`:

```rust
fn port_to_service(port: u16) -> &'static str {
    match port {
        20 | 21 => "FTP",
        22      => "SSH",
        23      => "Telnet",
        25      => "SMTP",
        53      => "DNS",
        67 | 68 => "DHCP",
        80      => "HTTP",
        110     => "POP3",
        123     => "NTP",
        143     => "IMAP",
        161     => "SNMP",
        179     => "BGP",
        389     => "LDAP",
        443     => "HTTPS",
        465     => "SMTPS",
        514     => "Syslog",
        587     => "SMTP/TLS",
        636     => "LDAPS",
        993     => "IMAPS",
        995     => "POP3S",
        1433    => "MSSQL",
        1521    => "Oracle",
        3306    => "MySQL",
        3389    => "RDP",
        5432    => "PostgreSQL",
        5900    => "VNC",
        6379    => "Redis",
        8080    => "HTTP-Alt",
        8443    => "HTTPS-Alt",
        9200    => "Elasticsearch",
        27017   => "MongoDB",
        _       => "Other",
    }
}
```

## Acceptance Criteria

- Returns top ports sorted by `total_bytes` desc
- `service` field populated from static lookup
- v4 + v6 merged
- `cargo build` succeeds
