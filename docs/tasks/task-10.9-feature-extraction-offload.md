# Task 10.9 — Move Feature Extraction Off the Async Runtime

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 30 minutes
**Files:** `collector-core/src/features.rs`, `collector-core/src/main.rs`

## Problem

`features::extract` runs **synchronously inside the ClickHouse exporter's async task**:

```rust
while let Ok(map) = export_rx.recv_async().await {
    ...
    let ml_features = features::extract(now, &map);   // CPU-bound, blocking
```

It makes two full passes over the aggregation map and, for every distinct source IP,
allocates a `HashSet<u32>` and a `HashSet<u16>`. Both those sets and the outer
`HashMap` use the **std default hasher (SipHash)**, not `ahash` like the rest of the
pipeline.

On a busy window (100k+ keys) this occupies a Tokio worker thread for tens of
milliseconds while the ClickHouse insert it is supposed to be issuing waits behind it.
Blocking a Tokio worker also stalls unrelated tasks scheduled on the same thread —
the HTTP API, the detector, the Telegram notifier.

## Implementation

### 1. Use `ahash` for the intermediate maps

```rust
use ahash::RandomState;

let mut per_ip: HashMap<(u32, u32), IpEntry, RandomState> =
    HashMap::with_hasher(RandomState::new());
```

Same for `IpEntry::dst_ips` / `dst_ports` (`HashSet<_, RandomState>`).

### 2. Get it off the async thread

Wrap the call in `tokio::task::spawn_blocking` so the CPU-bound pass runs on the
blocking pool and the exporter task stays free to issue the insert:

```rust
let map = Arc::new(window.map);
let feat_map = Arc::clone(&map);
let ml_features = tokio::task::spawn_blocking(move || {
    features::extract(window_ts, &feat_map)
}).await.unwrap_or_default();
```

The map must be shared rather than moved, since the exporter still needs to iterate it
to build the ClickHouse rows. `Arc` is enough — both readers are read-only.

Alternatively the extraction could move into the worker thread itself. That is
cleaner but couples the worker to the ML pipeline's cadence; `spawn_blocking` keeps
the concerns separate for the same benefit.

## Acceptance Criteria

- No CPU-bound work remains inline in the exporter task
- ML features are unchanged for a given window (same values, same count)
- `cargo build` succeeds
