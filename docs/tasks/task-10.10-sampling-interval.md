# Task 10.10 — Sampling Interval Support (Options Templates)

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 3 hours
**Files:** `netflow-parser/src/lib.rs`, `template-cache/src/lib.rs`, `flow-types/src/lib.rs`

## Problem

This is the most serious functional gap in the parser.

Options templates are explicitly ignored:

```rust
1 => { /* Options Template — skip */ }        // NetFlow v9
3 => { /* Options Template Set — skip */ }    // IPFIX
```

Every production router at an ISP edge exports **sampled** NetFlow — `1:1000` is a
common default on Cisco/Juniper/Huawei/Mikrotik gear, because unsampled export at
10G+ is not feasible. The sampling rate is announced in an *options data record*
described by an *options template*, both of which the collector discards.

Consequence: with `1:1000` sampling, FlowVision reports **0.1% of real traffic**, and
there is nothing in the UI to indicate it. Top talkers, billing-adjacent volume
numbers, alert thresholds and ML features are all wrong by three orders of magnitude,
consistently and invisibly.

## Implementation

### 1. Parse options templates

**NetFlow v9 (FlowSet ID 1):** header is `template_id (u16)`, `option_scope_length
(u16)`, `option_length (u16)`, then scope fields then option fields, each
`type (u16), length (u16)`.

**IPFIX (Set ID 3):** header is `template_id (u16)`, `field_count (u16)`,
`scope_field_count (u16)`, then all fields, each `type (u16), length (u16)` with the
enterprise bit and 4-byte enterprise number handled as in regular templates.

Store them in the same cache, flagged:

```rust
pub struct Template {
    pub key: TemplateKey,
    pub fields: Vec<TemplateField>,
    pub scope_field_count: u16,   // NEW — 0 for regular data templates
    pub is_options: bool,         // NEW
    pub timestamp: u64,
}
```

### 2. Decode the sampling rate from options data records

Relevant IANA IDs:

| ID | Name | Notes |
|----|------|-------|
| 34 | `samplingInterval` | v9 classic |
| 35 | `samplingAlgorithm` | informational |
| 48 | `flowSamplerId` | v9 sampler binding |
| 49 | `flowSamplerMode` | informational |
| 50 | `flowSamplerRandomInterval` | v9 random sampler |
| 305 | `samplingPacketInterval` | IPFIX |
| 306 | `samplingPacketSpace` | IPFIX — rate = (interval + space) / interval |
| 302 | `selectorId` | IPFIX sampler binding |

When a data set matches an options template, extract the rate and store it per
`(exporter_ip, source_id)`:

```rust
pub struct SamplingCache {
    rates: HashMap<(Ipv4Addr, u32), u32, RandomState>,
}
```

Default rate is `1` (unsampled) when nothing has been learned yet.

### 3. Scale counters

In `parse_data_set`, after decoding a record:

```rust
if sampling_rate > 1 {
    flow.bytes   = flow.bytes.saturating_mul(sampling_rate as u64);
    flow.packets = flow.packets.saturating_mul(sampling_rate as u64);
}
```

Add `sampling_rate: u32` to `NormalizedFlow` so downstream consumers can tell scaled
from unscaled data. **Do not** put it in `AggregationKey` — it would fragment the
aggregation.

### 4. Surface it

- New gauge `exporter_sampling_rate{exporter_ip}` so operators can confirm what was
  learned
- `tracing::info!` once when a rate is learned or changes for an exporter

## Risks

Scaling changes reported volumes for anyone already running sampled exporters — their
graphs will jump by the sampling factor on upgrade. This is the correction of a bug,
not a regression, but it must be called out in the release notes.

## Acceptance Criteria

- A v9 exporter sending an options template with `samplingInterval = 1000` produces
  flows with counters scaled ×1000
- An IPFIX exporter with `samplingPacketInterval` / `samplingPacketSpace` is handled
- Unsampled exporters are unaffected (rate stays 1)
- Unit tests cover both v9 and IPFIX options template decoding
