# Task 10.6 — Bounded Queues & UDP Receive Error Backoff

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 30 minutes
**Files:** `collector-core/src/main.rs`

## Problem

### 1. `QUEUE_CAPACITY = 1_000_000` is a memory bomb

Each worker channel holds up to 1M `PacketPayload`, and each payload owns a `Vec<u8>`
of the raw datagram (typically ~1400 bytes, up to 64 KB). The queue is bounded by
*count*, not by *bytes*:

```
4 workers × 1_000_000 × ~1400 B ≈ 5.6 GB
```

The queue only fills when parsing falls behind reception, which is exactly the
overload case — so the collector's response to overload is to OOM instead of to shed
load. Dropping early is the correct behaviour for a UDP telemetry collector.

### 2. `recv_from` errors spin the CPU

```rust
Err(e) => tracing::error!("UDP recv error: {}", e),
```

A persistent socket error (`ENETDOWN`, `ENOTCONN`, interface removed) turns the
receive loop into a tight loop emitting log lines at full CPU, which also floods the
log destination.

## Implementation

### 1. Right-size the queue

```rust
const QUEUE_CAPACITY: usize = 65_536; // ~90 MB/worker worst case at 1400 B
```

At 65k packets of headroom per worker, a worker has ~0.5s of buffer at 128k pps
before shedding — enough to ride out a GC-free stall, small enough to bound RSS.
Make it overridable via the `COLLECTOR_QUEUE_CAPACITY` env var for large deployments.

### 2. Classify receive errors

```rust
Err(e) => match e.kind() {
    std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock => continue,
    _ => {
        tracing::error!("UDP recv error: {e}");
        std::thread::sleep(Duration::from_millis(100)); // avoid hot-spinning
    }
},
```

`EINTR` is normal and must not be logged. Everything else gets a 100 ms backoff so a
broken socket costs ~10 log lines/s instead of millions.

## Acceptance Criteria

- Sustained overload sheds packets (visible in `packets_dropped_total`) instead of
  growing RSS without bound
- `COLLECTOR_QUEUE_CAPACITY` overrides the default
- A forced socket error does not saturate a core
- `cargo build` succeeds
