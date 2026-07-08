# Task 9.4 — AI Insights Dashboard (frontend)

## Goal

Painel dedicado às detecções do sistema de IA — Isolation Forest + LLM explainability.
Exibe o status do modelo por exporter (warm-up vs ativo), estatísticas de detecção,
histórico de anomalias ML e as explicações geradas.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/ml_api.rs` | CREATE — API endpoints para status e métricas do modelo |
| `collector-core/src/main.rs` | MODIFY — register ml_api routes |
| `frontend/src/views/AiInsights.vue` | CREATE — painel principal |
| `frontend/src/stores/ai.ts` | CREATE — Pinia store para dados de IA |
| `frontend/src/layouts/AppLayout.vue` | MODIFY — add "IA" link na sidebar |
| `frontend/src/router.ts` | MODIFY — add `/ai` route |

---

## API endpoints

```
GET  /api/ml/status       → status dos modelos por exporter
GET  /api/ml/events       → anomalias ml_anomaly paginadas (reutiliza alert_events)
GET  /api/ml/stats        → métricas agregadas
```

### `GET /api/ml/status` response

```json
{
  "llm_enabled": true,
  "llm_model": "qwen2.5:3b",
  "exporters": [
    {
      "exporter_ip": "10.0.0.1",
      "status": "active",       // "warming_up" | "active" | "no_data"
      "samples_collected": 12500,
      "samples_needed": 5000,
      "n_scored": 8200,
      "anomalies_detected": 3,
      "last_retrain": "2024-01-15T14:30:00Z"
    }
  ]
}
```

### `GET /api/ml/stats` response

```json
{
  "total_ml_anomalies": 7,
  "anomalies_last_24h": 2,
  "top_offenders": [
    { "src_ip": "192.168.1.55", "count": 3 }
  ],
  "severity_breakdown": { "warning": 5, "critical": 2 }
}
```

---

## `ml_api.rs`

The model state is shared via `Arc<RwLock<HashMap<String, ExporterModel>>>`.
Wrap it in `AppState` or pass directly as Axum extension.

```rust
pub struct MlStatus {
    pub llm_enabled: bool,
    pub llm_model: String,
    pub exporters: Vec<ExporterStatus>,
}

pub struct ExporterStatus {
    pub exporter_ip: String,
    pub status: String,  // "warming_up" | "active" | "no_data"
    pub samples_collected: usize,
    pub samples_needed: usize,
    pub n_scored: u64,
    pub anomalies_detected: u32,
    pub last_retrain: Option<String>,
}
```

`anomalies_detected` and `last_retrain` are queried from SQLite:
```sql
SELECT COUNT(*) FROM alert_events WHERE alert_type = 'ml_anomaly' AND exporter_ip = ?
```

---

## `AiInsights.vue` — layout

```
┌─────────────────────────────────────────────────────┐
│  🤖  IA Insights                     [Refresh]      │
├─────────────────────────────────────────────────────┤
│  LLM: qwen2.5:3b ● ativo  │  7 anomalias detectadas  │
│                           │  2 nas últimas 24h        │
├───────────────────────────┴───────────────────────────┤
│  Status por Exporter                                  │
│  ┌─────────────┬──────────┬──────────┬─────────────┐  │
│  │ Exporter IP │ Status   │ Amostras │ Anomalias   │  │
│  ├─────────────┼──────────┼──────────┼─────────────┤  │
│  │ 10.0.0.1   │ ● Ativo  │ 12.5k    │ 3           │  │
│  │ 10.0.0.2   │ ⏳ Warm  │ 2.1k/5k  │ —           │  │
│  └─────────────┴──────────┴──────────┴─────────────┘  │
├───────────────────────────────────────────────────────┤
│  Anomalias ML Recentes                                │
│  ┌────────┬──────────┬────────┬──────────────────────┐ │
│  │ Hora   │ IP       │ Score  │ Explicação IA         │ │
│  ├────────┼──────────┼────────┼──────────────────────┤ │
│  │ 14:32  │ 10.1.1.5 │ 0.82 ● │ "Padrão consistente  │ │
│  │        │          │        │  com scan de portas   │ │
│  │        │          │        │  UDP em larga escala."│ │
│  └────────┴──────────┴────────┴──────────────────────┘ │
└───────────────────────────────────────────────────────┘
```

### Status badge colors

| Status | Color | Description |
|--------|-------|-------------|
| `active` | emerald | modelo treinado, pontuando em tempo real |
| `warming_up` | amber | coletando amostras para treino inicial |
| `no_data` | zinc | exporter sem flows recentes |

### Score visualization

Score exibido como badge colorido:
- `>= 0.80` → vermelho (critical)
- `0.65–0.79` → âmbar (warning)
- `< 0.65` → zinc (não atingiu threshold — mostrado apenas em histórico)

---

## `ai.ts` Pinia store

```typescript
export interface ExporterModelStatus {
  exporter_ip: string
  status: 'warming_up' | 'active' | 'no_data'
  samples_collected: number
  samples_needed: number
  n_scored: number
  anomalies_detected: number
  last_retrain: string | null
}

export interface MlStatus {
  llm_enabled: boolean
  llm_model: string
  exporters: ExporterModelStatus[]
}

export interface MlStats {
  total_ml_anomalies: number
  anomalies_last_24h: number
  top_offenders: { src_ip: string; count: number }[]
  severity_breakdown: { warning: number; critical: number }
}

export interface MlAnomaly {
  id: number
  exporter_ip: string
  src_ip: string
  severity: 'warning' | 'critical'
  message: string
  pps: number | null
  score: number | null      // extracted from message via regex if not in schema
  explanation: string | null
  created_at: string | null
}
```

All actions use `withLoading` + `authHeaders()` pattern from `alerts.ts`.

---

## Sidebar entry

`AppLayout.vue` — add in the sidebar below "Alertas" section:

```html
<!-- IA -->
<div class="px-3 pt-4 pb-1 text-xs font-semibold text-zinc-600 uppercase tracking-wider">IA</div>
<SidebarLink to="/ai" :icon="Brain" label="Insights" />
```

Import `Brain` from `lucide-vue-next`.

---

## `router.ts`

```typescript
{ path: '/ai', component: () => import('./views/AiInsights.vue'), meta: { requiresAuth: true } }
```

Available to all authenticated users (not admin-only — visibility is fine).

---

## Warm-up progress bar

During warm-up, show a progress bar in the exporter status table:

```html
<div class="w-full bg-zinc-700 rounded-full h-1.5 mt-1">
  <div class="bg-amber-500 h-1.5 rounded-full transition-all"
       :style="{ width: Math.min(100, (s.samples_collected / s.samples_needed) * 100) + '%' }">
  </div>
</div>
<span class="text-xs text-zinc-500">{{ s.samples_collected.toLocaleString() }} / {{ s.samples_needed.toLocaleString() }}</span>
```

---

## Acceptance criteria

- [ ] `/ai` page loads without errors; requires auth
- [ ] Status table shows all active exporters with warm-up progress when applicable
- [ ] LLM status badge shows model name when enabled, "desabilitado" when not
- [ ] ML anomaly list auto-refreshes every 30s (same pattern as AlertEvents)
- [ ] Score badge uses correct color thresholds
- [ ] Explanation column shows "gerando..." while `null`, full text when available
- [ ] `cargo build` passes for `ml_api.rs`
- [ ] No `unsafe` code
