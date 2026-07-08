# Task 5.2 — Docker Compose Production Hardening

**Phase:** 5 (Hardening)  
**Effort:** 30 minutes  
**Files:** `docker-compose.yml`, `Dockerfile`

## Issues to Fix

### 1. JWT secret in docker-compose.yml (requires task 1.1)

```yaml
# docker-compose.yml
flow-collector:
  environment:
    - CLICKHOUSE_URL=http://clickhouse:8123
    - JWT_SECRET=${JWT_SECRET:-insecure-default-change-me}
    - FLOW_RETENTION_DAYS=${FLOW_RETENTION_DAYS:-30}
```

Add a `.env.example` file (not `.env` — never commit secrets):
```
JWT_SECRET=replace-with-a-64-char-random-string
FLOW_RETENTION_DAYS=90
```

### 2. Remove `cap_add: NET_ADMIN`

The collector does not need `NET_ADMIN` capability. It only binds a UDP socket.
`NET_ADMIN` grants full network configuration access to the container — unnecessary and a security risk.

Remove from `docker-compose.yml`:
```yaml
# DELETE THESE LINES:
cap_add:
  - NET_ADMIN
```

To bind a privileged port (< 1024), you'd need `NET_BIND_SERVICE`. Port 2055 is unprivileged
(> 1024), so no capability is needed at all.

### 3. Add `restart: unless-stopped` to all services

```yaml
flow-collector:
  restart: unless-stopped

dashboard:
  restart: unless-stopped

clickhouse:
  restart: unless-stopped
```

### 4. Fix dashboard health dependency

Currently `dashboard: depends_on: clickhouse` without a health condition, meaning nginx
may start before ClickHouse is ready. Fix:

```yaml
dashboard:
  depends_on:
    flow-collector:
      condition: service_started
```

Dashboard only needs the collector (for API proxying). ClickHouse readiness is the collector's problem.

### 5. Add resource limits (optional but recommended)

```yaml
flow-collector:
  deploy:
    resources:
      limits:
        memory: 512M
      reservations:
        memory: 128M
```

### 6. Healthcheck for flow-collector

```yaml
flow-collector:
  healthcheck:
    test: ["CMD", "curl", "-f", "http://localhost:3000/metrics"]
    interval: 10s
    timeout: 5s
    retries: 3
    start_period: 15s
```

Requires task 3.2 (Prometheus endpoint) for the healthcheck target to exist.
Alternative until then: `["CMD-SHELL", "echo > /dev/tcp/localhost/3000 2>/dev/null && echo ok"]`

## Acceptance Criteria

- `docker compose up` works without `NET_ADMIN`
- All services restart automatically after crash
- No secrets committed to the repository
- `docker compose config` validates without errors
