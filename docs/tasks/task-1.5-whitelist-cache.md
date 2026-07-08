# Task 1.5 — Move Exporter Whitelist Check Off the Hot Path

**Phase:** 1 (Critical Fixes)  
**Effort:** 45 minutes  
**Files:** `collector-core/src/main.rs`, `collector-core/src/exporters.rs`

## Problem

Every incoming UDP packet triggers a synchronous SQLite query on the receive thread:

```rust
// main.rs — in the hot receive loop
let allowed = rt.block_on(exporters::is_exporter_allowed(&pool, &src_ip.to_string()))
    .unwrap_or(false);
```

`rt.block_on()` blocks the current OS thread until the async SQLite query completes.
At high packet rates (>100k pps) this becomes the bottleneck.

## Implementation

Replace the per-packet SQLite lookup with an in-memory `DashSet` (from the `dashmap` crate)
that is refreshed periodically from SQLite.

### 1. Add dependency

```toml
# collector-core/Cargo.toml
[dependencies]
dashmap = "6"
```

### 2. Build the cache

```rust
use dashmap::DashSet;
use std::sync::Arc;

type AllowedSet = Arc<DashSet<std::net::Ipv4Addr>>;
```

### 3. Populate on startup + refresh loop

```rust
async fn refresh_whitelist(pool: SqlitePool, set: AllowedSet) {
    loop {
        match exporters::fetch_enabled_ips(&pool).await {
            Ok(ips) => {
                set.clear();
                for ip in ips {
                    set.insert(ip);
                }
            }
            Err(e) => tracing::error!("whitelist refresh error: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
```

Add `fetch_enabled_ips` to `exporters.rs`:
```rust
pub async fn fetch_enabled_ips(pool: &SqlitePool) -> Result<Vec<Ipv4Addr>, sqlx::Error> {
    let rows = sqlx::query!("SELECT ip_address FROM exporters WHERE enabled = 1")
        .fetch_all(pool)
        .await?;
    Ok(rows.iter()
        .filter_map(|r| r.ip_address.parse::<Ipv4Addr>().ok())
        .collect())
}
```

### 4. Replace hot-path check

```rust
// Before:
let allowed = rt.block_on(exporters::is_exporter_allowed(&pool, &src_ip_str)).unwrap_or(false);

// After (zero async, zero allocation):
let allowed = allowed_set.contains(&src_ipv4);
```

### 5. Spawn refresh task

```rust
let allowed_set: AllowedSet = Arc::new(DashSet::new());
// initial population
let ips = exporters::fetch_enabled_ips(&pool).await?;
for ip in ips { allowed_set.insert(ip); }
// background refresh
tokio::spawn(refresh_whitelist(pool.clone(), allowed_set.clone()));
```

## Acceptance Criteria

- At startup, the in-memory set is populated before the UDP socket binds
- Packets from enabled exporters are accepted; packets from non-listed IPs are dropped
- Adding/removing an exporter takes effect within 30 seconds (next refresh cycle)
- `cargo build` succeeds
- No `rt.block_on()` call remains in the packet receive loop
