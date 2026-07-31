# Task 10.3 — ClickHouse Insert Retry with Backoff

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 45 minutes
**Files:** `clickhouse-exporter/src/lib.rs`, `collector-core/src/main.rs`

## Problem

```rust
if let Err(e) = exporter_clone.insert_batch(&v4_batch, &v6_batch).await {
    tracing::error!("Failed to insert batch to ClickHouse: {}", e);
}
```

A single transient failure — ClickHouse restarting, a merge stall, a network blip —
throws away a full second of aggregated traffic. A 30-second ClickHouse outage means
30 seconds of permanently lost data with nothing but an error line to show for it.

## Implementation

### 1. Retry inside the exporter

Add `insert_batch_with_retry` to `ClickhouseExporter`:

```rust
pub async fn insert_batch_with_retry(
    &self,
    v4: &[NetworkFlowV4Row],
    v6: &[NetworkFlowV6Row],
    max_attempts: u32,
) -> Result<()> {
    let mut delay = Duration::from_millis(250);
    for attempt in 1..=max_attempts {
        match self.insert_batch(v4, v6).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt == max_attempts => return Err(e),
            Err(e) => {
                tracing::warn!("ClickHouse insert attempt {attempt} failed: {e}; retrying in {delay:?}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(4));
            }
        }
    }
    unreachable!()
}
```

Default `max_attempts = 4` → ~3.75s of total retry, which is short enough that the
export channel absorbs the backlog rather than stalling the collector.

### 2. Count the failures

On final failure, increment `clickhouse_insert_errors` (task 10.2) so the loss is
visible in Prometheus instead of only in the log.

## Notes

Retrying an insert can duplicate rows if ClickHouse committed the batch but the
response was lost. For traffic accounting this is an acceptable trade against silent
loss; the tables are `MergeTree`, not `ReplacingMergeTree`, so no dedup is attempted.

## Acceptance Criteria

- Stopping ClickHouse for ~3s no longer loses the in-flight batch
- Permanent failure increments `clickhouse_insert_errors_total` and logs once
- `cargo build` succeeds
