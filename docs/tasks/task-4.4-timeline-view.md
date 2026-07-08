# Task 4.4 — Traffic Timeline / History View (Frontend)

**Phase:** 4 (Dashboard Views)  
**Effort:** 1.5 hours  
**Depends on:** Task 2.4 (backend endpoint)  
**Files:** `frontend/src/views/TrafficHistory.vue` (NEW), `frontend/src/router.ts`

## Spec

New route `/history`. Shows a full-page traffic timeline chart with historical data
(up to 24 hours). Complements the live 5-minute line chart on the Dashboard.

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│  Traffic History              [Device ▼]  [Last 1h ▼]   [↺] │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  [Line chart — full width, 300px height]                    │
│   Y: Mbps    X: time (minute buckets)                       │
│                                                              │
├──────────────────┬───────────────┬──────────────────────────-┤
│  Metric          │  Value        │                            │
├──────────────────┼───────────────┤                            │
│  Peak            │  125.4 Mbps   │                            │
│  Average         │   42.1 Mbps   │                            │
│  Total           │    3.2 GB     │                            │
└──────────────────┴───────────────┘                            │
```

## Chart Details

- Chart.js `Line` chart (same component as Dashboard)
- X-axis: formatted time labels (use `toLocaleTimeString` for <24h, include date for 24h)
- Y-axis: Mbps (convert `total_bytes / 1e6 * 8 / 60`)
- No animation on mount (performance)
- `tension: 0.3` for smooth curves
- Fill: subtle gradient under the line (`backgroundColor: 'rgba(16,185,129,0.1)'`)
- Point radius: 0 (no dots — too many points)

## Summary Stats Panel

Computed from the API response array:
- **Peak**: `Math.max(...data.map(p => p.total_bytes))`
- **Average**: `sum / count`
- **Total**: `sum(total_bytes)` — formatted as `formatBytes()`

## Time Window Options

1h, 6h, 12h, 24h. Default 1h.

## API Call

```typescript
// GET /api/stats/timeline?hours=1&exporter_ip=...
```

The API returns minute buckets; for the 24h view that's up to 1440 points.
Chart.js handles this fine without downsampling.

## Router Entry

```typescript
{ path: '/history', name: 'TrafficHistory', component: () => import('./views/TrafficHistory.vue'), meta: { requiresAuth: true } }
```

## Auto-Refresh

Refresh every 60 seconds (historical data doesn't change rapidly).

## Acceptance Criteria

- Chart renders correctly for all time window options
- Summary stats are accurate (match the chart data)
- X-axis labels are human-readable
- Empty state when no data in the selected window
- Auto-refresh stops on `onUnmounted`
