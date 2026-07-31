# Task 11.2 — Direction-Aware Stats API

**Phase:** 11 (Traffic Direction & Mirrored Charts)
**Effort:** 2 hours
**Files:** `collector-core/src/stats.rs`

## Spec

Extend the analytics endpoints to return traffic split by direction, so the frontend
can render mirrored (in above axis / out below axis) charts.

### 1. Timeline

`GET /api/stats/timeline` gains per-direction series:

```json
{
  "minute": 1753980000,
  "in_bytes": 123,  "in_packets": 4,
  "out_bytes": 456, "out_packets": 7,
  "unknown_bytes": 0, "unknown_packets": 0
}
```

SQL: `sumIf(bytes, direction = 0)`, `sumIf(bytes, direction = 1)`,
`sumIf(bytes, direction = 255)` — one pass, no extra scan.

Keep `total_bytes` / `total_packets` in the response for backward compatibility
(`total = in + out + unknown`).

### 2. Top talkers & exporter summary

Same `sumIf` split on `/api/stats/top-talkers` and `/api/stats/exporters`:
`in_bytes` / `out_bytes` / `unknown_bytes` per row.

### 3. Semantics

Direction is the **router's perspective** (IE 61): `0` = ingress (traffic entering
the exporter interface), `1` = egress. The API passes it through without
reinterpretation; naming in the UI ("download/upload") is a frontend concern
(task 11.3).

## Acceptance Criteria

- Timeline returns the three series plus totals, v4+v6 merged as today
- Old clients reading `total_bytes` keep working
- Queries stay single-pass (`sumIf`, no UNION per direction)
- `cargo build` succeeds
