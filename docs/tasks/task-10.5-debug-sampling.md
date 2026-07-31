# Task 10.5 — Debug Console Producer-Side Sampling

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 30 minutes
**Files:** `collector-core/src/main.rs`

## Problem

When any client is connected to `/ws/debug`, the worker does this **per flow**:

```rust
if debug_tx.receiver_count() > 0 {
    let src_ip_str = ...to_string();   // alloc
    let dst_ip_str = ...to_string();   // alloc
    let _ = debug_tx.send(DebugFlow {
        exporter_ip: payload.exporter_ip.to_string(),  // alloc
        ...
    });
}
```

Three `String` allocations plus a broadcast send for every decoded flow. Meanwhile:

- the broadcast channel holds 2048 entries and drops the rest
- `handle_debug_socket` rate-limits itself to one message per 10 ms

So at 100k flows/s the worker does ~300k allocations per second to produce ~100
messages per second of useful output. Over 99.9% of the work is thrown away *after*
paying for it. Opening the Debug Console on a busy collector degrades collection —
a self-inflicted denial of service on the hot path.

Additionally `SystemTime::now()` is computed once per packet **outside** the
`receiver_count()` guard, even though `now` is only consumed inside it.

## Implementation

### 1. Move the clock read inside the guard

`let now = ...` currently sits before the flow loop and is only used by `DebugFlow`.
Move it into the debug branch, computed once per packet, not per flow.

### 2. Rate-limit at the producer

Keep a `last_debug_send: Instant` in the worker loop. Emit at most one `DebugFlow`
per `DEBUG_MIN_INTERVAL` (10 ms), matching what the consumer can actually forward:

```rust
const DEBUG_MIN_INTERVAL: Duration = Duration::from_millis(10);

let debug_active = debug_tx.receiver_count() > 0
    && last_debug_send.elapsed() >= DEBUG_MIN_INTERVAL;
```

With N workers this yields up to `N * 100` flows/s to the console — plenty for
eyeballing traffic, and bounded regardless of load.

### 3. Keep the filter honest

`handle_debug_socket` filters by `src_ip` after the fact, so aggressive producer
sampling could starve a filtered view. Document this: the Debug Console is a sampled
view, not a capture. Note it in the UI copy for `DebugConsole.vue` if trivial;
otherwise leave a comment in the handler.

## Acceptance Criteria

- With a debug client connected under load, worker throughput is within noise of the
  disconnected case
- The console still shows a live flow trickle
- `SystemTime::now()` is not called when no debug client is connected
- `cargo build` succeeds
