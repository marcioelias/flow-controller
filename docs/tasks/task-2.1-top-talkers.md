# Task 2.1 — Top-Talkers Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 30 minutes  
**Depends on:** Task 1.3 (ASN columns must exist for future enrichment; not strictly required here)  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/top-talkers` endpoint.

## Query Parameters

| Param | Type | Default | Max | Description |
|-------|------|---------|-----|-------------|
| `exporter_ip` | string | — | — | Filter by device IP |
| `minutes` | u32 | 5 | 1440 | Lookback window |
| `limit` | u32 | 20 | 100 | Number of results |

## ClickHouse Query

```sql
SELECT
    src_ip,
    sum(bytes)      AS total_bytes,
    sum(packets)    AS total_packets,
    sum(flow_count) AS flow_count
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE
  [AND exporter_ip = '{ip}']
GROUP BY src_ip
ORDER BY total_bytes DESC
LIMIT {limit}
FORMAT JSON
```

Run same query against `network_flows_v6` and merge, sort, re-limit.

## Response

```json
[
  {
    "src_ip": "192.168.1.100",
    "total_bytes": 104857600,
    "total_packets": 75000,
    "flow_count": 250
  }
]
```

## Implementation Steps

1. Add `TopTalkerRow` struct in `stats.rs`:
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct TopTalkerRow {
    pub src_ip: String,
    pub total_bytes: u64,
    pub total_packets: u64,
    pub flow_count: u64,
}
```

2. Add query function `get_top_talkers(client, url, exporter_ip, minutes, limit) -> Vec<TopTalkerRow>`.

3. Parse ClickHouse JSON response. ClickHouse returns `{ "data": [...] }` for FORMAT JSON.
   Parse with `serde_json::Value`, extract `data` array.

4. Add `#[derive(Deserialize)]` query struct:
```rust
struct TopTalkersQuery {
    exporter_ip: Option<String>,
    minutes: Option<u32>,
    limit: Option<u32>,
}
```

5. Add handler `get_top_talkers_handler` following the pattern of `get_protocol_stats_handler`.

6. Register route in `main.rs`:
```rust
.route("/stats/top-talkers", get(stats::get_top_talkers_handler))
```
Apply the same `require_auth` middleware layer as other stats routes.

## Acceptance Criteria

- `GET /api/stats/top-talkers` returns array sorted by `total_bytes` descending
- `exporter_ip`, `minutes`, `limit` query params work
- Returns `[]` (empty array) if no data, not a 500
- `cargo build` succeeds
