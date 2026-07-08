# Task 1.3 — Add ASN and src_port Columns to ClickHouse Schema

**Phase:** 1 (Critical Fixes)  
**Effort:** 30 minutes  
**Files:** `clickhouse-exporter/src/lib.rs`

## Problem

`NormalizedFlow` has `src_asn`, `dst_asn`, and `src_port` fields that are populated by the parser
but discarded when building `NetworkFlowV4Row` because those columns don't exist in the table.

## Implementation

### 1. Update `NetworkFlowV4Row` and `NetworkFlowV6Row` structs

```rust
#[derive(Row, Serialize)]
pub struct NetworkFlowV4Row {
    pub timestamp: u32,
    pub exporter_ip: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,       // ADD
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,        // ADD
    pub dst_asn: u32,        // ADD
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}
```

Same for `NetworkFlowV6Row`.

### 2. Update `setup_tables` DDL

Replace the `CREATE TABLE IF NOT EXISTS` DDL strings for both tables to include the new columns:

```sql
CREATE TABLE IF NOT EXISTS network_flows_v4 (
    timestamp    DateTime,
    exporter_ip  String,
    src_ip       String,
    dst_ip       String,
    src_port     UInt16,
    dst_port     UInt16,
    protocol     UInt8,
    src_asn      UInt32,
    dst_asn      UInt32,
    packets      UInt64,
    bytes        UInt64,
    flow_count   UInt64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY (timestamp, exporter_ip, src_ip, dst_ip, protocol)
```

### 3. Add ALTER TABLE migration for existing installations

In `setup_tables`, after the `CREATE TABLE IF NOT EXISTS`, run these idempotent alters:

```rust
let alters = [
    "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS src_port UInt16 DEFAULT 0",
    "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS src_asn UInt32 DEFAULT 0",
    "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS dst_asn UInt32 DEFAULT 0",
    "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS src_port UInt16 DEFAULT 0",
    "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS src_asn UInt32 DEFAULT 0",
    "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS dst_asn UInt32 DEFAULT 0",
];
for sql in &alters {
    self.client.query(sql).execute().await?;
}
```

### 4. Update the row-building code in `main.rs`

In the async batch exporter task, find where `NetworkFlowV4Row` is constructed from
aggregated entries and add `src_port`, `src_asn`, `dst_asn` fields. They will be 0 until
task 3.3 stores them through the aggregation key.

## Acceptance Criteria

- `cargo build` succeeds
- `setup_tables()` creates tables with the new columns on a fresh ClickHouse
- `setup_tables()` adds columns without error on an existing ClickHouse with old schema
- Inserted rows have `src_port`, `src_asn`, `dst_asn` present (0-valued initially)
