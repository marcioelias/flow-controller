# Task 10.2 — Backpressure Metrics & Counter Accuracy

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 45 minutes
**Files:** `metrics/src/lib.rs`, `collector-core/src/main.rs`

## Problem

Data is dropped silently in three places, and the existing counters conflate
unrelated events:

| Location | Issue |
|----------|-------|
| `export_tx.try_send(map)` in `worker_loop` | A whole 1s window is discarded with no log and no metric when the export queue is full |
| `ml_tx.try_send(features)` in the exporter task | Same, for the ML pipeline |
| `insert_batch` error | Logged, but no counter to alert on |
| `flows_received` | Incremented once per **packet**, not per flow — the name lies |
| `packets_dropped` | Incremented both for queue-full **and** for parse errors — two different failures in one number |
| whitelist block | Counted in a local `AtomicUsize` that is never exported |

Without these, a collector silently losing 30% of its traffic looks healthy.

## Implementation

### 1. Rework `CollectorMetrics`

```rust
pub struct CollectorMetrics {
    pub registry: Registry,
    pub packets_received: IntCounter,        // renamed from flows_received
    pub packets_blocked: IntCounter,         // NEW — not in whitelist
    pub packets_dropped: IntCounter,         // queue full only
    pub parse_errors: IntCounter,            // NEW — split out of packets_dropped
    pub flows_decoded: IntCounter,
    pub export_windows_dropped: IntCounter,  // NEW — export queue full
    pub ml_windows_dropped: IntCounter,      // NEW — ML queue full
    pub clickhouse_insert_errors: IntCounter,// NEW
    pub clickhouse_rows_inserted: IntCounter,// NEW
    pub template_cache_size: IntGauge,
    pub export_queue_depth: IntGauge,        // NEW
}
```

Keep `flows_received_total` registered as an alias so existing Grafana panels do not
break; document it as deprecated.

### 2. Wire the counters

- `packets_blocked` in the whitelist rejection branch (replaces the local atomic)
- `parse_errors` in the `Err(_)` arm of `parse_packet`
- `export_windows_dropped` + a rate-limited `tracing::warn!` on `try_send` failure
- `ml_windows_dropped` likewise
- `clickhouse_insert_errors` / `clickhouse_rows_inserted` in the exporter task
- `export_queue_depth` set from `export_tx.len()` on each flush

## Acceptance Criteria

- `GET /metrics` exposes all new counters
- Dropping a window increments `export_windows_dropped_total` and logs at most once per 10s
- `packets_dropped_total` no longer moves on malformed packets
- `cargo build` succeeds
