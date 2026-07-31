# Task 12.2 — NOC Dashboard Revamp (Frontend)

**Phase:** 12 (NOC Dashboard)
**Effort:** 1 day
**Files:** `frontend/src/views/Dashboard.vue`, `frontend/src/components/`, `frontend/src/stores/`

## Spec

Rebuild the main dashboard as a NOC wall view. Layout top-down:

### 1. Stat tile row (from `/api/stats/overview`, refresh 10s)

Eight tiles: **In atual** / **Out atual** (bps, with sparkline), **Pico 5 min**,
**95º percentil** (billing), **Flows/s**, **Talkers ativos**, **Exporters**
(`up/total`, red when any missing), **Alertas 24h** (red badge when active > 0).

Each tile: big number + label + small trend arrow vs previous window.

### 2. Main mirrored traffic chart

The Phase 11 mirrored chart (in above / out below), window selector
(1h / 6h / 24h / 7d), live-updating from the `/ws` stream for the rightmost edge.

### 3. Middle row — three panels

- **Protocolos**: donut TCP/UDP/ICMP/outros (existing endpoint)
- **Top talkers**: top 8 with horizontal bars + in/out split, click → TopTalkers view
- **Top ASNs**: top 8, same treatment, click → AsnTraffic view

### 4. Bottom row — operational state

- **Alertas recentes**: last 5 events with severity color, click → AlertEvents
- **Sessões BGP**: per-peer state chips (established = green), click → BgpDashboard
- **Saúde do coletor**: packets/s received, dropped %, export queue depth,
  ClickHouse insert errors — from `/metrics` values exposed via overview or a
  small `/api/system/collector` endpoint; degraded values highlighted

### 5. Heatmap hora × dia (7 days)

Traffic intensity heatmap (24 cols × 7 rows) below the fold — the "encher os
olhos" panel: instantly shows the network's daily rhythm and anomalous hours.
Data: 1-hour buckets from timeline endpoint with `hours=168`.

## Guidance

- Load the `dataviz` skill before building the charts.
- Keep every panel clickable through to its detail view.
- Auto-refresh must pause when the tab is hidden (`document.visibilityState`).
- Dark theme first — NOC walls run dark.

## Acceptance Criteria

- Dashboard renders all panels with live data and graceful empty states
- No panel blocks another's load (independent fetches, skeleton placeholders)
- `npm run build` passes with no type errors
