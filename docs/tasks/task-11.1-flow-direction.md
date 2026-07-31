# Task 11.1 — Flow Direction Through the Pipeline

**Phase:** 11 (Traffic Direction & Mirrored Charts)
**Effort:** 3 hours
**Files:** `netflow-parser/src/lib.rs`, `flow-types/src/lib.rs`, `aggregator/src/lib.rs`, `collector-core/src/main.rs`, `clickhouse-exporter/src/lib.rs`

## Problem

The system has no notion of inbound vs outbound. The parser never decodes
`flowDirection` (IE 61) nor `ingressInterface` (IE 10) / `egressInterface` (IE 14) —
`NormalizedFlow.ingress_interface` / `egress_interface` are always 0. Everything is
summed into a single traffic figure.

Worse: an interface configured to export **both** ingress and egress (standard on
Cisco `ip flow ingress`+`egress`, Huawei NetStream inbound+outbound) exports the same
packet twice, and the collector counts it twice with no indication in the UI.

**Decision (2026-07-31):** direction comes from the router (IE 61, falling back to
interfaces), not from IP/ASN ownership. `OWN_ASN_LIST` / `INTERNAL_PREFIXES` stay as
they are (currently unused by this path).

## Implementation

### 1. Parser — decode the direction fields

```rust
pub const IANA_INGRESS_IFACE: u16 = 10;
pub const IANA_EGRESS_IFACE: u16 = 14;
pub const IANA_FLOW_DIRECTION: u16 = 61; // 0 = ingress, 1 = egress
```

In `parse_data_set`, fill `flow.ingress_interface`, `flow.egress_interface` and a new
field:

```rust
/// Direction as reported by the exporter (IE 61): 0=ingress, 1=egress, 255=unknown
pub direction: u8,
```

Default `255` (unknown) when IE 61 is absent.

### 2. Aggregation — direction is part of the key

Add `direction: u8` to `AggregationKey`. Flows in opposite directions must not merge.

### 3. Storage — new column

```sql
ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS direction UInt8 DEFAULT 255
ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS direction UInt8 DEFAULT 255
```

Values: `0` ingress, `1` egress, `255` unknown. Idempotent alter in `setup_tables`,
same pattern as the ASN columns.

### 4. Double-count visibility

Per exporter, track whether both directions are being received for overlapping
interfaces (IE 61 = 0 and = 1 seen from the same exporter within a window). Expose as
gauge `exporter_bidirectional{exporter_ip}` and a warning banner in the exporter list
UI — the operator must know whether totals can be summed or must be split.

## Notes

- Mikrotik frequently omits IE 61; those flows stay `255` and the mirrored chart
  shows them in a neutral "unclassified" band (task 11.3 decides rendering).
- No fallback classification by ownership in this phase — explicit user decision.

## Acceptance Criteria

- v9 and IPFIX flows with IE 61 carry direction end-to-end into ClickHouse
- Interfaces (IE 10/14) populate `ingress_interface` / `egress_interface`
- Flows without IE 61 arrive with `direction = 255`
- Unit tests: template with IE 61 ingress, egress, and absent
- `cargo test` passes
