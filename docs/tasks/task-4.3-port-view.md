# Task 4.3 — Protocol/Port Breakdown View (Frontend)

**Phase:** 4 (Dashboard Views)  
**Effort:** 1 hour  
**Depends on:** Task 2.3 (backend endpoint)  
**Files:** `frontend/src/views/PortBreakdown.vue` (NEW), `frontend/src/router.ts`

## Spec

New route `/ports`. Shows top destination ports and the traffic split between
known applications (HTTPS, DNS, etc.) and "other".

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│  Applications / Ports      [Device ▼] [Last 5m ▼]       [↺] │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  [Bar chart — top 15 ports, stacked by known/unknown]       │
│                                                              │
├──────────┬────────────┬─────────────┬─────────────┬─────────-┤
│  Port    │  Service   │  Traffic    │  Packets    │  % Total │
├──────────┼────────────┼─────────────┼─────────────┼──────────┤
│  443     │  HTTPS     │  50 MB      │  40,000     │  48.2%   │
│  53      │  DNS       │   1 MB      │  12,000     │   1.0%   │
└──────────┴────────────┴─────────────┴─────────────┴──────────┘
```

## Bar Chart

Horizontal bar, sorted by bytes desc. Top 15 ports.
- Known services (`service !== 'Other'`): color `#10b981` (emerald)
- Unknown ports: color `#6b7280` (gray)

## `% Total` Column

Computed client-side: `(row.total_bytes / totalBytes) * 100` where `totalBytes = sum of all rows`.

## API Call

```typescript
// GET /api/stats/ports?minutes=5&limit=20&exporter_ip=...
```

## Time Window Options

5m, 15m, 1h (default 5m — port breakdown is most useful for recent traffic).

## Router Entry

```typescript
{ path: '/ports', name: 'PortBreakdown', component: () => import('./views/PortBreakdown.vue'), meta: { requiresAuth: true } }
```

## Acceptance Criteria

- Bar chart colors known vs unknown services differently
- `% Total` sums to 100% across all rows
- Auto-refresh every 30s
- Empty state handled
