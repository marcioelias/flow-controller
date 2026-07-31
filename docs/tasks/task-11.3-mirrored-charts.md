# Task 11.3 — Mirrored Traffic Charts (Frontend)

**Phase:** 11 (Traffic Direction & Mirrored Charts)
**Effort:** 3 hours
**Files:** `frontend/src/views/TrafficHistory.vue`, `frontend/src/views/Dashboard.vue`, `frontend/src/views/Exporters.vue`, `frontend/src/utils/format.ts`

## Spec

Classic ISP-style mirrored chart: **inbound above the zero axis, outbound below**,
same time scale, using the per-direction series from task 11.2.

### 1. TrafficHistory & Dashboard

- Area chart with `in_bytes` plotted positive and `out_bytes` plotted negative
  (render as `-value`, format axis labels with `Math.abs`)
- Distinct colors for in/out; legend shows current/avg/max per direction
- `unknown_bytes > 0` (exporters not sending IE 61): show as a third neutral series
  above the axis, plus a dismissible hint explaining the exporter does not report
  direction
- Tooltip shows both directions at the hovered instant with absolute values

### 2. Fallback

When **all** traffic in the selected window is `unknown` (no exporter reports IE 61),
fall back to the current single-series rendering automatically — no broken mirrored
chart with an empty bottom half.

### 3. Exporter list

Show a per-exporter badge derived from `exporter_bidirectional` /
direction breakdown: `in+out`, `in only`, `sem direção` — so the operator sees at a
glance which routers need `flowDirection` enabled and whether totals may double-count
(task 11.1).

## Acceptance Criteria

- Mirrored chart on TrafficHistory and Dashboard with correct axis formatting
  (no negative byte labels)
- Graceful fallback when direction data is absent
- Exporter list shows the direction badge
- `npm run build` succeeds
