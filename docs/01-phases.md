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

## Phase 9 — ML Anomaly Detection + LLM Explainability ✅
**Effort:** ~3 dias  |  **Files touched:** `collector-core/src/`, `flow-types/src/`, `frontend/src/`, `docker-compose.yml`

Detecção de anomalias por aprendizado de máquina (Isolation Forest, `linfa`) e explicações
em linguagem natural via LLM local (Ollama + qwen2.5:3b). O sistema **aprende o perfil de
tráfego normal de cada assinante** a partir dos flows coletados — sem supervisão e sem
labels manuais — e detecta automaticamente comportamentos anômalos (ataques, infecções,
exfiltração de dados).

| Task | Spec file | Status |
|------|-----------|--------|
| 9.1 Feature extractor (aggregated window → ML vector) | `tasks/task-9.1-feature-extractor.md` | ✅ |
| 9.2 Isolation Forest (model + training pipeline) | `tasks/task-9.2-isolation-forest.md` | ✅ |
| 9.3 LLM explainability (Ollama + qwen2.5:3b) | `tasks/task-9.3-llm-explainability.md` | ✅ |
| 9.4 AI Insights Dashboard (frontend) | `tasks/task-9.4-ai-dashboard.md` | ✅ |

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

## Phase 10 — Collector Performance & Robustness ✅
**Effort:** ~2 dias  |  **Files touched:** `collector-core/src/`, `netflow-parser/`, `template-cache/`, `aggregator/`, `clickhouse-exporter/`

Auditoria do caminho quente (recepção UDP → parse → agregação → ClickHouse). As fases
1–9 entregaram funcionalidade; esta fase trata throughput, perda silenciosa de dados e
robustez do parser sob templates inesperados.

| Task | Spec file | Status |
|------|-----------|--------|
| 10.1 Flush determinístico + timestamp real da janela | `tasks/task-10.1-window-flush.md` | ✅ |
| 10.2 Métricas de backpressure e precisão dos contadores | `tasks/task-10.2-backpressure-metrics.md` | ✅ |
| 10.3 Retry com backoff no insert ClickHouse | `tasks/task-10.3-clickhouse-retry.md` | ✅ |
| 10.4 Poda do cache de templates | `tasks/task-10.4-template-pruning.md` | ✅ |
| 10.5 Amostragem no produtor do Debug Console | `tasks/task-10.5-debug-sampling.md` | ✅ |
| 10.6 Limites de fila + backoff em erro de recv | `tasks/task-10.6-queue-bounds.md` | ✅ |
| 10.7 Múltiplos receptores UDP via SO_REUSEPORT | `tasks/task-10.7-reuseport-receivers.md` | ✅ |
| 10.8 Robustez do parser (varlen IE, read_uint, capacity) | `tasks/task-10.8-parser-robustness.md` | ✅ |
| 10.9 Extração de features fora do runtime async | `tasks/task-10.9-feature-extraction-offload.md` | ✅ |
| 10.10 Suporte a sampling interval (options templates) | `tasks/task-10.10-sampling-interval.md` | ✅ |
| 10.11 Merge de janelas antes do insert | `tasks/task-10.11-batch-merge.md` | ✅ |
| 10.12 Colunas IPv4/IPv6 nativas ⚠️ requer migração | `tasks/task-10.12-native-ip-columns.md` | ✅ |

### Severidade

| Grupo | Tasks | Sintoma se não fizer |
|-------|-------|----------------------|
| Perda silenciosa de dados | 10.1, 10.2, 10.3 | Janelas somem sem log; timestamps errados na timeline |
| Vazamento / OOM | 10.4, 10.6 | RSS cresce sem limite; overload vira OOM em vez de shed |
| Teto de throughput | 10.5, 10.7, 10.9, 10.11 | Um core satura a recepção; abrir o Debug Console degrada a coleta |
| Correção do parser | 10.8, 10.10 | Exporter com IE varlen não decodifica nada; exporter amostrado reporta 1/1000 do tráfego |
| Storage / query | 10.12 | 3 allocs por linha; IP como String custa storage e latência de query |

### Limitação conhecida — não coberta nesta fase

O sharding de workers é `hash(exporter_ip) % worker_count`, exigido pelo cache de
template thread-local. Com **um único exporter**, um worker faz todo o parsing e os
demais ficam ociosos — cenário comum em ISP com um roteador de borda. Resolver exige
shardar por `(exporter_ip, source_id)` ou compartilhar o cache atrás de um lock.
Fica registrado como dívida técnica; a task 10.7 alivia o lado da recepção, não o do
parsing.

---

## Phase 11 — Traffic Direction & Mirrored Charts ✅
**Effort:** ~1 dia  |  **Files touched:** `netflow-parser/`, `flow-types/`, `aggregator/`, `collector-core/src/`, `frontend/src/`

Hoje o sistema não diferencia inbound de outbound — tudo é somado. Exporters
configurados para exportar nos dois sentidos são **contados em dobro** sem aviso.
Decisão (2026-07-31): direção vem do roteador (IE 61 `flowDirection`, interfaces
IE 10/14), sem fallback por posse de IP/ASN nesta fase.

| Task | Spec file | Status |
|------|-----------|--------|
| 11.1 Direção do flow ponta a ponta (parser → ClickHouse) | `tasks/task-11.1-flow-direction.md` | ✅ |
| 11.2 Stats API com séries por direção (`sumIf`) | `tasks/task-11.2-direction-stats-api.md` | ✅ |
| 11.3 Gráficos espelhados (in acima, out abaixo) | `tasks/task-11.3-mirrored-charts.md` | ✅ |

---

## Phase 12 — NOC Dashboard ✅
**Effort:** ~1.5 dias  |  **Files touched:** `collector-core/src/stats.rs`, `frontend/src/`

Revamp do dashboard principal como painel de NOC: stat tiles (bps in/out, pico,
95º percentil, flows/s, talkers, exporters, alertas), gráfico espelhado principal,
donut de protocolos, top talkers/ASN, estado operacional (alertas, BGP, saúde do
coletor) e heatmap hora×dia.

| Task | Spec file | Status |
|------|-----------|--------|
| 12.1 NOC overview API (`GET /api/stats/overview`) | `tasks/task-12.1-noc-overview-api.md` | ✅ |
| 12.2 Dashboard NOC (frontend) | `tasks/task-12.2-noc-dashboard.md` | ✅ |

Depende da Fase 11 (séries por direção alimentam tiles e gráfico espelhado).

---

## Phase 13 — Fidelidade Temporal ✅
**Effort:** ~1 dia  |  **Files touched:** `netflow-parser/`, `flow-types/`, `aggregator/`, `collector-core/src/`, `frontend/src/`

Flow telemetry chega atrasada por natureza (active timeout do exporter). Hoje o
coletor carimba tudo na chegada: um upload de 20s vira um espigão de 1s, datado
errado. Esta fase usa os timestamps que o protocolo já envia para reconstruir o
tráfego como ele realmente aconteceu.

| Task | Spec file | Status |
|------|-----------|--------|
| 13.1 Decodificar flowStart/flowEnd (v9 + IPFIX) | `tasks/task-13.1-flow-timestamps.md` | ✅ |
| 13.2 Distribuição temporal dos bytes (time spreading) | `tasks/task-13.2-time-spread.md` | ✅ |
| 13.3 Indicador de latência de telemetria na UI | `tasks/task-13.3-latency-hint.md` | ✅ |
| 13.4 Gráfico ao vivo espelhado e sem suavização | `tasks/task-13.4-live-chart-mirror.md` | ✅ |
| 13.5 Dashboard em abas (Tempo real / Histórico / Servidor) | `tasks/task-13.5-dashboard-tabs.md` | ✅ |
| 13.6 Taxa real no gráfico ao vivo (retroativo) | `tasks/task-13.6-live-true-rate.md` | ✅ |
| 13.7 Banda por versão de IP (v4 × v6) | `tasks/task-13.7-ip-family-chart.md` | ✅ (superseded por 13.9) |
| 13.8 Timestamps IPFIX relativos (IE 21/22 + IE 160) | `tasks/task-13.8-ipfix-sysuptime.md` | ✅ |
| 13.9 Espelho empilhado por família + stats 95% | `tasks/task-13.9-stacked-family.md` | ✅ |
| 13.10 Janela ao vivo persistente entre views | `tasks/task-13.10-live-persistence.md` | ✅ |
| 13.11 Features do ML por segundo real | `tasks/task-13.11-ml-per-second-features.md` | ✅ |

Dependências: 13.1 → 13.2; 13.3, 13.4 e 13.5 independentes.

---

## Phase 14 — Backlog (decisões de produto pendentes)

| Task | Spec file | Status |
|------|-----------|--------|
| 14.1 Enforcement da licença free tier (banner + degradação 7d + cap de talkers) | `tasks/task-14.1-license-enforcement.md` | ✅ |

---

## Phase 15 — Feedback & Explicabilidade

| Task | Spec file | Status |
|------|-----------|--------|
| 15.1 Detalhe da anomalia + feedback do operador | `tasks/task-15.1-anomaly-detail.md` | ✅ |
| 15.2 Feedback → aprendizado do detector (threshold por IP) | `tasks/task-15.2-ml-feedback.md` | ✅ |
| 15.3 Idioma das explicações IA (APP_LANGUAGE) | `tasks/task-15.3-llm-language.md` | ✅ |
| 15.4 Reforma da tela de Configurações (abas + CRUD + IA guiada) | `tasks/task-15.4-settings-redesign.md` | ✅ |
| 15.5 Piso de significância para anomalias ML | `tasks/task-15.5-ml-significance-floor.md` | ✅ |

---

## Phase 16 — Observabilidade

| Task | Spec file | Status |
|------|-----------|--------|
| 16.1 Observabilidade fina no NOC (lag, perda, sparkline, top ASN) | `tasks/task-16.1-noc-observability.md` | ✅ |
| 16.2 Distribuição nativa Debian (.deb + systemd) — avaliação | `tasks/task-16.2-debian-packaging.md` | 🔲 |

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
    ├──► Phase 9 (ML) — requires Phase 6
    │         9.1 → 9.2 (hot path: features → model)
    │         9.2 → 9.3 (optional: LLM explain)
    │         9.2 + 9.3 → 9.4 (frontend)
    │
    ├──► Phase 10 (perf & robustness) — requires Phase 9
    │         10.1 → 10.11        (window_ts é pré-requisito do merge)
    │         10.2 → 10.3         (contadores antes do retry)
    │         10.4, 10.5, 10.6, 10.8, 10.9 — independentes
    │         10.7 — independente, mas medir depois de 10.6
    │         10.10 — independente, maior escopo
    │         10.12 — por último, requer migração
    │
    └──► Phase 11 (direção in/out) — requires Phase 10
              11.1 → 11.2 → 11.3
              (11.1 muda AggregationKey e schema; fazer após 10.12
               para aproveitar a mesma janela de migração)
```

## Agent Assignment

Each task file is self-contained: it lists the exact files to read, the exact changes to make,
and the acceptance criteria. An agent can pick up any single task file and implement it
without needing context from other tasks in the same phase (unless noted as a dependency).
