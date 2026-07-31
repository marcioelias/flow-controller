# Task 12.1 — NOC Overview API

**Phase:** 12 (NOC Dashboard)
**Effort:** 3 hours
**Files:** `collector-core/src/stats.rs`, `collector-core/src/main.rs`

## Spec

Single endpoint feeding the dashboard's stat tiles in one round-trip:

`GET /api/stats/overview?minutes=60` →

```json
{
  "current_bps_in": 0, "current_bps_out": 0,
  "peak_bps_5m": 0,
  "p95_bps": 0,
  "avg_bps": 0,
  "current_pps": 0,
  "flows_per_sec": 0,
  "active_talkers": 0,
  "active_exporters": 0,
  "total_bytes_24h": 0,
  "alerts_24h": 0,
  "alerts_active": 0,
  "bgp_sessions_up": 0, "bgp_sessions_total": 0,
  "top_protocol": "TCP",
  "sampling_exporters": []
}
```

### Sources

| Field | Source |
|-------|--------|
| `current_bps_*` | last full minute from `network_flows_v4/v6`, split by `direction` (11.2); before Phase 11 lands, `in` carries the total and `out` = 0 |
| `peak_bps_5m` | `max` of 1-minute buckets over 5 min |
| `p95_bps` | `quantile(0.95)` over 1-minute buckets in the window — the billing percentile every ISP watches |
| `avg_bps` | mean over the same buckets |
| `flows_per_sec` | `sum(flow_count)` last minute / 60 |
| `active_talkers` | `uniq(src_ip)` last 5 min |
| `active_exporters` | `uniq(exporter_ip)` last 5 min vs configured count |
| `alerts_*` | SQLite `alert_events` |
| `bgp_sessions_*` | in-memory `bgp_sessions` map |
| `sampling_exporters` | exporters with learned rate > 1 (10.10) |

One ClickHouse query with `sumIf`/`quantile` + one SQLite query + in-memory reads.
Route: user-level auth (`require_auth`), same group as the other stats routes.

## Acceptance Criteria

- Endpoint returns all fields in a single response, < 200 ms on 30d of data
- v4+v6 merged; `minutes` parameter clamps to 1–1440
- `cargo build` succeeds
