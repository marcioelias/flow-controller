# Task 2.4 — Traffic Timeline Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 25 minutes  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/timeline` endpoint. Returns total traffic bucketed by minute,
for use in historical line charts.

## Query Parameters

| Param | Type | Default | Max | Description |
|-------|------|---------|-----|-------------|
| `exporter_ip` | string | — | — | Filter by device |
| `hours` | u32 | 1 | 24 | Lookback window in hours |

## ClickHouse Query

```sql
SELECT
    toUnixTimestamp(toStartOfMinute(timestamp)) AS minute,
    sum(bytes)   AS total_bytes,
    sum(packets) AS total_packets
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {hours} HOUR
  [AND exporter_ip = '{ip}']
GROUP BY minute
ORDER BY minute ASC
FORMAT JSON
```

Run against both `network_flows_v4` and `network_flows_v6`.

Merge v4 + v6 in Rust:
- Build a `BTreeMap<u64, (u64, u64)>` (minute → (bytes, packets))
- Insert/add v4 results, then v6 results
- Collect into sorted vec

## Response

```json
[
  { "minute": 1720000000, "total_bytes": 1048576, "total_packets": 750 },
  { "minute": 1720000060, "total_bytes": 2097152, "total_packets": 1500 }
]
```

`minute` is Unix timestamp (seconds) of the start of each minute bucket.
Frontend converts to local time for display.

## Struct

```rust
#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub minute: u64,
    pub total_bytes: u64,
    pub total_packets: u64,
}
```

## Acceptance Criteria

- Returns chronologically ordered array
- Empty buckets are not included (ClickHouse aggregation naturally omits them)
- v4 + v6 merged by minute bucket
- `hours` capped at 24
- `cargo build` succeeds
