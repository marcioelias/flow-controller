# Task 6.5 — Alert Dashboard (Frontend)

## Goal

Add an Alerting section to the Vue 3 dashboard with three views:
- **AlertEvents** — feed of recent anomaly events with type-specific detail columns
- **AlertRules** — CRUD for alert rules with dynamic parameter forms per rule type
- **TelegramSettings** — configure bot token, chat ID, severity filter, test button

Depends on task 6.4 (API endpoints).

---

## Files to create/modify

| File | Action |
|------|--------|
| `frontend/src/views/AlertEvents.vue` | CREATE |
| `frontend/src/views/AlertRules.vue` | CREATE |
| `frontend/src/views/TelegramSettings.vue` | CREATE |
| `frontend/src/stores/alerts.ts` | CREATE |
| `frontend/src/router.ts` | MODIFY |
| `frontend/src/App.vue` (sidebar) | MODIFY |

---

## AlertEvents view

Table columns adapt to event type. Common columns always shown:

| Time | Severity | Type | Exporter | IP | Message |

Type-specific extra columns (shown when at least one row has that type):

**upload_inversion:**
| Upload (Mbps) | Download (Mbps) | Ratio |

**attack_signature:**
| PPS | Avg pkt (bytes) | Ports |

Severity badge colours: `critical` → red, `warning` → yellow.

**Filters:** severity dropdown, rule type, notified status.  
**Pagination:** 50 per page, auto-refresh every 30 s.  
**"Clear all"** button with confirm dialog (admin only).

---

## AlertRules view

Table: name, type label, exporter, status toggle, edit, delete.

"+ New Rule" opens a slide-over with two steps:

**Step 1 — Rule type**
```
○ Upload Inversion
  Detects hosts that start uploading more than their historical download baseline.
  Typical signal: botnet participation, undeclared server, routing loop.

○ Attack Signature
  Detects high-PPS small-packet traffic toward known attack destination ports.
  Typical signal: SYN flood, UDP amplification, brute force.
```

**Step 2 — Parameters (dynamic by type)**

For `upload_inversion`:
```
Name:               [________________________]
Exporter:           [All exporters          ▼]
Short window:       [10]  min
History window:     [1440] min  (24 h)
Inversion ratio:    [2.0] ×   (current upload > current download × N)
Min hist DL ratio:  [3.0] ×   (skip IPs without strong download history)
Min upload:         [5]   Mbps (ignore near-idle IPs)
Cooldown:           [30]  min
```

For `attack_signature`:
```
Name:               [________________________]
Exporter:           [All exporters          ▼]
Eval window:        [2]   min
Min PPS:            [1000]
Max avg pkt size:   [200] bytes
Min total packets:  [10000]
Attack ports:       [80, 443, 53, 22, 123, 1900, 11211]
                    (empty = use built-in default list)
Cooldown:           [5]   min
```

---

## TelegramSettings view

```
┌────────────────────────────────────────────────────────────────┐
│  Telegram Notifications                                        │
├────────────────────────────────────────────────────────────────┤
│  Bot Token    [●●●●●●●●●●●●●●●abcd        ]  [Show / Hide]     │
│  Chat ID      [-100123456789           ]                        │
│  Enabled      [● ON  ]                                          │
│  Min severity [Warning                 ▼]                       │
│                                                                │
│  [Save]                         [Send test message]            │
│                                                                │
│  Last test: ✓ Sent 2 minutes ago                               │
└────────────────────────────────────────────────────────────────┘
```

- Show button reveals token for 30 s then re-masks automatically
- Test button shows inline result without page reload
- Save disables the form until the response returns

---

## Router additions

```ts
{ path: '/alerts/events',   component: () => import('./views/AlertEvents.vue'),
  meta: { requiresAuth: true } },
{ path: '/alerts/rules',    component: () => import('./views/AlertRules.vue'),
  meta: { requiresAdmin: true } },
{ path: '/alerts/telegram', component: () => import('./views/TelegramSettings.vue'),
  meta: { requiresAdmin: true } },
```

---

## Sidebar additions

Section "Alertas" after "Análise":
```
Alertas
  ├─ Eventos        →  /alerts/events   (badge: unread critical count)
  ├─ Regras         →  /alerts/rules    (admin only)
  └─ Telegram       →  /alerts/telegram (admin only)
```

---

## Acceptance criteria

- [ ] Event feed shows correct columns per alert type
- [ ] Severity badges correct colours
- [ ] Rule form shows different fields for each rule type
- [ ] Upload inversion form validates that `history_window_min >= short_window_min`
- [ ] Attack ports field accepts comma-separated integers; empty = default list
- [ ] Toggle enable/disable works without reopening the form
- [ ] TelegramSettings masks token; auto-hides after 30 s
- [ ] Test button shows inline success or error
- [ ] Admin-only links hidden from non-admin users
