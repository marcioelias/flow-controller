# Task 1.4 — Fix IPv6 Stats Endpoint

**Phase:** 1 (Critical Fixes)  
**Effort:** 20 minutes  
**Files:** `collector-core/src/stats.rs`

## Problem

The protocol stats endpoint only queries `network_flows_v4`, ignoring IPv6 traffic entirely.

```rust
// stats.rs — only v4 is queried
let query = format!("SELECT protocol, sum(bytes) AS total_bytes FROM network_flows_v4 ...");
```

## Implementation

Query both tables and sum the results:

```rust
pub async fn get_protocol_stats(
    client: &reqwest::Client,
    clickhouse_url: &str,
    exporter_ip: Option<&str>,
    minutes: u32,
) -> anyhow::Result<ProtocolStats> {
    let where_clause = match exporter_ip {
        Some(ip) => format!("WHERE exporter_ip = '{}' AND timestamp >= now() - INTERVAL {} MINUTE", ip, minutes),
        None => format!("WHERE timestamp >= now() - INTERVAL {} MINUTE", minutes),
    };

    // Query v4 and v6 separately, then merge
    let sql = format!(
        "SELECT protocol, sum(bytes) AS total_bytes FROM network_flows_v4 {wc} GROUP BY protocol
         UNION ALL
         SELECT protocol, sum(bytes) AS total_bytes FROM network_flows_v6 {wc} GROUP BY protocol
         FORMAT JSON",
        wc = where_clause
    );
    // ... parse and fold into ProtocolStats as before
}
```

Also add an optional `minutes` query parameter (default 5):

```rust
// In the handler
#[derive(Deserialize)]
struct StatsQuery {
    exporter_ip: Option<String>,
    minutes: Option<u32>,
}
// use minutes.unwrap_or(5).min(1440)
```

## Acceptance Criteria

- Protocol stats reflect traffic from both v4 and v6 flows
- `minutes` query param accepted (default 5, capped at 1440)
- `cargo build` succeeds
