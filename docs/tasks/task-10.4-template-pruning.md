# Task 10.4 — Template Cache Pruning

**Phase:** 10 (Collector Performance & Robustness)
**Effort:** 20 minutes
**Files:** `collector-core/src/main.rs`, `template-cache/src/lib.rs`

## Problem

`ThreadLocalTemplateCache::prune_old_templates` exists but is **never called** anywhere
in the codebase. The cache grows monotonically, keyed by
`(exporter_ip, source_id, template_id)`.

Exporters that rotate template IDs, use many observation domains (one per line card),
or are re-addressed over time will keep adding entries that are never reclaimed. Each
entry holds a `Vec<TemplateField>`. Over months of uptime this is an unbounded leak.

## Implementation

### 1. Prune on the flush tick

In `worker_loop`, inside the 1-second flush branch:

```rust
templates.prune_old_templates(now_secs, TEMPLATE_MAX_AGE_SECS);
worker_metrics.template_cache_size.set(templates.len() as i64);
```

`TEMPLATE_MAX_AGE_SECS = 3600` (1 hour). NetFlow v9 exporters resend templates every
few minutes by default; IPFIX exporters resend on a timer or template-refresh count.
One hour is well beyond any standard refresh interval.

### 2. Refresh the timestamp on re-insert

`prune_old_templates` compares against `Template::timestamp`, which is set from the
packet's export time on insert. Since exporters resend templates periodically, a live
template is naturally refreshed and never pruned. No change needed — but confirm that
`insert` overwrites the existing entry (it does: `HashMap::insert`).

### 3. Guard against exporter clock skew

`Template::timestamp` comes from the exporter's own header clock. If an exporter's
clock is far in the past, its templates would be pruned immediately. Use the
collector's own wall clock for both the stored timestamp and the comparison instead
of the packet's `export_time`.

## Acceptance Criteria

- Template cache size stops growing for exporters that stop sending
- `template_cache_size` gauge reflects the pruned count
- A live exporter never loses its template (verified by sustained decode with no
  `parse_errors` increase)
- `cargo build` succeeds
