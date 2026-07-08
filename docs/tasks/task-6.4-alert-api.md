# Task 6.4 — Alert Rules & Telegram Config API

## Goal

Expose REST endpoints to manage alert rules, configure Telegram, and query recent events.
All endpoints are admin-only.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/alert_api.rs` | CREATE |
| `collector-core/src/main.rs` | MODIFY — register routes |

---

## Routes

All routes require `require_admin` middleware.

```
GET    /api/alerts/rules          — list all rules
POST   /api/alerts/rules          — create rule
PUT    /api/alerts/rules/:id      — update rule (full replace of params)
DELETE /api/alerts/rules/:id      — delete rule
PATCH  /api/alerts/rules/:id/toggle — enable/disable

GET    /api/alerts/events         — recent events (paginated)
DELETE /api/alerts/events         — clear all events

GET    /api/alerts/telegram       — get telegram config (token masked)
PUT    /api/alerts/telegram       — update telegram config
POST   /api/alerts/telegram/test  — send a test message
```

---

## Request / Response shapes

### GET /api/alerts/rules
```json
[
  {
    "id": 1,
    "name": "BNG PPPoE sustained abuse",
    "exporter_id": 2,
    "rule_type": "sustained",
    "enabled": true,
    "params": {
      "short_window_min": 5,
      "long_window_min": 60,
      "multiplier": 4.0,
      "min_duration_min": 10,
      "min_long_avg_mbps": 1
    },
    "created_at": "2024-07-06T10:00:00Z"
  }
]
```

### POST /api/alerts/rules
```json
{
  "name": "Edge absolute cap",
  "exporter_id": null,
  "rule_type": "absolute",
  "enabled": true,
  "params": {
    "threshold_mbps": 800,
    "window_min": 5,
    "per_ip": false
  }
}
```
Response 201: created rule object  
Response 400: `{ "error": "unknown rule_type" }` or invalid params

**Validation per rule_type (enforce on create + update):**

| rule_type  | Required params keys                                              |
|------------|-------------------------------------------------------------------|
| sustained  | short_window_min, long_window_min, multiplier, min_duration_min   |
| absolute   | threshold_mbps, window_min                                        |
| night      | quiet_start_hour, quiet_end_hour, threshold_mbps                  |
| new_ip     | threshold_mbps, lookback_days                                     |

### PATCH /api/alerts/rules/:id/toggle
No body. Flips `enabled` boolean.  
Response 200: `{ "id": 1, "enabled": false }`

---

### GET /api/alerts/events
```
Query params:
  limit   (default 50, max 500)
  offset  (default 0)
  severity (info|warning|critical — optional filter)
  notified (true|false — optional filter)
```

```json
{
  "total": 142,
  "events": [
    {
      "id": 99,
      "rule_id": 1,
      "exporter_ip": "10.0.1.1",
      "src_ip": "192.168.50.33",
      "alert_type": "sustained",
      "severity": "critical",
      "message": "...",
      "bytes_short": 2812500000,
      "bytes_long": 75000000,
      "notified": true,
      "created_at": "2024-07-06T03:42:00Z"
    }
  ]
}
```

---

### GET /api/alerts/telegram
Token is masked: last 4 chars visible, rest replaced with `*`.
```json
{
  "bot_token": "***************abcd",
  "chat_id": "-100123456789",
  "enabled": true,
  "min_severity": "warning"
}
```

### PUT /api/alerts/telegram
```json
{
  "bot_token": "7123456789:AAF...",
  "chat_id": "-100123456789",
  "enabled": true,
  "min_severity": "warning"
}
```
All fields required. Empty `bot_token` is allowed (disables notifications even if `enabled: true`).  
Response 200: masked config object (same as GET)

### POST /api/alerts/telegram/test
Sends a test message with current config. Returns error if Telegram API fails.
```json
// 200
{ "ok": true, "message": "Test message sent successfully" }

// 400
{ "error": "Bot token not configured" }

// 502
{ "error": "Telegram API error: 400 Bad Request — chat not found" }
```

---

## `main.rs` route registration

```rust
let alert_routes = Router::new()
    .route("/api/alerts/rules", get(alert_api::list_rules).post(alert_api::create_rule))
    .route("/api/alerts/rules/:id", put(alert_api::update_rule).delete(alert_api::delete_rule))
    .route("/api/alerts/rules/:id/toggle", axum::routing::patch(alert_api::toggle_rule))
    .route("/api/alerts/events", get(alert_api::list_events).delete(alert_api::clear_events))
    .route("/api/alerts/telegram", get(alert_api::get_telegram).put(alert_api::update_telegram))
    .route("/api/alerts/telegram/test", post(alert_api::test_telegram))
    .layer(axum_middleware::from_fn(middleware::require_admin));
```

---

## Acceptance criteria

- [ ] `cargo build` passes
- [ ] CRUD for rules works; invalid rule_type returns 400
- [ ] Toggle flips enabled and returns new state
- [ ] Events endpoint returns paginated results with total count
- [ ] Telegram GET masks the bot token
- [ ] Telegram PUT persists all fields
- [ ] Telegram test returns 502 with Telegram's error message on API failure
- [ ] All routes return 401 without a valid admin JWT
