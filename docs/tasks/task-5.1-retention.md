# Task 5.1 — ClickHouse TTL & Retention Policy

**Phase:** 5 (Hardening)  
**Effort:** 20 minutes  
**Files:** `clickhouse-exporter/src/lib.rs`

## Spec

Add TTL to ClickHouse tables so data is automatically purged after 90 days.
Without TTL, the tables grow unbounded.

## Implementation

### 1. Add TTL to CREATE TABLE DDL

```sql
CREATE TABLE IF NOT EXISTS network_flows_v4 (
    ...
)
ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (timestamp, exporter_ip, src_ip, dst_ip, protocol)
TTL timestamp + INTERVAL 30 DAY
SETTINGS index_granularity = 8192
```

Same for `network_flows_v6`.

### 2. Add ALTER TABLE for existing installations

In `setup_tables`, after CREATE TABLE:

```rust
let ttl_alters = [
    "ALTER TABLE network_flows_v4 MODIFY TTL timestamp + INTERVAL 30 DAY",
    "ALTER TABLE network_flows_v6 MODIFY TTL timestamp + INTERVAL 30 DAY",
];
for sql in &ttl_alters {
    if let Err(e) = self.client.query(sql).execute().await {
        // Non-fatal: log and continue. TTL may already be set.
        tracing::warn!("TTL alter warning: {e}");
    }
}
```

`MODIFY TTL` is idempotent in ClickHouse — safe to run on every startup.

### 3. Make retention configurable via env var

```rust
let retention_days: u32 = std::env::var("FLOW_RETENTION_DAYS")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(30);
```

Use `retention_days` in the DDL format string.

## Acceptance Criteria

- Tables created with TTL on fresh ClickHouse
- Existing tables get TTL applied on restart
- `FLOW_RETENTION_DAYS` env var overrides the 90-day default
- `cargo build` succeeds
