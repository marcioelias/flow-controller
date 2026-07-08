# Phases & Delivery Order

Each phase is independently delegatable. Phases 1–3 are backend-only; Phase 4 is frontend-only.
Phase 5 is ops/hardening and can run in parallel with Phase 4.

---

## Phase 1 — Core Fixes & Critical Gaps ✅
**Effort:** ~1 day  |  **Files touched:** `collector-core/src/`, `clickhouse-exporter/src/`

| Task | Spec file | Status |
|------|-----------|--------|
| 1.1 Secure JWT secret via env var | `tasks/task-1.1-jwt-secret.md` | ✅ |
| 1.2 Apply `require_auth` to user routes | `tasks/task-1.2-user-route-auth.md` | ✅ |
| 1.3 Add ASN columns to ClickHouse schema | `tasks/task-1.3-asn-schema.md` | ✅ |
| 1.4 Fix IPv6 stats endpoint to query v6 table | `tasks/task-1.4-ipv6-stats.md` | ✅ |
| 1.5 Move exporter whitelist check off hot path | `tasks/task-1.5-whitelist-cache.md` | ✅ |

---

## Phase 2 — Analytics Queries (Backend) ✅
**Effort:** ~1 day  |  **Files touched:** `collector-core/src/stats.rs`

| Task | Spec file | Status |
|------|-----------|--------|
| 2.1 Top-talkers endpoint | `tasks/task-2.1-top-talkers.md` | ✅ |
| 2.2 ASN traffic endpoint | `tasks/task-2.2-asn-traffic.md` | ✅ |
| 2.3 Port/application breakdown endpoint | `tasks/task-2.3-port-breakdown.md` | ✅ |
| 2.4 Traffic timeline endpoint (historical) | `tasks/task-2.4-traffic-timeline.md` | ✅ |
| 2.5 Per-exporter summary endpoint | `tasks/task-2.5-exporter-summary.md` | ✅ |

---

## Phase 3 — Protocol Improvements (Backend) ✅
**Effort:** ~1.5 days  |  **Files touched:** `netflow-parser/`, `flow-types/`, `clickhouse-exporter/`

| Task | Spec file | Status |
|------|-----------|--------|
| 3.1 IPFIX (v10) parser | `tasks/task-3.1-ipfix-parser.md` | ✅ |
| 3.2 Wire Prometheus metrics endpoint | `tasks/task-3.2-prometheus.md` | ✅ |
| 3.3 Store src_asn / dst_asn from parsed flows | `tasks/task-3.3-asn-storage.md` | ✅ |

---

## Phase 4 — Dashboard Views (Frontend) ✅
**Effort:** ~1.5 days  |  **Files touched:** `frontend/src/views/`, `frontend/src/stores/`

| Task | Spec file | Status |
|------|-----------|--------|
| 4.1 Top-talkers view | `tasks/task-4.1-top-talkers-view.md` | ✅ |
| 4.2 ASN traffic view | `tasks/task-4.2-asn-view.md` | ✅ |
| 4.3 Protocol/port breakdown view | `tasks/task-4.3-port-view.md` | ✅ |
| 4.4 Traffic timeline / history view | `tasks/task-4.4-timeline-view.md` | ✅ |
| 4.5 Nav & routing updates | `tasks/task-4.5-nav.md` | ✅ |

---

## Phase 5 — Hardening & Ops ✅
**Effort:** ~0.5 day  |  Can run in parallel with Phase 4

| Task | Spec file | Status |
|------|-----------|--------|
| 5.1 ClickHouse TTL & retention policy | `tasks/task-5.1-retention.md` | ✅ |
| 5.2 Docker Compose production hardening | `tasks/task-5.2-docker-hardening.md` | ✅ |
| 5.3 Structured logging (tracing + JSON) | `tasks/task-5.3-logging.md` | ✅ |

---

---

## Phase 6 — Alerting & Telegram Notifications ✅
**Effort:** ~2 days  |  **Files touched:** `collector-core/src/`, `frontend/src/`

Anomaly detection engine with configurable rules and Telegram push notifications.
Phases 1–5 must be complete. Phase 6 tasks run sequentially (each builds on the last).

| Task | Spec file | Status |
|------|-----------|--------|
| 6.1 Alert infrastructure (SQLite schema + Rust types) | `tasks/task-6.1-alert-infrastructure.md` | ✅ |
| 6.2 Anomaly detector engine (background task) | `tasks/task-6.2-anomaly-detector.md` | ✅ |
| 6.3 Telegram notifier (background task) | `tasks/task-6.3-telegram-notifier.md` | ✅ |
| 6.4 Alert rules & Telegram config API | `tasks/task-6.4-alert-api.md` | ✅ |
| 6.5 Alert dashboard views (Frontend) | `tasks/task-6.5-alert-dashboard.md` | ✅ |

### Rule types

| Type | Best for | Signal |
|------|----------|--------|
| `upload_inversion` | BNG/PPPoE | Upload > download × N, confirmed by 24h history |
| `attack_signature` | Any exporter | High PPS + small packets + known attack ports |

### Why volume-based thresholds are wrong for BNG

A PPPoE subscriber can burst to 1 Gbps for 2 minutes during a large download — that's
normal. 500 Mbps at 3 AM from a subscriber who normally sleeps at that hour is suspicious
on its surface, but could still be a legitimate overnight download.

The signals that actually matter are behavioral:

- **Upload inversion**: a residential subscriber's traffic is asymmetric by design.
  When upload starts exceeding download (sustained), the host is likely compromised
  (botnet, DDoS participant, spam relay). Volume is irrelevant — the direction inversion
  is the anomaly.

- **Attack signature**: high PPS + small average packet size + destination to known
  attack ports (NTP, SSDP, DNS, HTTP, SSH, etc.) is a near-zero-false-positive signal
  that the host is actively sending attack traffic, regardless of total bytes.

### Performance at 25k subscribers

Detection uses ClickHouse Materialized Views (`SummingMergeTree`, hourly buckets) for
the 24h history window. At 25k subscribers:
- MV adds ~360 MB/month (< 0.1% of raw table size)
- Each 60s eval cycle scans ~3.6M rows total, completes in ~30ms
- MVs carry the same TTL as the raw tables (controlled by `FLOW_RETENTION_DAYS`)

---

## Phase 7 — Licensing, Health & Ops
**Effort:** ~2 days  |  **Files touched:** `collector-core/src/`, `license-gen/`, `frontend/src/`

| Task | Spec file | Status |
|------|-----------|--------|
| 7.1 Software licensing (Ed25519, license-gen CLI, UI) | `tasks/task-7.1-licensing.md` | ✅ Implementado |
| 7.2 Bug fix: usuário admin não abre para edição | `tasks/task-7.2-bug-user-edit.md` | ✅ Corrigido |
| 7.3 Server health dashboard (CPU, mem, disco, IOPS) | `tasks/task-7.3-server-health-dashboard.md` | ✅ Implementado |
| 7.4 Backup & Restore (config + flows opcionais) | `tasks/task-7.4-backup-restore.md` | ✅ Implementado |
| 7.5 Configurações do sistema (Settings) | `tasks/task-7.5-settings.md` | ✅ Implementado |
| 7.6 About / versionamento de componentes | `tasks/task-7.6-about-versioning.md` | ✅ Implementado |
| 7.7 Flow Debug Console (WebSocket + modal) | `tasks/task-7.7-flow-debug-console.md` | ✅ Implementado |

### Settings implementadas (task 7.5)

| Key | Grupo | Padrão | Efeito |
|-----|-------|--------|--------|
| `OWN_ASN_LIST` | Rede | — | ASNs próprios, usado para classificar direção de tráfego e detecção de anomalias |
| `INTERNAL_PREFIXES` | Rede | — | CIDRs próprios, complementa ASN para redes sem BGP |
| `ORG_NAME` | Organização | — | Nome exibido no dashboard e alertas |
| `TIMEZONE` | Organização | `America/Sao_Paulo` | Fuso para exibição de datas |
| `FLOW_RETENTION_DAYS` | Análise | `30` | TTL do ClickHouse (requer reinício) |
| `MAX_TOP_TALKERS` | Análise | `50` | Limite de resultados nas views analíticas |
| `DEFAULT_WINDOW_MIN` | Análise | `60` | Janela pré-selecionada nas telas de análise |
| `ALERT_COOLDOWN_MIN` | Alertas | `60` | Cooldown entre alertas do mesmo IP |
| `UPLOAD_INVERSION_FACTOR` | Alertas | `2` | Upload > download × fator → anomalia |
| `ATTACK_PPS_THRESHOLD` | Alertas | `10000` | PPS acima disso aciona regra de ataque |
| `ATTACK_PKT_SIZE_MAX` | Alertas | `200` | Pacotes menores que isso + alto PPS = ataque |
| `SESSION_TIMEOUT_HOURS` | Sessão | `24` | Duração do token JWT |
| `COLLECTOR_WORKERS` | Coletor | `4` | Threads de parsing/agregação (lido no startup, clamp 1–64) |

---

---

## Phase 8 — BGP Announcement via ExaBGP
**Effort:** ~2.5 dias  |  **Files touched:** `collector-core/src/`, `frontend/src/`, `docker-compose.yml`

Integração com ExaBGP para envio de updates BGP (blackhole, mitigação) controlados via UI.
**Objetivo:** enviar anúncios BGP, não participar do plano de roteamento.

| Task | Spec file | Status |
|------|-----------|--------|
| 8.1 BGP schema + tipos Rust | `tasks/task-8.1-bgp-schema.md` | ✅ |
| 8.2 ExaBGP no Docker + geração de config | `tasks/task-8.2-exabgp-docker.md` | ✅ |
| 8.3 Bridge de controle (announce/withdraw) | `tasks/task-8.3-route-control.md` | ✅ |
| 8.4 Monitor de sessão (background task) | `tasks/task-8.4-session-monitor.md` | ✅ |
| 8.5 REST API BGP (CRUD + apply + sessions) | `tasks/task-8.5-bgp-api.md` | ✅ |
| 8.6 Frontend BGP (peers, communities, prefixes, sessions) | `tasks/task-8.6-bgp-frontend.md` | ✅ |

### Decisões de arquitetura (resumo)
- ExaBGP é um **anunciador** — não faz roteamento, apenas envia BGP UPDATEs
- Controle via **named pipe** `/run/exabgp.in` (volume compartilhado collector ↔ exabgp)
- Mudanças de peer (config) → gera `exabgp.conf` → sinaliza reload via Docker socket (`SIGUSR1`)
- Mudanças de rota (announce/withdraw) → escreve diretamente no named pipe (sem reload)
- ExaBGP **requer `network_mode: host`** — BGP usa porta 179 e o peer espera o IP real do host
- Configurações armazenadas no SQLite (`auth.db`) como fonte de verdade

---

## Phase 9 — ML Anomaly Detection + LLM Explainability 🔲
**Effort:** ~3 dias  |  **Files touched:** `collector-core/src/`, `flow-types/src/`, `frontend/src/`, `docker-compose.yml`

Detecção de anomalias por aprendizado de máquina (Isolation Forest, `linfa`) e explicações
em linguagem natural via LLM local (Ollama + qwen2.5:3b). O sistema **aprende o perfil de
tráfego normal de cada assinante** a partir dos flows coletados — sem supervisão e sem
labels manuais — e detecta automaticamente comportamentos anômalos (ataques, infecções,
exfiltração de dados).

| Task | Spec file | Status |
|------|-----------|--------|
| 9.1 Feature extractor (aggregated window → ML vector) | `tasks/task-9.1-feature-extractor.md` | 🔲 |
| 9.2 Isolation Forest (model + training pipeline) | `tasks/task-9.2-isolation-forest.md` | 🔲 |
| 9.3 LLM explainability (Ollama + qwen2.5:3b) | `tasks/task-9.3-llm-explainability.md` | 🔲 |
| 9.4 AI Insights Dashboard (frontend) | `tasks/task-9.4-ai-dashboard.md` | 🔲 |

### Como o sistema aprende

O Isolation Forest é um algoritmo de **detecção de anomalias não-supervisionado**: ele
aprende o que é "normal" a partir de exemplos normais, sem precisar de dados rotulados.

**Feature vector por IP por janela de 1s (9 dimensões):**

| Feature | Captura |
|---------|---------|
| `pps` (log) | ritmo de transmissão |
| `bps` (log) | volume de dados |
| `avg_pkt_bytes / 1500` | tamanho médio de pacote (normalizado) |
| `unique_dst_ips` (log) | diversidade de alvos |
| `unique_dst_ports` (log) | diversidade de portas destino |
| `upload_ratio` | proporção upload/download |
| `tcp_ratio` | fração de bytes TCP |
| `udp_ratio` | fração de bytes UDP |
| `icmp_ratio` | fração de bytes ICMP |

O modelo detecta automaticamente:
- **DDoS outbound**: pps alto + pacotes pequenos + muitas portas
- **Port scan**: unique_dst_ips alto + unique_dst_ports alto
- **Botnet C&C**: upload_ratio invertido vs histórico
- **Data exfiltration**: bps alto sustentado com poucos destinos únicos
- **Amplification participation**: udp_ratio alto + pps extremo

### Lifecycle do modelo

```
Warm-up (24h)        → coleta features, sem alertas
Primeiro treino       → 5.000 amostras mínimas coletadas
Pontuação contínua   → cada janela recebe um score [0,1]
Re-treino periódico  → a cada 6h, reconstruído do ring buffer (50k amostras)
```

### Requisitos de hardware

| Componente | RAM | CPU |
|------------|-----|-----|
| ClickHouse | ~4 GB | variável |
| flow-collector | ~512 MB | 2 cores |
| Ollama + qwen2.5:3b Q4_K_M | ~2.3 GB | 4 cores |
| **Total** | **~7 GB** | **~6 cores** |

Funciona em uma **VM de 8GB / 8 vCPUs**. Sem GPU necessária.
Ollama é opcional — sem `LLM_ENABLED=true` o serviço não é iniciado.

### Dependências

- Phase 6 completa (usa `alert_events` e `alerts::insert_event`)
- `linfa = "0.7"`, `linfa-trees = "0.7"`, `flume = "0.11"`

---

## Dependency Graph

```
Phase 1 (fixes)
    │
    ├──► Phase 2 (analytics endpoints)
    │         │
    │         └──► Phase 4 (dashboard views)
    │
    ├──► Phase 3 (protocol improvements)
    │
    ├──► Phase 5 (hardening) — parallel to all
    │
    ├──► Phase 6 (alerting)
    │         6.1 → 6.2 → 6.3 → 6.4 → 6.5
    │                │
    │                └──► task 6.8 (anomaly → BGP auto-announce)
    │                          │
    │                          └── depende de Phase 8 task 8.3
    │
    ├──► Phase 7 (licensing & ops) — independent
    │         7.1 ✅  7.2 ✅  7.3 → 7.4  7.5 ✅  7.6 ✅  7.7 ✅
    │
    ├──► Phase 8 (BGP) — independent, requires Phase 5 (Docker)
    │         8.1 → 8.2 → 8.3 → 8.4 → 8.5 → 8.6
    │
    └──► Phase 9 (ML) — requires Phase 6
              9.1 → 9.2 (hot path: features → model)
              9.2 → 9.3 (optional: LLM explain)
              9.2 + 9.3 → 9.4 (frontend)
```

## Agent Assignment

Each task file is self-contained: it lists the exact files to read, the exact changes to make,
and the acceptance criteria. An agent can pick up any single task file and implement it
without needing context from other tasks in the same phase (unless noted as a dependency).
