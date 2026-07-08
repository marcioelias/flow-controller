# Flow Collector — Spec Documentation

This directory contains the complete Spec-Driven Development (SDD) documentation for the project.
Each task file is self-contained and can be handed to an independent agent to implement.

## Documents

| File | Description |
|------|-------------|
| [00-overview.md](00-overview.md) | Architecture, current state, what's done vs missing |
| [01-phases.md](01-phases.md) | Delivery phases, dependency graph, agent assignment strategy |
| [02-data-model.md](02-data-model.md) | ClickHouse schema, SQLite schema, query patterns |
| [03-api.md](03-api.md) | Full API reference (current + planned endpoints) |

## Tasks

### Phase 1 — Critical Fixes (do first, unblocks everything)

| Task | Title | Effort |
|------|-------|--------|
| [task-1.1](tasks/task-1.1-jwt-secret.md) | JWT secret via env var | 15m |
| [task-1.2](tasks/task-1.2-user-route-auth.md) | Apply auth middleware to user routes | 15m |
| [task-1.3](tasks/task-1.3-asn-schema.md) | Add ASN + src_port columns to ClickHouse | 30m |
| [task-1.4](tasks/task-1.4-ipv6-stats.md) | Fix IPv6 stats endpoint | 20m |
| [task-1.5](tasks/task-1.5-whitelist-cache.md) | Move whitelist check off hot path | 45m |

### Phase 2 — Analytics Endpoints (requires Phase 1)

| Task | Title | Effort |
|------|-------|--------|
| [task-2.1](tasks/task-2.1-top-talkers.md) | Top-talkers endpoint | 30m |
| [task-2.2](tasks/task-2.2-asn-traffic.md) | ASN traffic endpoint | 30m |
| [task-2.3](tasks/task-2.3-port-breakdown.md) | Port/application breakdown endpoint | 30m |
| [task-2.4](tasks/task-2.4-traffic-timeline.md) | Traffic timeline endpoint | 25m |
| [task-2.5](tasks/task-2.5-exporter-summary.md) | Per-exporter summary endpoint | 20m |

### Phase 3 — Protocol Improvements (parallel with Phase 2)

| Task | Title | Effort |
|------|-------|--------|
| [task-3.1](tasks/task-3.1-ipfix-parser.md) | IPFIX (v10) parser | 2-3h |
| [task-3.2](tasks/task-3.2-prometheus.md) | Wire Prometheus metrics endpoint | 45m |
| [task-3.3](tasks/task-3.3-asn-storage.md) | Store ASN through aggregation pipeline | 45m |

### Phase 4 — Dashboard Views (requires Phase 2)

| Task | Title | Effort |
|------|-------|--------|
| [task-4.1](tasks/task-4.1-top-talkers-view.md) | Top-talkers view | 1.5h |
| [task-4.2](tasks/task-4.2-asn-view.md) | ASN traffic view | 1h |
| [task-4.3](tasks/task-4.3-port-view.md) | Port/application breakdown view | 1h |
| [task-4.4](tasks/task-4.4-timeline-view.md) | Traffic history view | 1.5h |
| [task-4.5](tasks/task-4.5-nav.md) | Nav & routing updates | 30m |

### Phase 5 — Hardening (parallel with any phase)

| Task | Title | Effort |
|------|-------|--------|
| [task-5.1](tasks/task-5.1-retention.md) | ClickHouse TTL & retention (default 30d, `FLOW_RETENTION_DAYS`) | 20m |
| [task-5.2](tasks/task-5.2-docker-hardening.md) | Docker Compose hardening | 30m |
| [task-5.3](tasks/task-5.3-logging.md) | Structured JSON logging | 30m |

## How to Delegate to an Agent

Each task file contains:
1. Exact files to read before starting
2. Step-by-step implementation with code snippets
3. Acceptance criteria to verify completion

Hand an agent the task file path and the codebase root. The agent should:
1. Read the task file
2. Read the listed source files
3. Implement exactly what's described
4. Verify acceptance criteria via `cargo build` / `cargo test`

Tasks within the same phase are independent of each other and can run in parallel
(different agents, different git worktrees).

## IPv6 Coverage

All analytics endpoints and views cover both `network_flows_v4` and `network_flows_v6`.
Results are merged in Rust before returning to the client. See each task spec for the exact merge strategy.
