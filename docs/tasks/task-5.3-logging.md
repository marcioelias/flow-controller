# Task 5.3 — Structured Logging

**Phase:** 5 (Hardening)  
**Effort:** 30 minutes  
**Files:** `collector-core/src/main.rs`, `collector-core/Cargo.toml`

## Spec

Replace the current default `tracing-subscriber` setup with structured JSON logging
for production, and human-readable logging for development.

## Current State

`tracing` and `tracing-subscriber` are already declared as workspace dependencies.
The subscriber is initialized somewhere in `main.rs` (likely `tracing_subscriber::fmt::init()`).

## Implementation

### 1. Add `tracing-subscriber` with JSON feature

In `collector-core/Cargo.toml`:
```toml
[dependencies]
tracing-subscriber = { workspace = true, features = ["env-filter", "json"] }
```

In workspace `Cargo.toml`:
```toml
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

### 2. Environment-aware initialization

```rust
fn init_tracing() {
    let json = std::env::var("LOG_FORMAT").map(|v| v == "json").unwrap_or(false);
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    if json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_current_span(true)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .compact()
            .init();
    }
}
```

Call `init_tracing()` at the very start of `main()`, before anything else.

### 3. Key log events to add/verify

```rust
// Startup
tracing::info!(port = 2055, workers = WORKER_COUNT, "UDP collector started");
tracing::info!(port = 3000, "HTTP/WS server started");

// Per-packet drops (debug level to avoid log spam)
tracing::debug!(src_ip = %src_ip, "packet from unlisted exporter, dropped");
tracing::debug!(worker_id = id, "worker channel full, packet dropped");

// Worker flush
tracing::debug!(worker = id, flows = aggregated.len(), "window flushed");

// ClickHouse errors (error level)
tracing::error!(err = %e, "ClickHouse insert failed");

// Exporter whitelist refresh
tracing::debug!(count = ips.len(), "exporter whitelist refreshed");
```

### 4. docker-compose.yml environment

```yaml
flow-collector:
  environment:
    - LOG_FORMAT=json
    - RUST_LOG=info
```

## Acceptance Criteria

- `LOG_FORMAT=json` produces JSON log lines
- `LOG_FORMAT` unset (default) produces compact human-readable lines
- `RUST_LOG=debug` shows debug-level messages
- `cargo build` succeeds
- No `println!` or `eprintln!` remain in non-test code
