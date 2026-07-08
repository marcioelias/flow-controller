# Task 3.2 — Wire Prometheus Metrics Endpoint

**Phase:** 3 (Protocol Improvements)  
**Effort:** 45 minutes  
**Files:** `collector-core/src/main.rs`, `metrics/src/lib.rs`

## Problem

`CollectorMetrics` is defined but never instantiated. No `/metrics` HTTP endpoint exists.
Counters are never incremented anywhere.

## Implementation

### 1. Instantiate metrics at startup

```rust
// main.rs
let metrics = Arc::new(metrics::CollectorMetrics::new());
```

Pass the `Arc<CollectorMetrics>` to:
- The UDP receive loop (increment `flows_received`)
- Worker threads (increment `flows_decoded`, `packets_dropped`)
- The `AppState` struct (for the HTTP handler)

### 2. Increment counters in the right places

**In the UDP receive loop (main.rs):**
```rust
metrics.flows_received.inc();
```

**In each worker thread after `parse_packet`:**
```rust
match parse_packet(&payload, &mut templates, exporter_ip) {
    Ok(flows) => {
        metrics.flows_decoded.inc_by(flows.len() as u64);
        for flow in flows { aggregator.aggregate(&flow); }
    }
    Err(_) => {
        metrics.packets_dropped.inc();
    }
}
```

**Template cache size gauge** — update after each flush in worker:
```rust
metrics.template_cache_size.set(templates.len() as i64);
```

Add `len()` method to `ThreadLocalTemplateCache`:
```rust
pub fn len(&self) -> usize { self.cache.len() }
```

### 3. Add /metrics HTTP endpoint

In `main.rs`, add the route:
```rust
.route("/metrics", get(metrics_handler))
```

Handler (no auth — Prometheus scrapes externally):
```rust
async fn metrics_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = state.metrics.registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    (
        StatusCode::OK,
        [("Content-Type", prometheus::TEXT_FORMAT)],
        buffer,
    )
}
```

### 4. Update AppState to hold metrics

```rust
pub struct AppState {
    pub db: SqlitePool,
    pub ws_tx: broadcast::Sender<LiveFlowStats>,
    pub metrics: Arc<CollectorMetrics>,   // ADD
    pub clickhouse_url: String,            // ADD (currently passed separately)
}
```

### 5. Thread-safe metrics in worker threads

`Arc<CollectorMetrics>` is already `Send + Sync` because Prometheus counters are atomic.
Clone the `Arc` once per worker thread; no mutex needed.

## Acceptance Criteria

- `GET /metrics` returns Prometheus text format
- `flows_received_total` increments with each UDP packet
- `flows_decoded_total` increments for each flow successfully parsed
- `packets_dropped_total` increments on parse errors or queue-full drops
- `template_cache_size` gauge reflects the per-worker template count (sum of all workers)
- `cargo build` succeeds
