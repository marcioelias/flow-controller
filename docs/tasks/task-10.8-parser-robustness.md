# Task 10.8 — Parser Robustness

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 1 hour
**Files:** `netflow-parser/src/lib.rs`

## Problem

Three defects in `netflow-parser`, all triggered by templates the parser does not
expect rather than by malicious input:

### 1. Variable-length IEs silently discard the whole data set

IPFIX (RFC 7011 §7) encodes variable-length information elements with a template
length of `0xFFFF`; the actual length is a prefix in each record. The parser sums
template field lengths to get a fixed `record_size`:

```rust
let record_size: usize = template.fields.iter().map(|f| f.length as usize).sum();
```

One `0xFFFF` field makes `record_size` ≥ 65535, the `while d_ptr + record_size <=
set_data.len()` loop never runs, and **every flow in that set is dropped with no
error**. Exporters that include `applicationName`, `interfaceName` or similar hit
this and appear to export nothing.

### 2. `read_uint` overflows silently on long fields

```rust
fn read_uint(bytes: &[u8]) -> u64 {
    let mut res = 0u64;
    for &b in bytes { res = (res << 8) | (b as u64); }
    res
}
```

A field declared longer than 8 bytes shifts the high bytes off the top and returns a
garbage value — reported as a legitimate byte/packet count.

### 3. Attacker-influenced `Vec::with_capacity`

```rust
let mut fields = Vec::with_capacity(field_count);  // field_count up to 65535
```

`field_count` comes straight off the wire. The exporter whitelist limits exposure,
but the allocation should be clamped to what the set can actually contain.

## Implementation

### 1. Handle variable-length fields

Mark them in the template and decode per record:

- Add `const VARLEN: u16 = 0xFFFF;`
- In `parse_data_set`, if any field has `length == VARLEN`, take the per-record path:
  walk fields one at a time, reading the 1-byte length prefix (or `255` + 2-byte
  prefix per RFC 7011) for variable-length fields, and advance a running cursor
  instead of using a fixed `record_size`.
- Variable-length values are not among the IANA IDs we decode, so they are skipped —
  but the record boundary is now correct and the fixed-length fields around them
  parse normally.

### 2. Guard `read_uint`

```rust
#[inline]
fn read_uint(bytes: &[u8]) -> u64 {
    // Fields wider than 8 bytes are not numeric counters we understand;
    // take the low-order 8 bytes rather than silently shifting them out.
    let start = bytes.len().saturating_sub(8);
    bytes[start..].iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
}
```

### 3. Clamp the template allocation

```rust
let max_possible = (set_data.len() - ptr) / 4;
let mut fields = Vec::with_capacity(field_count.min(max_possible));
```

## Tests

Add to the existing `mod tests`:

- `test_ipfix_varlen_field_skipped` — template with a `0xFFFF` field plus
  `IANA_IN_BYTES`; assert the flow is decoded with the correct byte count
- `test_read_uint_oversized_field` — 12-byte field decodes to its low 8 bytes
- `test_template_field_count_clamped` — template claiming 65535 fields in a 20-byte
  set does not over-allocate and does not panic

## Acceptance Criteria

- Exporters sending variable-length IEs decode normally
- No panic or garbage counter on malformed templates
- `cargo test -p netflow-parser` passes, including the three new tests
