# Task 4.1 — Top-Talkers View (Frontend)

**Phase:** 4 (Dashboard Views)  
**Effort:** 1.5 hours  
**Depends on:** Task 2.1 (backend endpoint)  
**Files:** `frontend/src/views/TopTalkers.vue` (NEW), `frontend/src/stores/stats.ts` (NEW or extend), `frontend/src/router.ts`

## Spec

New route `/top-talkers`. Shows a ranked table of source IPs sorted by traffic volume,
with a bar chart of the top 10. Includes device selector and time window selector.

## Layout

```
┌─────────────────────────────────────────────────────────────┐
│  Top Talkers                    [Device ▼] [Last 5m ▼]  [↺] │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  [Horizontal bar chart — top 10 IPs by bytes]              │
│                                                             │
├──────┬──────────────────┬───────────┬──────────┬──────────-─┤
│  #   │  Source IP       │  Traffic  │  Packets │  Flows     │
├──────┼──────────────────┼───────────┼──────────┼────────────┤
│  1   │  192.168.1.100   │  100 MB   │  75,000  │  250       │
│  2   │  10.0.0.50       │   52 MB   │  40,000  │  130       │
│  …   │                  │           │          │            │
└──────┴──────────────────┴───────────┴──────────┴────────────┘
```

## Component Details

### Time window selector
Options: 5m, 15m, 1h, 6h, 24h. Stored in local `ref`, triggers API refetch.

### Device selector
Reuse the same pattern as `Dashboard.vue` — fetch `/api/exporters/enabled` on mount.

### Bar chart
Use Chart.js `HorizontalBar` (type: `'bar'`, `indexAxis: 'y'`).
- X-axis: bytes (format as `formatBytes()`)
- Y-axis: IP addresses (top 10 only)
- Color: single solid color (e.g., `#10b981` — emerald-500 matching existing palette)

### Table
- All rows from the API response (up to `limit=50`)
- `Traffic` column formatted as human-readable bytes: `formatBytes(bytes)`
- Auto-refresh every 30 seconds

### `formatBytes` helper

```typescript
// src/utils/format.ts (create this file)
export function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

export function formatNumber(n: number): string {
    return n.toLocaleString('pt-BR')
}
```

## API Call

```typescript
// GET /api/stats/top-talkers?minutes=5&limit=50&exporter_ip=...
const params = new URLSearchParams({ minutes: String(minutes.value), limit: '50' })
if (selectedExporter.value) params.set('exporter_ip', selectedExporter.value)
const res = await fetch(`/api/stats/top-talkers?${params}`, { headers: authStore.getAuthHeaders() })
```

## Router Entry

```typescript
// router.ts — add to routes array
{ path: '/top-talkers', name: 'TopTalkers', component: () => import('./views/TopTalkers.vue'), meta: { requiresAuth: true } }
```

## Nav Entry (task 4.5 handles this — reference only)

Add "Top Talkers" link to `AppLayout.vue` sidebar, visible to all authenticated users.

## Acceptance Criteria

- Page loads without errors
- Table shows IPs sorted by bytes desc
- Bar chart shows top 10 only
- Selecting a device filters results
- Changing time window refetches data
- Auto-refresh every 30s (stops on unmount via `onUnmounted` clearInterval)
- Loading state shown while fetching
- Empty state shown when no data
