# Task 4.5 — Navigation & Routing Updates

**Phase:** 4 (Dashboard Views)  
**Effort:** 30 minutes  
**Files:** `frontend/src/layouts/AppLayout.vue`, `frontend/src/router.ts`

## Spec

Update sidebar navigation to include all new views. Group nav items logically.

## Updated Sidebar Structure

```
Navigation
──────────────────────────────
📊 Dashboard          (always visible, auth)
📈 Traffic History    (always visible, auth)    ← NEW
──────────────────────────────
Análise
──────────────────────────────
🏆 Top Talkers        (always visible, auth)    ← NEW
🌐 ASN Traffic        (always visible, auth)    ← NEW
🔌 Applications       (always visible, auth)    ← NEW
──────────────────────────────
Administração
──────────────────────────────
📡 Exporters          (admin only)
👤 Usuários           (admin only)
```

## Implementation

### AppLayout.vue changes

The current sidebar has two items (Dashboard, Exporters, Usuários). Extend to:

```html
<!-- Main section -->
<nav-item to="/dashboard"  icon="LayoutDashboard" label="Dashboard" />
<nav-item to="/history"    icon="TrendingUp"       label="Histórico" />

<!-- Analysis section header -->
<div class="px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider">Análise</div>
<nav-item to="/top-talkers" icon="Users"           label="Top Talkers" />
<nav-item to="/asn-traffic" icon="Globe"           label="ASN Traffic" />
<nav-item to="/ports"       icon="Plug"            label="Aplicações" />

<!-- Admin section header (v-if="authStore.isAdmin") -->
<div v-if="authStore.isAdmin" class="px-3 py-2 text-xs font-semibold text-gray-500 uppercase tracking-wider mt-4">Administração</div>
<nav-item v-if="authStore.isAdmin" to="/exporters" icon="Router"  label="Exporters" />
<nav-item v-if="authStore.isAdmin" to="/users"     icon="UserCog" label="Usuários" />
```

All Lucide icon names used are already imported in the existing `AppLayout.vue`.
Check the existing import and add `TrendingUp`, `Users`, `Globe`, `Plug`, `Router`, `UserCog`
if not already present.

### Active link styling

The existing `router-link-active` class already applies highlighting via `exact-active-class`.
No changes needed.

### router.ts changes

The four new routes (TopTalkers, AsnTraffic, PortBreakdown, TrafficHistory) should be added here.
Each uses lazy loading: `component: () => import('./views/Xxx.vue')`.

All require `meta: { requiresAuth: true }`. None require admin.

The existing navigation guard already handles `requiresAuth` — no guard changes needed.

## Acceptance Criteria

- All five nav items visible to authenticated non-admin users
- Admin section hidden from non-admin users
- Active route highlighted
- Clicking each nav item loads the correct view
- Router guard still works (unauthenticated → /login)
