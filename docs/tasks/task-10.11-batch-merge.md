# Task 10.11 — Merge Worker Windows Before Inserting

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 1 hour
**Files:** `collector-core/src/main.rs`

## Problem

Every worker flushes its own map every second, and the exporter task inserts each map
as it arrives:

```
worker_count × 2 tables = 8 INSERTs/second   (at the default 4 workers)
```

ClickHouse creates **one part per insert**. At 8 parts/s that is ~690k parts/day
before merges, against a strong recommendation of no more than ~1 insert/s/table.
The symptoms are background merge pressure, `TOO_MANY_PARTS` under load, and degraded
query latency as the part count climbs.

The serialization is the second half of the problem: one task awaits each insert in
turn, so N workers queue behind a single in-flight insert. If an insert takes 300 ms
and 4 workers each produce a window per second, the exporter falls behind and the
100-slot channel starts dropping whole windows (task 10.2 makes that visible).

## Implementation

### 1. Accumulate before inserting

In the exporter task, drain the channel into a merge buffer and flush on either a
time or a size trigger:

```rust
const EXPORT_FLUSH_SECS: u64 = 5;
const EXPORT_MAX_ROWS: usize = 500_000;
```

Windows are merged by `AggregationKey`, summing `packets` / `bytes` / `flow_count`.
Rows keep the `window_ts` of the window they came from (task 10.1), so merging across
seconds does **not** collapse the time dimension — one merged insert can carry rows
for several distinct timestamps.

This drops the insert rate to `2 INSERTs / 5s` regardless of worker count, and makes
each part large enough for ClickHouse to be happy with it.

### 2. Keep the live WebSocket cadence at 1s

`LiveFlowStats` must stay per-second — it drives the realtime dashboard. Broadcast it
as each window is drained, before merging, not at insert time.

Same for `features::extract`: the ML pipeline expects 1-second windows
(`WINDOW_SECS: f64 = 1.0` in `features.rs`). Extract per window, not per merged batch.

### 3. Enable `async_insert` as a second line of defence

```rust
Client::default().with_option("async_insert", "1").with_option("wait_for_async_insert", "0")
```

Consider this optional and off by default: it moves batching into ClickHouse but
acknowledges the insert before it is durable, which trades a small loss window for
throughput. Gate it behind `CLICKHOUSE_ASYNC_INSERT=true`.

## Acceptance Criteria

- Insert rate is ~2 per `EXPORT_FLUSH_SECS`, independent of `COLLECTOR_WORKERS`
- `system.parts` growth rate drops proportionally
- The realtime dashboard still updates once per second
- ML features are still produced per 1-second window
- `cargo build` succeeds
