# Task 2.5 — Per-Exporter Summary Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 20 minutes  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/exporters` endpoint. Returns a summary per exporter for the current window.
Used by the dashboard's device overview panel.

## Query Parameters

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `minutes` | u32 | 5 | Lookback window |

## ClickHouse Query

```sql
SELECT
    exporter_ip,
    sum(bytes)      AS total_bytes,
    sum(flow_count) AS flow_count,
    uniq(src_ip)    AS unique_sources
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE
GROUP BY exporter_ip
ORDER BY total_bytes DESC
FORMAT JSON
```

Run against `network_flows_v6` too; merge by `exporter_ip` in Rust.

## Response

```json
[
  {
    "exporter_ip": "10.0.0.1",
    "total_bytes": 104857600,
    "flow_count": 5000,
    "unique_sources": 120
  },
  {
    "exporter_ip": "10.0.0.2",
    "total_bytes": 52428800,
    "flow_count": 2400,
    "unique_sources": 60
  }
]
```

## Struct

```rust
#[derive(Debug, Serialize)]
pub struct ExporterSummaryRow {
    pub exporter_ip: String,
    pub total_bytes: u64,
    pub flow_count: u64,
    pub unique_sources: u64,
}
```

## Merging v4 + v6

Use a `HashMap<String, ExporterSummaryRow>` keyed by `exporter_ip`.
For each row from v6, look up by `exporter_ip` and add `total_bytes`, `flow_count`, `unique_sources`
(unique_sources is approximate — adding two `uniq()` results is conservative).

## Acceptance Criteria

- Returns rows sorted by `total_bytes` desc
- v4 + v6 merged
- `cargo build` succeeds
