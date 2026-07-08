# Task 3.1 — IPFIX (v10) Parser

**Phase:** 3 (Protocol Improvements)  
**Effort:** 2–3 hours  
**Files:** `netflow-parser/src/lib.rs`, `flow-types/src/lib.rs`

## Problem

IPFIX (v10) match arm in `parse_packet` is empty:
```rust
10 => { /* TODO: IPFIX */ vec![] }
```

Many modern network devices send IPFIX instead of NetFlow v9.

## IPFIX vs NetFlow v9

IPFIX is structurally similar to NetFlow v9:
- Same template/data record model
- Templates are set ID 2 (v9 uses 0), options templates are set ID 3 (v9 uses 1)
- Data sets use set IDs ≥ 256 (same as v9)
- Header differs: uses `Observation Domain ID` instead of `Source ID`

## IPFIX Header (16 bytes)

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Version (=10)           |            Length             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Export Time                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Sequence Number                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                    Observation Domain ID                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

Fields after header are `Set`s (same structure as v9 FlowSets).

## IPFIX Information Elements (IEs) used

| IE ID | Name | Length |
|-------|------|--------|
| 1 | octetDeltaCount | 8 |
| 2 | packetDeltaCount | 8 |
| 4 | protocolIdentifier | 1 |
| 6 | tcpControlBits | 1 |
| 7 | sourceTransportPort | 2 |
| 8 | sourceIPv4Address | 4 |
| 11 | destinationTransportPort | 2 |
| 12 | destinationIPv4Address | 4 |
| 27 | sourceIPv6Address | 16 |
| 28 | destinationIPv6Address | 16 |
| 56 | sourceMacAddress | 6 (skip) |
| 152 | flowStartMilliseconds | 8 |
| 153 | flowEndMilliseconds | 8 |
| 16 | bgpSourceAsNumber | 4 |
| 17 | bgpDestinationAsNumber | 4 |

Note: Enterprise-specific IEs have a bit set in the field ID (bit 15) and are followed by
a 4-byte Enterprise Number. Skip these during template parsing.

## Implementation

### Step 1: IPFIX header parser

```rust
pub struct IpfixHeader {
    pub version: u16,       // always 10
    pub length: u16,        // total message length including header
    pub export_time: u32,   // Unix seconds
    pub seq_num: u32,
    pub observation_domain_id: u32,
}

fn parse_ipfix_header(cursor: &mut Cursor<&[u8]>) -> Result<IpfixHeader, ParseError>
```

### Step 2: Re-use template cache

The `TemplateKey` uses `source_id: u32` — map `observation_domain_id` to this field.
Template and data set parsing logic is nearly identical to v9; extract shared helpers.

### Step 3: Enterprise IE handling in template records

```rust
// In template record parsing loop:
let field_type = cursor.read_u16::<BigEndian>()?;
let field_length = cursor.read_u16::<BigEndian>()?;
if field_type & 0x8000 != 0 {
    // Enterprise bit set: skip the 4-byte enterprise number
    cursor.set_position(cursor.position() + 4);
    continue; // we don't use enterprise IEs
}
```

### Step 4: Wire into `parse_packet`

```rust
10 => parse_ipfix_message(payload, templates, exporter_ip),
```

### Step 5: Tests

Add tests in `netflow-parser/src/lib.rs` with captured IPFIX byte sequences.
At minimum: one template record packet + one data record packet that correctly produces a `NormalizedFlow`.

## Acceptance Criteria

- IPFIX packets from a real device produce `NormalizedFlow` entries
- Enterprise IEs are silently skipped, not errored
- Existing NetFlow v9 tests still pass
- `cargo test` in `netflow-parser` passes
