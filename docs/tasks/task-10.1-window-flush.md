# Task 10.1 — Deterministic Window Flush & Real Window Timestamp

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 45 minutes
**Files:** `collector-core/src/main.rs`, `aggregator/src/lib.rs`

## Problem

Two defects in the 1-second aggregation window, both in `worker_loop`:

### 1. Flush only happens when a packet arrives

```rust
while let Ok(payload) = rx.recv() {      // blocks forever
    ...
    if last_flush.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) { ... }
}
```

`rx.recv()` blocks indefinitely. The `last_flush` check runs only *after* a packet is
processed. If an exporter stops sending (link down, maintenance, low traffic at night),
the final partial window stays in the aggregator forever — never reaches ClickHouse,
never feeds the detector, never feeds the ML pipeline.

### 2. Rows are timestamped at drain time, not window time

The exporter task computes `SystemTime::now()` when it pops a map off the channel
(`main.rs`, exporter task). The map may have been sitting in a 100-slot queue for
several seconds. Every row of that window is then stored with the wrong second,
skewing the timeline view, top-talkers windows and the anomaly detector.

## Implementation

### 1. Timed receive in the worker loop

Replace the blocking `recv()` with `recv_timeout` and move the flush check out of the
packet branch so it runs on every iteration, packet or not:

```rust
loop {
    match rx.recv_timeout(Duration::from_millis(200)) {
        Ok(payload) => { /* parse + aggregate */ }
        Err(flume::RecvTimeoutError::Timeout) => {}
        Err(flume::RecvTimeoutError::Disconnected) => break,
    }

    if last_flush.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) {
        // flush regardless of whether a packet arrived
    }
}
```

### 2. Carry the window timestamp with the batch

Change the export channel payload from a bare map to a struct:

```rust
pub struct FlowWindow {
    pub window_ts: u32, // unix seconds, stamped when the window is closed
    pub map: HashMap<AggregationKey, AggregatedMetrics, ahash::RandomState>,
}
```

The worker stamps `window_ts` at flush time. The exporter task uses `window.window_ts`
for `NetworkFlowV4Row::timestamp`, for `LiveFlowStats::timestamp_sec` and for
`features::extract`, instead of calling `SystemTime::now()`.

## Acceptance Criteria

- With traffic stopped, the last partial window is exported within ~1s
- Rows carry the timestamp of the window that produced them, not the drain time
- `SystemTime::now()` no longer appears in the exporter task
- `cargo build` and `cargo test` succeed
