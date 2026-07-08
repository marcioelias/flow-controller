# Task 4.2 — ASN Traffic View (Frontend)

**Phase:** 4 (Dashboard Views)  
**Effort:** 1 hour  
**Depends on:** Task 2.2 (backend endpoint), Task 3.3 (ASN data flowing through)  
**Files:** `frontend/src/views/AsnTraffic.vue` (NEW), `frontend/src/router.ts`

## Spec

New route `/asn-traffic`. Shows traffic volume grouped by ASN.
Useful for detecting which autonomous systems are generating or receiving the most traffic.

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│  ASN Traffic          [Device ▼] [Last 1h ▼] [Both ▼]  [↺]  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  [Doughnut chart — top 8 ASNs + "Other" slice]              │
│                                                              │
├────────┬──────────────────┬─────────────┬───────────────────-┤
│  ASN   │  Label           │  Traffic    │  Packets           │
├────────┼──────────────────┼─────────────┼────────────────────┤
│  15169 │  AS15169         │  200 MB     │  150,000           │
│  0     │  Unknown         │    5 MB     │    3,000           │
└────────┴──────────────────┴─────────────┴────────────────────┘
```

## Controls

- **Device selector**: enabled exporters, same as Dashboard
- **Time window**: 15m, 1h, 6h, 24h (default 1h)
- **Direction**: Source, Destination, Both (maps to `direction=src|dst|both`)

## Doughnut Chart

Reuse the same `Doughnut` component from `vue-chartjs` already present in Dashboard.
Show top 8 ASNs; aggregate the rest as "Other".
Use the same color palette as the existing protocol doughnut (emerald, blue, amber, red, +4 more).

```typescript
const CHART_COLORS = [
  '#10b981', '#3b82f6', '#f59e0b', '#ef4444',
  '#8b5cf6', '#06b6d4', '#f97316', '#ec4899',
  '#6b7280', // "Other"
]
```

## API Call

```typescript
// GET /api/stats/asn?minutes=60&limit=20&direction=both&exporter_ip=...
```

## Router Entry

```typescript
{ path: '/asn-traffic', name: 'AsnTraffic', component: () => import('./views/AsnTraffic.vue'), meta: { requiresAuth: true } }
```

## Empty State

Show a card with text: "Sem dados de ASN disponíveis. Verifique se o equipamento está enviando informações de BGP no template NetFlow/IPFIX."

## Acceptance Criteria

- Doughnut chart renders with correct color slices
- Table shows all API rows sorted by traffic desc
- Direction selector changes API call
- Auto-refresh every 60s (ASN data changes slowly)
- Empty state rendered correctly
