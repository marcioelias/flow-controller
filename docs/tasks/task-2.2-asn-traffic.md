# Task 2.2 — ASN Traffic Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 30 minutes  
**Depends on:** Task 1.3 (src_asn/dst_asn columns must exist), Task 3.3 (ASN values must be stored)  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/asn` endpoint. Returns top ASNs by byte volume.

## Query Parameters

| Param | Type | Default | Options | Description |
|-------|------|---------|---------|-------------|
| `exporter_ip` | string | — | — | Filter by device |
| `minutes` | u32 | 60 | max 1440 | Lookback window |
| `limit` | u32 | 20 | max 100 | Results count |
| `direction` | string | `both` | src, dst, both | Which ASN field to query |

## ClickHouse Query

**direction=src:**
```sql
SELECT src_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {minutes} MINUTE
  AND src_asn != 0
  [AND exporter_ip = '{ip}']
GROUP BY src_asn
ORDER BY total_bytes DESC
LIMIT {limit}
FORMAT JSON
```

**direction=dst:** same but `dst_asn`.

**direction=both:**
```sql
SELECT asn, sum(total_bytes) AS total_bytes, sum(total_packets) AS total_packets FROM (
    SELECT src_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets
    FROM network_flows_v4
    WHERE timestamp >= now() - INTERVAL {minutes} MINUTE AND src_asn != 0
    GROUP BY src_asn
    UNION ALL
    SELECT dst_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets
    FROM network_flows_v4
    WHERE timestamp >= now() - INTERVAL {minutes} MINUTE AND dst_asn != 0
    GROUP BY dst_asn
)
GROUP BY asn
ORDER BY total_bytes DESC
LIMIT {limit}
FORMAT JSON
```

Run same against `network_flows_v6`, merge, sort, re-limit to `limit`.

## Response

```json
[
  { "asn": 15169, "label": "AS15169", "total_bytes": 209715200, "total_packets": 150000 },
  { "asn": 0,     "label": "Unknown", "total_bytes": 5242880,   "total_packets": 3000 }
]
```

`label` is `format!("AS{}", asn)` in Rust. ASN 0 → "Unknown".

## Implementation

1. Add `AsnRow` struct in `stats.rs`.
2. Add `get_asn_stats(client, url, exporter_ip, minutes, limit, direction)` function.
3. Add handler + register route `/stats/asn`.
4. Merge v4 + v6 results in Rust: extend one vec with the other, then sort by `total_bytes` desc, truncate to `limit`.

## Acceptance Criteria

- Returns empty array (not 500) if ASN columns are 0 (before task 3.3)
- `direction=src|dst|both` all work
- Results merged from v4 + v6
- `cargo build` succeeds
