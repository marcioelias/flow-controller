import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useAuthStore } from './auth'

export interface ExporterModelStatus {
  exporter_ip: string
  status: 'warming_up' | 'active' | 'no_data'
  samples_collected: number
  samples_needed: number
  n_scored: number
  anomalies_total: number
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
  avg_pkt_bytes: number | null
  upload_bytes: number | null
  download_bytes: number | null
  explanation: string | null
  created_at: string | null
}

export interface MlEventsResponse {
  total: number
  events: MlAnomaly[]
}

function authHeaders() {
  const auth = useAuthStore()
  return {
    Authorization: `Bearer ${auth.token}`,
    'Content-Type': 'application/json',
  }
}

async function apiGet<T>(path: string): Promise<T> {
  const resp = await fetch(path, { headers: authHeaders() })
  if (!resp.ok) throw new Error(resp.statusText)
  return resp.json()
}

export const useAiStore = defineStore('ai', () => {
  const status = ref<MlStatus | null>(null)
  const stats = ref<MlStats | null>(null)
  const events = ref<MlAnomaly[]>([])
  const eventsTotal = ref(0)
  const loading = ref(false)
  const error = ref('')

  async function withLoading<T>(fn: () => Promise<T>): Promise<T> {
    loading.value = true
    error.value = ''
    try {
      return await fn()
    } catch (e: unknown) {
      error.value = (e as Error).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function loadStatus() {
    status.value = await withLoading(() => apiGet<MlStatus>('/api/ml/status'))
  }

  async function loadStats() {
    stats.value = await withLoading(() => apiGet<MlStats>('/api/ml/stats'))
  }

  async function loadEvents(page = 0, limit = 50) {
    const resp = await withLoading(() =>
      apiGet<MlEventsResponse>(`/api/ml/events?limit=${limit}&offset=${page * limit}`)
    )
    events.value = resp.events
    eventsTotal.value = resp.total
  }

  return { status, stats, events, eventsTotal, loading, error, loadStatus, loadStats, loadEvents }
})
