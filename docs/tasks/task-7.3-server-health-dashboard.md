# Task 7.3 — Server Health Dashboard

## Status: ✅ IMPLEMENTADO

## Objetivo

Adicionar uma view "Servidor" no dashboard que exibe métricas de saúde do host onde o
collector está rodando: CPU, memória, disco, IOPS, uptime e estatísticas do próprio
processo collector.

---

## Onde coletar as métricas

Todas as métricas são lidas do sistema de arquivos `/proc` e `/sys` — sem dependências
externas. O collector já roda em Linux (bare metal ou container com acesso ao host `/proc`).

| Métrica | Fonte |
|---------|-------|
| CPU % por core | `/proc/stat` (dois snapshots com 1s de intervalo) |
| Memória total/usada/disponível | `/proc/meminfo` |
| Uptime do sistema | `/proc/uptime` |
| Disco: total/usado/disponível | `statvfs("/")` via `libc` |
| IOPS: reads/writes por segundo | `/proc/diskstats` (dois snapshots com 1s de intervalo) |
| Processo: RSS, CPU%, threads | `/proc/self/status` + `/proc/self/stat` |

---

## Backend

### Novo endpoint

```
GET /api/system/health    — auth required (non-admin)
```

### Response JSON

```json
{
  "cpu": {
    "cores": 8,
    "usage_percent": [12.5, 8.2, 45.1, 6.0, 11.3, 9.8, 7.2, 14.0],
    "usage_total_percent": 14.3
  },
  "memory": {
    "total_bytes":     16777216000,
    "used_bytes":       8388608000,
    "available_bytes":  8388608000,
    "used_percent":     50.0
  },
  "disk": {
    "total_bytes":    107374182400,
    "used_bytes":      32212254720,
    "available_bytes": 75161927680,
    "used_percent":    30.0,
    "reads_per_sec":   42.0,
    "writes_per_sec":  18.0
  },
  "system": {
    "uptime_seconds": 864000,
    "uptime_human":   "10 dias, 0h 0m"
  },
  "process": {
    "rss_bytes":       52428800,
    "cpu_percent":     0.8,
    "threads":         12,
    "uptime_seconds":  7200
  },
  "collector": {
    "flows_received":   1234567,
    "flows_decoded":    1230000,
    "packets_dropped":  4567,
    "template_cache_size": 42
  }
}
```

### Implementação

Novo arquivo `collector-core/src/system_health.rs`.

A leitura de CPU e IOPS requer dois snapshots espaçados de 500ms (dormir 500ms dentro
do handler é aceitável — é uma rota de uso humano, não de polling frequente).

```rust
pub async fn get_health_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SystemHealth>, StatusCode>
```

Registrar em `user_routes` (auth, não admin):
```rust
.route("/api/system/health", get(system_health::get_health_handler))
```

---

## Frontend

### Nova view: `frontend/src/views/ServerHealth.vue`

Layout em grid 2×2 de cards, mais um card de largura total no topo para CPU.

```
┌─────────────────────────────────────────────────────────────────┐
│  CPU  14.3% total                                               │
│  ████░░░░  Core 0: 12.5%   ████░░░░  Core 1:  8.2%            │
│  ████████  Core 2: 45.1%   █░░░░░░░  Core 3:  6.0%            │
│  ... (todos os cores)                                           │
└─────────────────────────────────────────────────────────────────┘

┌───────────────────────┐  ┌───────────────────────┐
│  Memória              │  │  Disco (/)             │
│  ████████░░  50%      │  │  ███░░░░░░░  30%       │
│  Usada:   8.0 GB      │  │  Usado:  30.0 GB       │
│  Total:  16.0 GB      │  │  Total: 100.0 GB       │
│  Livre:   8.0 GB      │  │  Leituras: 42/s        │
│                       │  │  Escritas: 18/s        │
└───────────────────────┘  └───────────────────────┘

┌───────────────────────┐  ┌───────────────────────┐
│  Sistema              │  │  Processo Collector    │
│  Uptime: 10d 0h 0m    │  │  CPU:    0.8%          │
│  Cores:  8            │  │  Memória: 50 MB        │
│                       │  │  Threads: 12           │
│                       │  │  Uptime:  2h 0m        │
└───────────────────────┘  └───────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Flow Collector — Métricas                                      │
│  Recebidos: 1.234.567   Decodificados: 1.230.000                │
│  Descartados: 4.567     Templates cache: 42                     │
└─────────────────────────────────────────────────────────────────┘
```

**Auto-refresh**: a cada 5 segundos (não precisa de WebSocket — polling é suficiente).

Barras de progresso coloridas:
- 0–60%: emerald
- 60–80%: yellow/amber
- 80–100%: red

### Router

```ts
{ path: '/server', name: 'ServerHealth',
  component: () => import('./views/ServerHealth.vue'),
  meta: { requiresAuth: true } }
```

### Sidebar

Adicionar antes da seção "Análise":

```html
<router-link to="/server" ...>
  <Server class="w-5 h-5" />
  Servidor
</router-link>
```

---

## Notas de implementação

- Em container sem acesso ao `/proc` do host, algumas métricas vão retornar zeros —
  o endpoint nunca deve retornar 500, apenas omitir ou zerar campos indisponíveis.
- `disk.reads_per_sec` / `writes_per_sec`: buscar o device correto em `/proc/diskstats`
  é o maior ponto de atenção. Usar o device de `/proc/mounts` que contém `/` como base.
- IOPS em containers (overlayfs) pode não refletir disco físico — documentar isso na UI
  com um tooltip "Pode não refletir disco físico em ambientes containerizados".

---

## Acceptance criteria

- [ ] `cargo build` passa
- [ ] GET `/api/system/health` retorna 200 com todos os campos
- [ ] Em container onde `/proc/diskstats` não tem o device esperado, retorna 0 sem panic
- [ ] Frontend exibe barras de progresso com cores corretas por threshold
- [ ] Auto-refresh a cada 5s sem memory leak (limpar `setInterval` no `onUnmounted`)
- [ ] Link "Servidor" aparece na sidebar para todos os usuários autenticados
