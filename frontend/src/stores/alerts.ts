import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useAuthStore } from './auth'

export interface AlertRule {
  id: number
  name: string
  exporter_id: number | null
  rule_type: string
  enabled: boolean
  params: Record<string, unknown>
  created_at: string | null
}

export interface AlertEvent {
  id: number
  rule_id: number | null
  exporter_ip: string
  src_ip: string
  alert_type: string
  severity: 'warning' | 'critical'
  message: string
  upload_bytes: number | null
  download_bytes: number | null
  pps: number | null
  avg_pkt_bytes: number | null
  attack_ports: string | null
  notified: boolean
  bgp_announced: boolean
  created_at: string | null
}

export interface EventsResponse {
  total: number
  events: AlertEvent[]
}

export interface TelegramConfig {
  bot_token: string
  chat_id: string
  enabled: boolean
  min_severity: string
}

function authHeaders() {
  const auth = useAuthStore()
  return {
    Authorization: `Bearer ${auth.token}`,
    'Content-Type': 'application/json',
  }
}

async function apiCall<T>(method: string, path: string, body?: unknown): Promise<T> {
  const resp = await fetch(path, {
    method,
    headers: authHeaders(),
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }))
    throw new Error(err.error || resp.statusText)
  }
  if (resp.status === 204) return undefined as T
  return resp.json()
}

export const useAlertsStore = defineStore('alerts', () => {
  const rules = ref<AlertRule[]>([])
  const events = ref<AlertEvent[]>([])
  const eventsTotal = ref(0)
  const telegram = ref<TelegramConfig | null>(null)
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

  async function loadRules() {
    rules.value = await withLoading(() => apiCall<AlertRule[]>('GET', '/api/alerts/rules'))
  }

  async function createRule(data: Omit<AlertRule, 'id' | 'created_at'>) {
    const rule = await withLoading(() => apiCall<AlertRule>('POST', '/api/alerts/rules', data))
    rules.value.unshift(rule)
    return rule
  }

  async function updateRule(id: number, data: Omit<AlertRule, 'id' | 'created_at'>) {
    const rule = await withLoading(() => apiCall<AlertRule>('PUT', `/api/alerts/rules/${id}`, data))
    const idx = rules.value.findIndex(r => r.id === id)
    if (idx >= 0) rules.value[idx] = rule
    return rule
  }

  async function deleteRule(id: number) {
    await withLoading(() => apiCall<void>('DELETE', `/api/alerts/rules/${id}`))
    rules.value = rules.value.filter(r => r.id !== id)
  }

  async function toggleRule(id: number) {
    const result = await withLoading(() =>
      apiCall<{ id: number; enabled: boolean }>('PATCH', `/api/alerts/rules/${id}/toggle`)
    )
    const idx = rules.value.findIndex(r => r.id === id)
    if (idx >= 0) rules.value[idx].enabled = result.enabled
    return result
  }

  async function loadEvents(params?: { limit?: number; offset?: number; severity?: string; notified?: string }) {
    const qs = new URLSearchParams()
    if (params?.limit) qs.set('limit', String(params.limit))
    if (params?.offset) qs.set('offset', String(params.offset))
    if (params?.severity) qs.set('severity', params.severity)
    if (params?.notified !== undefined) qs.set('notified', params.notified)
    const resp = await withLoading(() =>
      apiCall<EventsResponse>('GET', `/api/alerts/events?${qs}`)
    )
    events.value = resp.events
    eventsTotal.value = resp.total
    return resp
  }

  async function clearEvents() {
    await withLoading(() => apiCall<void>('DELETE', '/api/alerts/events'))
    events.value = []
    eventsTotal.value = 0
  }

  async function loadTelegram() {
    telegram.value = await withLoading(() => apiCall<TelegramConfig>('GET', '/api/alerts/telegram'))
    return telegram.value
  }

  async function saveTelegram(data: TelegramConfig) {
    telegram.value = await withLoading(() => apiCall<TelegramConfig>('PUT', '/api/alerts/telegram', data))
    return telegram.value!
  }

  async function testTelegram(): Promise<{ ok: boolean; message: string }> {
    return withLoading(() => apiCall<{ ok: boolean; message: string }>('POST', '/api/alerts/telegram/test'))
  }

  return {
    rules, events, eventsTotal, telegram, loading, error,
    loadRules, createRule, updateRule, deleteRule, toggleRule,
    loadEvents, clearEvents,
    loadTelegram, saveTelegram, testTelegram,
  }
})
