# Task 2.2 — ASN Traffic Endpoint

**Phase:** 2 (Analytics)  
**Effort:** 30 minutes  
**Depends on:** Task 1.3 (src_asn/dst_asn columns must exist), Task 3.3 (ASN values must be stored)  
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Add `GET /api/stats/asn` endpoint. Returns top ASNs by p95 rate (bps) over the window.

## Query Parameters

| Param | Type | Default | Options | Description |
|-------|------|---------|---------|-------------|
| `exporter_ip` | string | — | — | Filter by device |
| `minutes` | u32 | 60 | max 1440 | Lookback window |
| `limit` | u32 | 20 | max 100 | Results count |
| `direction` | string | `both` | src, dst, both | Which ASN field to query |

## Ranking metric — p95 rate

ASNs are ranked by the **95th percentile of their rate** over the window, not by
total volume. Same definition as the NOC overview `p95_bps` (task 12.1):

- The window is split into `K = minutes` **complete** 1-minute buckets:
  `timestamp >= toStartOfMinute(now()) - INTERVAL {minutes} MINUTE AND timestamp < toStartOfMinute(now())`.
  The open minute is excluded so a partial bucket never drags the rate down.
- Per bucket, rate = `bytes * 8 / 60` (bps). v4 and v6 bytes of the same ASN in the
  same minute are summed **before** the percentile; for `direction=both`, src and dst
  bytes are summed too (same semantics as before).
- Minutes where the ASN had no traffic count as **0 bps** — otherwise an ASN seen in a
  single burst minute would report that burst as its p95.
- Nearest-rank p95 over the K buckets: rank `r = ceil(0.95 * K)`. With zero-filled
  buckets this is the `(K - r + 1)`-th largest non-zero bucket, or 0 if the ASN has
  fewer non-zero buckets than that.
- `avg_bps = total_bytes * 8 / (K * 60)`.

## ClickHouse Query

One query covers both tables (`UNION ALL` of v4 and v6), so the per-minute sum is
done before the percentile. `{asn_buckets}` is, per table:

- `direction=src`: `SELECT src_asn AS asn, toStartOfMinute(timestamp) AS minute, bytes, packets FROM {table} {filter} AND src_asn != 0`
- `direction=dst`: same with `dst_asn`
- `direction=both`: both of the above, `UNION ALL`

```sql
SELECT asn,
       sum(b) AS total_bytes,
       sum(p) AS total_packets,
       arrayElement(arrayReverseSort(groupArray(b)), {K} - {r} + 1) * 8 / 60 AS p95_bps
FROM (
    SELECT asn, minute, sum(bytes) AS b, sum(packets) AS p
    FROM ({asn_buckets for v4} UNION ALL {asn_buckets for v6})
    GROUP BY asn, minute
)
GROUP BY asn
ORDER BY p95_bps DESC, total_bytes DESC
LIMIT {limit}
FORMAT JSON
```

`arrayElement` out of range returns 0, which is exactly the zero-filled case.
Ties at p95 = 0 (sporadic ASNs) fall back to volume.

## Response

```json
[
  { "asn": 15169, "label": "AS15169", "p95_bps": 48210000, "avg_bps": 31000000,
    "total_bytes": 209715200, "total_packets": 150000 }
]
```

`label` is `format!("AS{}", asn)` in Rust. ASN 0 → "Unknown".
`total_bytes` / `total_packets` are kept for consumers that still need volume.

## Implementation

1. `AsnRow` gains `p95_bps` and `avg_bps` (u64).
2. `build_asn_query` emits the single v4+v6 query above.
3. Handler maps rows and computes `avg_bps`; ordering and limit come from ClickHouse.

## Acceptance Criteria

- Returns empty array (not 500) if ASN columns are 0 (before task 3.3)
- `direction=src|dst|both` all work
- Results merged from v4 + v6 per minute before the percentile
- An ASN active in only 1 of 60 minutes reports `p95_bps = 0`
- An ASN with constant rate X in every minute reports `p95_bps = avg_bps = X`
- `cargo build` succeeds
