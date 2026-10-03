<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAlertsStore, type AlertEvent, type AlertEventDetail } from '../stores/alerts'
import { Bell, Trash2, RefreshCw, X, Activity, Loader2 } from 'lucide-vue-next'
import SortTh from '../components/SortTh.vue'
import type { SortDir, SortState } from '../composables/useSort'
import { formatBps, formatBytes } from '../utils/format'

const store = useAlertsStore()

const severityFilter = ref('')
const notifiedFilter = ref('')
const page = ref(0)
const perPage = 50
const confirmClear = ref(false)

let refreshTimer: ReturnType<typeof setInterval> | null = null

// Paginada no servidor: a ordenação também é no servidor (task 17.3 R-05)
const sortKey = ref('created_at')
const sortDir = ref<SortDir>('desc')
const ASC_FIRST = new Set(['alert_type', 'exporter_ip', 'src_ip'])
const sort: SortState = {
  key: sortKey,
  dir: sortDir,
  toggle(k: string) {
    if (sortKey.value === k) {
      sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
    } else {
      sortKey.value = k
      sortDir.value = ASC_FIRST.has(k) ? 'asc' : 'desc'
    }
    page.value = 0
    load()
  },
}

const TYPE_LABELS: Record<string, string> = {
  upload_inversion: 'Inversão de upload',
  attack_signature: 'Assinatura de ataque',
  ml_anomaly: 'Anomalia ML',
}
function typeLabel(t: string) {
  return TYPE_LABELS[t] ?? t
}

const hasUploadInversion = computed(() =>
  store.events.some(e => e.alert_type === 'upload_inversion')
)
const hasAttackSignature = computed(() =>
  store.events.some(e => e.alert_type === 'attack_signature')
)

async function load() {
  const params: Record<string, string | number> = {
    limit: perPage,
    offset: page.value * perPage,
    sort: sortKey.value,
    dir: sortDir.value,
  }
  if (severityFilter.value) params.severity = severityFilter.value
  if (notifiedFilter.value) params.notified = notifiedFilter.value
  await store.loadEvents(params)
}

async function doClear() {
  await store.clearEvents()
  confirmClear.value = false
  page.value = 0
}

function formatDate(s: string | null) {
  if (!s) return '—'
  return new Date(s + 'Z').toLocaleString('pt-BR')
}

// upload/download são somados na janela curta da regra (task 17.4 R-03)
function windowRate(bytes: number | null, windowMin: number | null) {
  if (bytes == null) return '—'
  if (!windowMin) return formatBytes(bytes)
  return formatBps((bytes * 8) / (windowMin * 60))
}

const selected = ref<AlertEventDetail | null>(null)
const loadingDetail = ref(false)

async function openEvent(ev: AlertEvent) {
  loadingDetail.value = true
  selected.value = { ...ev, explanation: null, feedback: null, exporter_name: null, rule: null }
  try {
    selected.value = await store.getEvent(ev.id)
  } catch {
    // mantém o que a listagem já trouxe
  } finally {
    loadingDetail.value = false
  }
}

function closeEvent() {
  selected.value = null
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') closeEvent()
}

const ratio = computed(() => {
  const ev = selected.value
  if (!ev?.upload_bytes || ev.download_bytes == null) return null
  return ev.download_bytes / ev.upload_bytes
})

function severityClass(s: string) {
  return s === 'critical'
    ? 'bg-red-500/10 text-red-400 border-red-500/20'
    : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
}

const totalPages = computed(() => Math.ceil(store.eventsTotal / perPage))

onMounted(async () => {
  await load()
  refreshTimer = setInterval(load, 30_000)
  window.addEventListener('keydown', onKey)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
  window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-7xl mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Bell class="w-6 h-6 text-amber-500" />
        <h1 class="text-2xl font-bold text-slate-100">Eventos de Alerta</h1>
        <span class="px-2 py-0.5 rounded-full bg-zinc-700 text-zinc-400 text-xs">{{ store.eventsTotal }}</span>
      </div>
      <div class="flex items-center gap-2">
        <button @click="load" class="p-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 transition-colors">
          <RefreshCw class="w-4 h-4" />
        </button>
        <div v-if="!confirmClear">
          <button @click="confirmClear = true"
            class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-red-500/10 hover:bg-red-500/20 text-red-400 text-sm transition-colors">
            <Trash2 class="w-4 h-4" /> Limpar todos
          </button>
        </div>
        <div v-else class="flex items-center gap-2">
          <span class="text-xs text-red-400">Confirmar?</span>
          <button @click="doClear" class="text-xs px-2 py-1 rounded bg-red-500 hover:bg-red-400 text-white transition-colors">Sim</button>
          <button @click="confirmClear = false" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
        </div>
      </div>
    </div>

    <!-- Filters -->
    <div class="flex items-center gap-3">
      <select v-model="severityFilter" @change="page = 0; load()"
        class="bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm text-slate-200 focus:outline-none focus:border-amber-500">
        <option value="">Todas severidades</option>
        <option value="warning">Warning</option>
        <option value="critical">Critical</option>
      </select>
      <select v-model="notifiedFilter" @change="page = 0; load()"
        class="bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm text-slate-200 focus:outline-none focus:border-amber-500">
        <option value="">Todos</option>
        <option value="false">Não notificados</option>
        <option value="true">Notificados</option>
      </select>
    </div>

    <!-- Table -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="store.events.length === 0" class="text-center py-12 text-zinc-500 text-sm">
        Nenhum evento encontrado.
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs text-zinc-500 border-b border-zinc-800 bg-zinc-900/80">
              <SortTh :sort="sort" k="created_at" class="px-4 py-3">Hora</SortTh>
              <SortTh :sort="sort" k="severity" class="px-4 py-3">Severidade</SortTh>
              <SortTh :sort="sort" k="alert_type" class="px-4 py-3">Tipo</SortTh>
              <SortTh :sort="sort" k="exporter_ip" class="px-4 py-3">Exporter</SortTh>
              <SortTh :sort="sort" k="src_ip" class="px-4 py-3">IP</SortTh>
              <th class="px-4 py-3">Mensagem</th>
              <SortTh :sort="sort" k="bgp_announced" class="px-4 py-3">BGP</SortTh>
              <template v-if="hasUploadInversion">
                <th class="px-4 py-3">Upload</th>
                <th class="px-4 py-3">Download</th>
              </template>
              <template v-if="hasAttackSignature">
                <th class="px-4 py-3">PPS</th>
                <th class="px-4 py-3">Pkt avg</th>
                <th class="px-4 py-3">Portas</th>
              </template>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/50">
            <tr v-for="ev in store.events" :key="ev.id"
              class="hover:bg-zinc-800/30 transition-colors cursor-pointer"
              :class="ev.notified ? 'opacity-60' : ''"
              @click="openEvent(ev)"
            >
              <td class="px-4 py-2.5 text-zinc-500 text-xs whitespace-nowrap">{{ formatDate(ev.created_at) }}</td>
              <td class="px-4 py-2.5">
                <span class="px-2 py-0.5 rounded-full border text-xs font-semibold uppercase" :class="severityClass(ev.severity)">
                  {{ ev.severity }}
                </span>
              </td>
              <td class="px-4 py-2.5">
                <span class="text-xs text-zinc-300">{{ typeLabel(ev.alert_type) }}</span>
              </td>
              <td class="px-4 py-2.5 font-mono text-zinc-400 text-xs">{{ ev.exporter_ip }}</td>
              <td class="px-4 py-2.5 font-mono text-slate-200 text-xs">{{ ev.src_ip }}</td>
              <td class="px-4 py-2.5 text-zinc-400 text-xs max-w-xs truncate" :title="ev.message">{{ ev.message }}</td>
              <td class="px-4 py-2.5">
                <span v-if="ev.bgp_announced" class="px-1.5 py-0.5 rounded text-xs font-semibold bg-red-500/10 text-red-400 border border-red-500/20">BGP</span>
                <span v-else class="text-zinc-700">—</span>
              </td>
              <template v-if="hasUploadInversion">
                <td class="px-4 py-2.5 text-xs text-zinc-400">
                  <span v-if="ev.alert_type === 'upload_inversion'">{{ windowRate(ev.upload_bytes, ev.window_min) }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
                <td class="px-4 py-2.5 text-xs text-zinc-400">
                  <span v-if="ev.alert_type === 'upload_inversion'">{{ windowRate(ev.download_bytes, ev.window_min) }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
              </template>
              <template v-if="hasAttackSignature">
                <td class="px-4 py-2.5 text-xs text-zinc-400">
                  <span v-if="ev.pps != null">{{ ev.pps.toFixed(0) }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
                <td class="px-4 py-2.5 text-xs text-zinc-400">
                  <span v-if="ev.avg_pkt_bytes != null">{{ ev.avg_pkt_bytes.toFixed(0) }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
                <td class="px-4 py-2.5 text-xs font-mono text-zinc-400">
                  <span v-if="ev.attack_ports">{{ ev.attack_ports }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
              </template>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div v-if="totalPages > 1" class="flex items-center justify-between px-4 py-3 border-t border-zinc-800 text-xs text-zinc-500">
        <span>{{ store.eventsTotal }} eventos</span>
        <div class="flex items-center gap-2">
          <button :disabled="page === 0" @click="page--; load()"
            class="px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors">
            ← Anterior
          </button>
          <span>{{ page + 1 }} / {{ totalPages }}</span>
          <button :disabled="page + 1 >= totalPages" @click="page++; load()"
            class="px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors">
            Próxima →
          </button>
        </div>
      </div>
    </div>

    <!-- Detalhe do evento (task 17.4) -->
    <Teleport to="body">
      <div v-if="selected" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4" @click.self="closeEvent">
        <div class="bg-zinc-900 border border-zinc-700 rounded-xl w-full max-w-2xl max-h-[90vh] overflow-y-auto shadow-2xl">
          <div class="flex items-start justify-between px-6 py-4 border-b border-zinc-800">
            <div class="space-y-1">
              <div class="flex items-center gap-2">
                <span class="px-2 py-0.5 rounded-full border text-xs font-semibold uppercase" :class="severityClass(selected.severity)">
                  {{ selected.severity }}
                </span>
                <h2 class="text-lg font-semibold text-slate-100">{{ typeLabel(selected.alert_type) }}</h2>
                <Loader2 v-if="loadingDetail" class="w-4 h-4 text-zinc-500 animate-spin" />
              </div>
              <p class="text-xs text-zinc-500">{{ formatDate(selected.created_at) }} · evento #{{ selected.id }}</p>
            </div>
            <button class="text-zinc-500 hover:text-zinc-200" @click="closeEvent"><X class="w-5 h-5" /></button>
          </div>

          <div class="px-6 py-5 space-y-5 text-sm">
            <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
              <div>
                <p class="text-xs text-zinc-500">Exporter</p>
                <p class="text-zinc-200">{{ selected.exporter_name ?? '—' }}</p>
                <p class="font-mono text-xs text-zinc-500">{{ selected.exporter_ip }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">IP</p>
                <p class="font-mono text-zinc-200">{{ selected.src_ip }}</p>
                <router-link :to="`/talkers/${encodeURIComponent(selected.src_ip)}`"
                  class="inline-flex items-center gap-1 text-xs text-emerald-400 hover:text-emerald-300">
                  <Activity class="w-3 h-3" /> Analisar tráfego
                </router-link>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Regra</p>
                <p class="text-zinc-200">{{ selected.rule?.name ?? (loadingDetail ? '…' : 'regra removida') }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Notificado (Telegram)</p>
                <p :class="selected.notified ? 'text-emerald-400' : 'text-zinc-400'">{{ selected.notified ? 'Sim' : 'Não' }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Anúncio BGP</p>
                <p :class="selected.bgp_announced ? 'text-red-400' : 'text-zinc-400'">{{ selected.bgp_announced ? 'Anunciado' : 'Não' }}</p>
              </div>
            </div>

            <!-- Métricas por tipo -->
            <div v-if="selected.alert_type === 'upload_inversion'" class="grid grid-cols-2 md:grid-cols-3 gap-4">
              <div>
                <p class="text-xs text-zinc-500">Upload</p>
                <p class="font-mono text-zinc-200">{{ windowRate(selected.upload_bytes, selected.window_min) }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Download</p>
                <p class="font-mono text-zinc-200">{{ windowRate(selected.download_bytes, selected.window_min) }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Razão download/upload</p>
                <p class="font-mono text-zinc-200">{{ ratio != null ? ratio.toFixed(2) + '×' : '—' }}</p>
              </div>
              <p class="col-span-full text-xs text-zinc-600">
                Taxa média na janela da regra{{ selected.window_min ? ` (${selected.window_min} min)` : '' }}.
              </p>
            </div>

            <div v-else class="grid grid-cols-2 md:grid-cols-3 gap-4">
              <div>
                <p class="text-xs text-zinc-500">PPS</p>
                <p class="font-mono text-zinc-200">{{ selected.pps != null ? selected.pps.toFixed(0) : '—' }}</p>
              </div>
              <div>
                <p class="text-xs text-zinc-500">Pacote médio</p>
                <p class="font-mono text-zinc-200">{{ selected.avg_pkt_bytes != null ? selected.avg_pkt_bytes.toFixed(0) + ' B' : '—' }}</p>
              </div>
              <div v-if="selected.alert_type === 'attack_signature'">
                <p class="text-xs text-zinc-500">Portas</p>
                <p class="font-mono text-zinc-200 break-all">{{ selected.attack_ports ?? '—' }}</p>
              </div>
              <template v-if="selected.alert_type === 'ml_anomaly'">
                <div>
                  <p class="text-xs text-zinc-500">Volume upload (janela)</p>
                  <p class="font-mono text-zinc-200">{{ selected.upload_bytes != null ? formatBytes(selected.upload_bytes) : '—' }}</p>
                </div>
                <div>
                  <p class="text-xs text-zinc-500">Volume download (janela)</p>
                  <p class="font-mono text-zinc-200">{{ selected.download_bytes != null ? formatBytes(selected.download_bytes) : '—' }}</p>
                </div>
              </template>
            </div>

            <div v-if="selected.alert_type === 'ml_anomaly'" class="space-y-1">
              <div class="flex items-center justify-between">
                <p class="text-xs text-zinc-500">Explicação IA</p>
                <router-link to="/ai" class="text-xs text-emerald-400 hover:text-emerald-300">Abrir na tela de IA →</router-link>
              </div>
              <p v-if="selected.explanation" class="text-zinc-300 leading-relaxed">{{ selected.explanation }}</p>
              <p v-else class="text-zinc-600 italic">sem explicação</p>
              <p v-if="selected.feedback" class="text-xs text-zinc-500">
                Feedback do operador:
                <span class="text-zinc-300">{{ selected.feedback === 'false_positive' ? 'falso positivo' : 'ameaça confirmada' }}</span>
              </p>
            </div>

            <div>
              <p class="text-xs text-zinc-500 mb-1">Mensagem</p>
              <p class="font-mono text-xs text-zinc-300 bg-zinc-950/60 border border-zinc-800 rounded-lg px-3 py-2 whitespace-pre-wrap break-words">{{ selected.message }}</p>
            </div>

            <div v-if="selected.rule">
              <p class="text-xs text-zinc-500 mb-1">Parâmetros da regra <span class="text-zinc-600">({{ typeLabel(selected.rule.rule_type) }})</span></p>
              <dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs bg-zinc-950/60 border border-zinc-800 rounded-lg px-3 py-2">
                <template v-for="(v, k) in selected.rule.params" :key="k">
                  <dt class="font-mono text-zinc-500">{{ k }}</dt>
                  <dd class="font-mono text-zinc-300 break-all">{{ typeof v === 'object' ? JSON.stringify(v) : v }}</dd>
                </template>
              </dl>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
