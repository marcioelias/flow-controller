# Task 3.3 — Store src_asn / dst_asn Through Aggregation

**Phase:** 3 (Protocol Improvements)  
**Effort:** 45 minutes  
**Depends on:** Task 1.3 (ClickHouse columns must exist)  
**Files:** `aggregator/src/lib.rs`, `collector-core/src/main.rs`, `clickhouse-exporter/src/lib.rs`

## Problem

`NormalizedFlow` has `src_asn` and `dst_asn` populated by the parser (from NetFlow fields 16/17 or IPFIX IEs 16/17), but the `AggregationKey` doesn't include them, so they're lost during aggregation. The ClickHouse row builder in `main.rs` inserts 0 for both.

## Implementation

### 1. Add ASN fields to `AggregationKey`

```rust
// aggregator/src/lib.rs
pub struct AggregationKey {
    pub exporter_ip: Ipv4Addr,
    pub src_ip: IpAddrType,
    pub dst_ip: IpAddrType,
    pub src_port: u16,          // already there after task 1.3
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,           // ADD
    pub dst_asn: u32,           // ADD
}
```

Note: Adding ASN to the key increases cardinality. This is acceptable because ASN values
are bounded (most networks have a few hundred unique ASNs in traffic).

### 2. Update `aggregate` to include ASN in the key

```rust
pub fn aggregate(&mut self, flow: &NormalizedFlow) {
    let key = AggregationKey {
        exporter_ip: flow.exporter_ip,
        src_ip: flow.src_ip,
        dst_ip: flow.dst_ip,
        src_port: flow.src_port,
        dst_port: flow.dst_port,
        protocol: flow.protocol,
        src_asn: flow.src_asn,
        dst_asn: flow.dst_asn,
    };
    // ... existing accumulation logic
}
```

### 3. Update row builder in `main.rs`

In the async exporter task, when building `NetworkFlowV4Row` from `(AggregationKey, AggregatedMetrics)`:

```rust
NetworkFlowV4Row {
    timestamp: window_start as u32,
    exporter_ip: key.exporter_ip.to_string(),
    src_ip: key.src_ip.to_string(),
    dst_ip: key.dst_ip.to_string(),
    src_port: key.src_port,
    dst_port: key.dst_port,
    protocol: key.protocol,
    src_asn: key.src_asn,    // ADD
    dst_asn: key.dst_asn,    // ADD
    packets: metrics.packets,
    bytes: metrics.bytes,
    flow_count: metrics.flow_count,
}
```

### 4. Verify parser populates ASN fields

Check `netflow-parser/src/lib.rs` field parsing:
- Field type 16 → `src_asn`
- Field type 17 → `dst_asn`

If these field type constants are missing, add them alongside the existing ones.

## Acceptance Criteria

- After receiving flows with ASN information, ClickHouse rows have non-zero `src_asn`/`dst_asn`
- If the exporter doesn't send ASN data, fields remain 0 (not an error)
- The `/api/stats/asn` endpoint returns non-empty results (requires task 2.2)
- `cargo build` succeeds
