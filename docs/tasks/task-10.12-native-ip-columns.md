# Task 10.12 — Native IPv4/IPv6 Columns

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 2 hours + migration window
**Files:** `clickhouse-exporter/src/lib.rs`, `collector-core/src/main.rs`, `collector-core/src/stats.rs`, `collector-core/src/detector.rs`

> ⚠️ **Requires a data migration. Do not run unattended on a production install.**

## Problem

IP addresses are stored as `String`:

```sql
exporter_ip String,
src_ip String,
dst_ip String,
```

and produced by `to_string()` on every aggregated row:

```rust
exporter_ip: key.exporter_ip.to_string(),
src_ip: src_ip.to_string(),
dst_ip: dst_ip.to_string(),
```

Costs:

- **3 heap allocations per aggregated row**, on the path that runs for every window
- ~13 bytes stored per IPv4 address instead of 4, before compression
- String comparison instead of integer comparison for every `WHERE src_ip = ...`,
  `GROUP BY src_ip` and `ORDER BY` in `stats.rs` and `detector.rs`
- No native `IPv4NumToString` / subnet functions available in queries

ClickHouse has first-class `IPv4` and `IPv6` types that fix all four.

## Implementation

### 1. New schema

```sql
exporter_ip IPv4,
src_ip IPv4,       -- IPv6 in network_flows_v6
dst_ip IPv4,
```

`ORDER BY (exporter_ip, timestamp, src_ip, dst_ip, protocol)` — see note below.

### 2. Row types

```rust
#[derive(Row, Serialize)]
pub struct NetworkFlowV4Row {
    pub timestamp: u32,
    pub exporter_ip: u32,   // serialized as IPv4
    pub src_ip: u32,
    pub dst_ip: u32,
    ...
}
```

`u32::from(ipv4_addr)` replaces `to_string()` — no allocation. For v6, `[u8; 16]`.

### 3. Query updates

Every query in `stats.rs`, `detector.rs`, `backup.rs` and `ml_api.rs` that selects an
IP must wrap it: `IPv4NumToString(src_ip) AS src_ip`, so the JSON returned to the
frontend is unchanged. Filters take the inverse: `src_ip = toIPv4('10.0.0.1')`.

### 4. Migration path

`ALTER TABLE ... MODIFY COLUMN src_ip IPv4` rewrites the column and is not safe to run
blind on a large table. Ship it as an **explicit, opt-in step**, not as part of
`setup_tables`:

1. `setup_tables` creates new installs with native types
2. Existing installs are detected by querying `system.columns` for the column type
3. If a `String` schema is found, log a prominent warning with the migration command
   and **keep using the String path** — do not auto-migrate
4. Provide `scripts/migrate-ip-columns.sh` doing create-new → `INSERT SELECT` →
   `RENAME`, with the estimated size and duration printed up front

This means the code must support both schemas during the transition, or the migration
must be gated behind a version bump with clear release notes. Decide before starting.

## Related: `ORDER BY` key

The current key is `(timestamp, exporter_ip, src_ip, dst_ip, protocol)`. Leading with
a second-granularity timestamp puts the highest-cardinality column first, which hurts
compression and makes the primary index nearly useless for the IP-filtered queries
that top-talkers and the detector actually run. Since the tables are already
partitioned by day, a lower-cardinality prefix is likely better:

```sql
ORDER BY (exporter_ip, timestamp, src_ip, dst_ip, protocol)
```

Benchmark both against a real dataset before committing — this is a guess until
measured, and it can only be changed by rebuilding the table anyway, so it should
ride along with the IP column migration rather than being a separate rewrite.

## Acceptance Criteria

- Fresh installs get native `IPv4` / `IPv6` columns
- No `to_string()` on the row-building path
- All API responses return IPs in the same string form as before
- Existing installs are detected and warned, never silently migrated
- Migration script is tested against a table with >100M rows
