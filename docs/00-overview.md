# FlowVision — Project Overview

**Produto:** Coletor e analisador de tráfego NetFlow/IPFIX  
**Desenvolvedor:** Hahn Tech Desenvolvimento e Consultoria Ltda  
**Versão atual:** 1.1.x (major.minor.build — build = git commit count)

---

## Goal

Plataforma comercial de monitoramento de rede para provedores de internet (ISPs) e operadoras:
- Recebe NetFlow v9 / IPFIX v10 via UDP
- Agrega e armazena flows no ClickHouse
- Expõe dashboard Vue 3 com visibilidade em tempo real: top talkers, ASN, aplicações, histórico
- Detecta anomalias (upload inversion, attack signature) e envia alertas via Telegram
- Aprende padrões de tráfego via Machine Learning (Isolation Forest) — sem supervisão, sem labels
- Explica alertas em linguagem natural via LLM local (Ollama + qwen2.5:3b, roda sem GPU)
- Anuncia prefixos BGP (blackhole, mitigação) via ExaBGP
- Licenciamento por fingerprint de máquina (Ed25519)

---

## Design Principles

1. **Performance first** — threads OS para parsing, zero-copy window rotation, bulk ClickHouse inserts LZ4
2. **Simple deployment** — Docker Compose com 4 serviços; instalador baremetal automatizado
3. **Operator-first UI** — toda configuração relevante exposta via dashboard; nenhum arquivo editado manualmente em produção (exceto `.env`)

---

## Architecture

```
                  ┌──────────────────────────────────────────────────┐
UDP :2055         │             collector-core binary                │
─────────────────►│                                                  │
                  │  [main thread] recv loop                         │
                  │    └─ whitelist check (DashSet em memória)       │
                  │    └─ hash(src_ip) % N → worker channel         │
                  │                                                  │
                  │  [worker 0..N]  parse → debug_tx → aggregate    │
                  │    └─ flush every 1s → export_tx channel        │
                  │                                                  │
                  │  [tokio] batch inserter                          │
                  │    └─ INSERT INTO network_flows_v4/v6           │
                  │    └─ broadcast LiveFlowStats via /ws            │
                  │                                                  │
                  │  [tokio] HTTP/WS server :3000                    │
                  │    ├─ REST API (auth, exporters, stats, BGP...)  │
                  │    ├─ /ws           — live traffic (bytes)       │
                  │    └─ /ws/debug    — flows individuais (debug)   │
                  │                                                  │
                  │  [tokio] anomaly detector (Fase 6)               │
                  │    └─ consulta ClickHouse MV hourly              │
                  │    └─ emite AlertEvent → Telegram + BGP          │
                  │                                                  │
                  │  [tokio] bgp session monitor (Fase 8)            │
                  │    └─ lê FIFO /run/exabgp/exabgp.out            │
                  └──────────────────────────────────────────────────┘
                            │                        │
                  ClickHouse :8123           /run/exabgp/ (volume)
                  network_flows_v4                   │
                  network_flows_v6          ┌────────┴───────────┐
                  flows_hourly_mv           │  ExaBGP container  │
                            │               │  network_mode: host │
                  nginx :8080               │  porta 179 (BGP)   │
                  Vue 3 SPA                 └────────────────────┘
```

---

## Repository Layout

```
flow-collector/
├── collector-core/      # Binário principal: UDP recv, HTTP/WS, API
│   └── src/
│       ├── main.rs        # Startup, worker threads, WS handlers
│       ├── auth.rs        # JWT + SQLite users + AppState
│       ├── exporters.rs   # Exporter whitelist CRUD
│       ├── middleware.rs  # Axum auth middleware
│       ├── stats.rs       # ClickHouse query handlers
│       ├── settings.rs    # Configurações globais (key/value SQLite)
│       ├── license.rs     # Ed25519 licensing + machine fingerprint
│       └── build.rs       # Embeds APP_VERSION, RUSTC_VERSION, CARGO_DEPS
├── netflow-parser/      # NetFlow v9 / IPFIX v10 parser
├── template-cache/      # Per-worker lock-free template store
├── aggregator/          # Janelas de 1s de agregação
├── flow-types/          # Structs compartilhados (NormalizedFlow)
├── metrics/             # Prometheus counters
├── clickhouse-exporter/ # ClickHouse bulk insert client (LZ4)
├── license-gen/         # CLI: keygen / issue / verify
├── frontend/            # Vue 3 + Pinia + Chart.js + Tailwind
│   └── src/
│       ├── layouts/AppLayout.vue   # Sidebar + topbar (botão Debug)
│       ├── stores/                 # auth, settings, bgp
│       └── views/                  # Dashboard, TopTalkers, About,
│                                   # Settings, License, DebugConsole...
├── docs/                # SDD completo
│   └── tasks/           # Spec por task (self-contained)
├── VERSION              # Fonte única de major.minor (ex: 1.1)
├── docker-compose.yml
├── install.sh           # Instalador baremetal
└── update.sh            # Atualizador com rollback
```

---

## Technology Stack

| Camada | Tecnologia |
|--------|-----------|
| Runtime | Tokio multi-thread |
| HTTP/WS | Axum 0.7 |
| Flow parsing | nom + byteorder (hand-written) |
| Armazenamento | ClickHouse 23.8 (MergeTree + SummingMergeTree MV) |
| Auth DB | SQLite via sqlx |
| BGP | ExaBGP 4.2.x (container host network, FIFO control) |
| **ML** | **linfa 0.7 (Isolation Forest, unsupervised anomaly detection)** |
| **LLM** | **Ollama + qwen2.5:3b Q4_K_M (CPU-only, ~2.3 GB RAM)** |
| Frontend | Vue 3 + Pinia + Chart.js + Tailwind CSS |
| Container | Docker Compose (4+1 serviços) |
| Métricas | Prometheus (`/metrics`) |
| Licenciamento | Ed25519 assimétrico (privkey com desenvolvedor) |

---

## Current State (2026-07)

### Implementado (Fases 1–5, parcial 7)

- UDP socket com dispatch por worker (hash src_ip)
- Parser NetFlow v9 completo + IPFIX v10 com enterprise IEs
- Thread-local template cache e agregador de janelas de 1s
- ClickHouse: `network_flows_v4/v6` com ASN, TTL configurável
- Bulk LZ4 insert via clickhouse-rs
- JWT auth (SQLite, bcrypt, 24h tokens, env JWT_SECRET)
- CRUD de exporters com whitelist em memória (DashSet, refresh 30s)
- WebSocket `/ws` push LiveFlowStats em tempo real
- WebSocket `/ws/debug` — flows individuais com filtro por src_ip
- Prometheus metrics (`/metrics`)
- Top Talkers, ASN Traffic, Port Breakdown, Traffic Timeline (endpoint + view)
- Settings globais (13 keys, 6 grupos) — persistidas no SQLite
- Licenciamento Ed25519: free tier (100 Mbps / 1 talker), license-gen CLI
- About page com versões de todos os componentes (Rust deps, npm deps, ClickHouse runtime)
- Flow Debug Console — modal com painel dividido lista/detalhe, filtro por IP
- Server Health dashboard — CPU por core, memória, disco, IOPS, uptime, métricas do processo
- Instalador baremetal (`install.sh`) + atualizador com rollback (`update.sh`)
- Docker Compose hardened (sem NET_ADMIN, healthcheck, restart policies)
- Structured logging JSON (LOG_FORMAT=json)
- Versionamento automático: `major.minor.build` onde build = git commit count

### A implementar

| Feature | Fase | Status |
|---------|------|--------|
| Server health dashboard (CPU, mem, disco) | 7.3 | ✅ |
| Backup & Restore | 7.4 | ✅ |
| Alerting (anomaly detector + Telegram) | 6 | ✅ |
| BGP announcement via ExaBGP | 8 | ✅ |
| Integração alerta → BGP blackhole automático | 6.8 | ✅ |
| ML anomaly detection (Isolation Forest) | 9.1 + 9.2 | 🔲 |
| LLM explainability (Ollama / qwen2.5:3b) | 9.3 | 🔲 |
| AI Insights dashboard | 9.4 | 🔲 |

---

## Docker Services

| Serviço | Imagem | Porta | Rede |
|---------|--------|-------|------|
| `clickhouse` | clickhouse/clickhouse-server:23.8 | 8123, 9000 | flow-net bridge |
| `flow-collector` | build local | 3000 (HTTP/WS), 2055/udp | flow-net bridge |
| `dashboard` | build local (nginx) | 8080 | flow-net bridge |
| `exabgp` *(Fase 8)* | pierky/exabgp:4.2.11 | 179 (BGP) | **host** |
| `ollama` *(Fase 9)* | ollama/ollama:latest | 11434 | flow-net bridge |

> ExaBGP usa `network_mode: host` — necessário para BGP TCP com IP real do host.
> Ollama é opcional — apenas necessário quando `LLM_ENABLED=true`.
