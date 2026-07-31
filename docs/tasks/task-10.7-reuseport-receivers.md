# Task 10.7 — SO_REUSEPORT Multi-Receiver

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 1 hour
**Files:** `collector-core/src/main.rs`

## Problem

A single thread performs every `recv_from`, and allocates a fresh `Vec<u8>` per packet:

```rust
let mut buf = [0u8; UDP_BUFFER_SIZE];
loop {
    match udp_socket.recv_from(&mut buf) {
        Ok((size, src_addr)) => {
            ...
            let payload = PacketPayload {
                exporter_ip: src_addr.ip(),
                data: buf[..size].to_vec(),   // one malloc per packet
            };
```

This is the throughput ceiling of the whole collector: one syscall plus one heap
allocation per datagram, serialized on one core, no matter how many worker threads
are configured.

## Implementation

### 1. Enable `SO_REUSEPORT` and bind N sockets

The socket currently sets `SO_REUSEADDR` only. With `SO_REUSEPORT`, N independent
sockets can bind the same `0.0.0.0:2055` and the kernel load-balances incoming
datagrams across them by flow hash — no userspace coordination, no lock, near-linear
scaling.

```rust
fn bind_receiver(port: u16, recv_buf: usize) -> anyhow::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    if let Err(e) = socket.set_recv_buffer_size(recv_buf) {
        tracing::warn!("Could not set UDP recv buffer size: {e}");
    }
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    Ok(socket.into())
}
```

Each receiver gets its own socket **and its own kernel receive buffer**, so total
buffering scales with the receiver count too.

### 2. Spawn N receiver threads

Receiver count from the `COLLECTOR_RECEIVERS` setting, defaulting to
`min(worker_count, 4)` and clamped to 1–16. Each thread runs the existing receive
loop verbatim (whitelist check → worker hash → `try_send`), against its own socket.

The main thread then parks instead of running the loop itself.

### 3. Keep the worker sharding rule

Dispatch stays `hash(exporter_ip) % worker_count`. This is required for correctness:
the template cache is thread-local and templates are per exporter, so all packets
from one exporter must reach the same worker. Any receiver thread may hand a packet
to any worker — that is safe, since the worker channel is MPSC-capable.

## Known limitation (not addressed here)

With a single exporter, one worker still does all parsing while the others idle.
Fixing that requires sharding by `(exporter_ip, source_id)` or sharing the template
cache behind a lock. Tracked separately — see the Phase 10 notes.

## Acceptance Criteria

- `COLLECTOR_RECEIVERS=4` binds 4 sockets on :2055 (verify with `ss -ulnp`)
- Aggregate `packets_received_total` scales with receiver count under a
  multi-exporter load test
- Single-receiver behaviour is unchanged when `COLLECTOR_RECEIVERS=1`
- `cargo build` succeeds
