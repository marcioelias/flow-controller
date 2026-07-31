<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { Line, Doughnut } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale, LinearScale, PointElement, LineElement,
  Title, Tooltip, Legend, ArcElement, Filler,
} from 'chart.js'
import {
  LayoutDashboard, Activity, Wifi, BarChart2, ArrowDown, ArrowUp,
  Gauge, Zap, Users, Bell, Radio, Flame,
} from 'lucide-vue-next'
import { formatBytes } from '../utils/format'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler)

const authStore = useAuthStore()
const router = useRouter()

// Series colors — validated pair on zinc-900 (dataviz palette dark slots 1/2)
const COLOR_IN = '#3987e5'
const COLOR_OUT = '#d95926'

interface Exporter { id: number; ip_address: string; name: string; enabled: boolean }
interface ExporterStat {
  exporter_ip: string; total_bytes: number; flow_count: number; unique_sources: number
  in_bytes: number; out_bytes: number; unknown_bytes: number; direction_mode: string
}
interface Overview {
  current_bps_in: number; current_bps_out: number; current_pps: number; flows_per_sec: number
  peak_bps_5m: number; p95_bps: number; avg_bps: number
  active_talkers: number; active_exporters: number; total_bytes_24h: number
  alerts_24h: number; alerts_active: number
  bgp_sessions_up: number; bgp_sessions_total: number
  top_protocol: string
  sampling_exporters: { exporter_ip: string; rate: number }[]
}
interface TopTalker { src_ip: string; total_bytes: number; in_bytes: number; out_bytes: number }
interface AlertEvent { id: number; src_ip: string; alert_type: string; severity: string; message: string; created_at: string }
interface TimelinePoint { minute: number; total_bytes: number }

const exporters = ref<Exporter[]>([])
const exporterStats = ref<ExporterStat[]>([])
const selectedDevice = ref<string | null>(null)
const protocolStats = ref({ tcp: 0, udp: 0, icmp: 0, other: 0 })
const overview = ref<Overview | null>(null)
const topTalkers = ref<TopTalker[]>([])
const recentAlerts = ref<AlertEvent[]>([])
const canSeeAlerts = ref(true)
const heatmap = ref<number[][]>([]) // [7 days][24 hours] bytes
const heatmapMax = ref(0)

// live stats
const liveTotalBps = ref(0)

let chartLabels: string[] = []
let chartDataPoints: number[] = []
let lastTrafficValue = 0

const lineChartData = ref({
  labels: [] as string[],
  datasets: [{
    label: 'Mbps',
    backgroundColor: 'rgba(16,185,129,0.08)',
    borderColor: '#10b981',
    borderWidth: 2,
    data: [] as number[],
    tension: 0.4,
    fill: true,
    pointRadius: 0,
    pointHoverRadius: 4,
  }],
})

const donutChartData = ref({
  labels: ['TCP', 'UDP', 'ICMP', 'Outros'],
  datasets: [{ backgroundColor: ['#10b981', '#3b82f6', '#f59e0b', '#ef4444'], data: [0, 0, 0, 0], borderWidth: 0 }],
})

const chartOptions = {
  responsive: true,
  maintainAspectRatio: false,
  animation: { duration: 0 },
  interaction: { mode: 'index' as const, intersect: false },
  plugins: {
    legend: { display: false },
    tooltip: { callbacks: { label: (c: any) => ` ${c.parsed.y.toFixed(2)} Mbps` } },
  },
  scales: {
    x: {
      ticks: { color: '#6b7280', maxRotation: 0, autoSkip: true, maxTicksLimit: 6 },
      grid: { display: false },
    },
    y: {
      ticks: { color: '#6b7280', callback: (v: any) => v.toFixed(1) },
      grid: { color: '#27272a' },
      beginAtZero: true,
    },
  },
}

const donutOptions: any = {
  responsive: true,
  maintainAspectRatio: false,
  plugins: { legend: { position: 'bottom', labels: { color: '#9ca3af', padding: 12, boxWidth: 10 } } },
  borderWidth: 0,
}

// ── formatting helpers ──
function formatBps(bps: number): string {
  if (bps >= 1e9) return (bps / 1e9).toFixed(2) + ' Gbps'
  if (bps >= 1e6) return (bps / 1e6).toFixed(1) + ' Mbps'
  if (bps >= 1e3) return (bps / 1e3).toFixed(1) + ' kbps'
  return bps.toFixed(0) + ' bps'
}
function formatCount(n: number): string {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M'
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'k'
  return String(n)
}

const topExporter = computed(() => (exporterStats.value.length > 0 ? exporterStats.value[0] : null))
const topExporterName = computed(() => {
  if (!topExporter.value) return '—'
  const found = exporters.value.find(e => e.ip_address === topExporter.value!.exporter_ip)
  return found ? found.name : topExporter.value.exporter_ip
})
const maxTalkerBytes = computed(() => Math.max(1, ...topTalkers.value.map(t => t.total_bytes)))
const configuredExporters = computed(() => exporters.value.filter(e => e.enabled).length)
const exportersMissing = computed(
  () => overview.value !== null && configuredExporters.value > overview.value.active_exporters,
)

function exporterName(ip: string) {
  const found = exporters.value.find(e => e.ip_address === ip)
  return found ? found.name : ip
}

function selectDevice(ip: string | null) {
  selectedDevice.value = ip
  lastTrafficValue = 0
  loadProtocolStats()
}

// ── data loading ──
async function loadExporters() {
  try {
    const res = await fetch('/api/exporters/enabled', { headers: { Authorization: `Bearer ${authStore.token}` } })
    if (res.ok) exporters.value = await res.json()
  } catch {}
}

async function loadOverview() {
  try {
    const res = await fetch('/api/stats/overview?minutes=60', { headers: authStore.getAuthHeaders() })
    if (res.ok) overview.value = await res.json()
  } catch {}
}

async function loadProtocolStats() {
  try {
    const url = selectedDevice.value
      ? `/api/stats/protocols?exporter_ip=${encodeURIComponent(selectedDevice.value)}`
      : '/api/stats/protocols'
    const res = await fetch(url, { headers: { Authorization: `Bearer ${authStore.token}` } })
    if (res.ok) {
      const stats = await res.json()
      protocolStats.value = stats
      const total = stats.tcp + stats.udp + stats.icmp + stats.other
      if (total > 0) {
        donutChartData.value = {
          labels: ['TCP', 'UDP', 'ICMP', 'Outros'],
          datasets: [{ backgroundColor: ['#10b981', '#3b82f6', '#f59e0b', '#ef4444'], data: [stats.tcp, stats.udp, stats.icmp, stats.other], borderWidth: 0 }],
        }
      }
    }
  } catch {}
}

async function loadExporterStats() {
  try {
    const res = await fetch('/api/stats/exporters?minutes=5', { headers: authStore.getAuthHeaders() })
    if (res.ok) exporterStats.value = await res.json()
  } catch {}
}

async function loadTopTalkers() {
  try {
    const res = await fetch('/api/stats/top-talkers?minutes=5&limit=8', { headers: authStore.getAuthHeaders() })
    if (res.ok) topTalkers.value = await res.json()
  } catch {}
}

async function loadRecentAlerts() {
  if (!canSeeAlerts.value) return
  try {
    const res = await fetch('/api/alerts/events?limit=5', { headers: authStore.getAuthHeaders() })
    if (res.status === 403 || res.status === 401) {
      canSeeAlerts.value = false // regular user — panel falls back to counts
      return
    }
    if (res.ok) {
      const body = await res.json()
      recentAlerts.value = Array.isArray(body) ? body : (body.events ?? [])
    }
  } catch {}
}

// 7-day hour×day heatmap from hourly timeline buckets
async function loadHeatmap() {
  try {
    const res = await fetch('/api/stats/timeline?hours=168', { headers: authStore.getAuthHeaders() })
    if (!res.ok) return
    const points: TimelinePoint[] = await res.json()
    const grid: number[][] = Array.from({ length: 7 }, () => Array(24).fill(0))
    let max = 0
    for (const p of points) {
      const d = new Date(p.minute * 1000)
      const day = d.getDay() // 0=Dom
      const hour = d.getHours()
      grid[day][hour] += p.total_bytes
      if (grid[day][hour] > max) max = grid[day][hour]
    }
    heatmap.value = grid
    heatmapMax.value = max || 1
  } catch {}
}

// Sequential ramp: single emerald hue, light→dark by intensity
function heatColor(v: number): string {
  if (v === 0) return 'rgba(63,63,70,0.35)' // zinc-700-ish empty cell
  const t = Math.pow(v / heatmapMax.value, 0.45) // perceptual-ish boost for low values
  return `rgba(16,185,129,${(0.12 + 0.88 * t).toFixed(3)})`
}

const dayNames = ['Dom', 'Seg', 'Ter', 'Qua', 'Qui', 'Sex', 'Sáb']

function severityClass(sev: string) {
  return sev === 'critical'
    ? 'bg-red-500/10 text-red-400 border border-red-500/20'
    : 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
}

function formatAlertTime(ts: string) {
  const d = new Date(ts.endsWith('Z') ? ts : ts + 'Z')
  return d.toLocaleString('pt-BR', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function updateChart() {
  const timeLabel = new Date().toTimeString().slice(0, 8)
  chartLabels.push(timeLabel)
  chartDataPoints.push(lastTrafficValue)
  if (chartLabels.length > 150) { chartLabels.shift(); chartDataPoints.shift() }

  lineChartData.value = {
    labels: [...chartLabels],
    datasets: [{
      label: 'Mbps',
      backgroundColor: 'rgba(16,185,129,0.08)',
      borderColor: '#10b981',
      borderWidth: 2,
      data: [...chartDataPoints],
      tension: 0.4,
      fill: true,
      pointRadius: 0,
      pointHoverRadius: 4,
    }],
  }
}

let ws: WebSocket | null = null
let chartTimer: any = null
let pollTimer: any = null
let overviewTimer: any = null

function connectWs() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${proto}//${location.host}/ws`)

  ws.onmessage = (e) => {
    try {
      const stats = JSON.parse(e.data)
      if (!stats.timestamp_sec) return
      const bytes = selectedDevice.value
        ? (stats.per_device?.[selectedDevice.value] ?? 0)
        : (stats.total_bytes ?? 0)
      lastTrafficValue = (bytes * 8) / 1_000_000
      liveTotalBps.value = lastTrafficValue
    } catch {}
  }
  ws.onclose = () => setTimeout(connectWs, 3000)
}

const visible = () => document.visibilityState === 'visible'

onMounted(() => {
  loadExporters()
  loadOverview()
  loadProtocolStats()
  loadExporterStats()
  loadTopTalkers()
  loadRecentAlerts()
  loadHeatmap()
  connectWs()
  overviewTimer = setInterval(() => { if (visible()) { loadOverview(); loadTopTalkers(); loadRecentAlerts() } }, 10_000)
  pollTimer = setInterval(() => { if (visible()) { loadExporters(); loadProtocolStats(); loadExporterStats(); loadHeatmap() } }, 60_000)
  chartTimer = setInterval(updateChart, 2000)
})

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer)
  if (overviewTimer) clearInterval(overviewTimer)
  if (chartTimer) clearInterval(chartTimer)
  if (ws) ws.close()
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-7xl mx-auto space-y-6">

      <!-- Page header -->
      <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold tracking-tight flex items-center gap-2">
            <LayoutDashboard class="w-6 h-6 text-emerald-500" />
            Visão Geral
          </h1>
          <p class="text-zinc-400 mt-1 text-sm">Monitoramento em tempo real</p>
        </div>

        <div class="relative min-w-[220px]">
          <select
            :value="selectedDevice"
            @change="selectDevice(($event.target as HTMLSelectElement).value || null)"
            class="appearance-none w-full pl-4 pr-10 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer hover:border-zinc-700 transition-colors"
          >
            <option value="">Todos os Dispositivos</option>
            <option v-for="exp in exporters" :key="exp.id" :value="exp.ip_address">
              {{ exp.name }} ({{ exp.ip_address }})
            </option>
          </select>
          <div class="pointer-events-none absolute inset-y-0 right-0 flex items-center px-3 text-zinc-500">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/></svg>
          </div>
        </div>
      </header>

      <!-- Sampling warning -->
      <div
        v-if="overview && overview.sampling_exporters.length > 0"
        class="text-xs text-zinc-400 bg-zinc-900/60 border border-zinc-800 rounded-lg px-4 py-2"
      >
        Volumes corrigidos por sampling:
        <span v-for="(s, i) in overview.sampling_exporters" :key="s.exporter_ip" class="font-mono text-emerald-400">
          {{ s.exporter_ip }} (1:{{ s.rate }}){{ i < overview.sampling_exporters.length - 1 ? ', ' : '' }}
        </span>
      </div>

      <!-- NOC stat tiles -->
      <div v-if="overview" class="grid grid-cols-2 md:grid-cols-4 xl:grid-cols-8 gap-3">
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Entrada</span>
            <ArrowDown class="w-3.5 h-3.5" :style="{ color: COLOR_IN }" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(overview.current_bps_in) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">último minuto</p>
        </div>

        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Saída</span>
            <ArrowUp class="w-3.5 h-3.5" :style="{ color: COLOR_OUT }" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(overview.current_bps_out) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">último minuto</p>
        </div>

        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Pico 5 min</span>
            <Zap class="w-3.5 h-3.5 text-amber-400" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(overview.peak_bps_5m) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">máx. bucket 1 min</p>
        </div>

        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">95º perc.</span>
            <Gauge class="w-3.5 h-3.5 text-purple-400" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(overview.p95_bps) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">janela 60 min</p>
        </div>

        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Flows/s</span>
            <Activity class="w-3.5 h-3.5 text-emerald-500" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatCount(overview.flows_per_sec) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">{{ formatCount(overview.current_pps) }} pps</p>
        </div>

        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Talkers</span>
            <Users class="w-3.5 h-3.5 text-blue-400" />
          </div>
          <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatCount(overview.active_talkers) }}</p>
          <p class="text-[11px] text-zinc-500 mt-0.5">IPs ativos (5 min)</p>
        </div>

        <div
          class="bg-zinc-900 border rounded-xl p-4 cursor-pointer transition-colors"
          :class="exportersMissing ? 'border-red-500/40 hover:border-red-500/60' : 'border-zinc-800 hover:border-zinc-700'"
          @click="router.push('/exporters')"
        >
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Exporters</span>
            <Wifi class="w-3.5 h-3.5" :class="exportersMissing ? 'text-red-400' : 'text-emerald-500'" />
          </div>
          <p class="text-xl font-bold tabular-nums leading-tight" :class="exportersMissing ? 'text-red-400' : 'text-slate-100'">
            {{ overview.active_exporters }}<span class="text-zinc-500 text-sm">/{{ configuredExporters }}</span>
          </p>
          <p class="text-[11px] text-zinc-500 mt-0.5">enviando flows</p>
        </div>

        <div
          class="bg-zinc-900 border rounded-xl p-4 cursor-pointer transition-colors"
          :class="overview.alerts_active > 0 ? 'border-red-500/40 hover:border-red-500/60' : 'border-zinc-800 hover:border-zinc-700'"
          @click="router.push('/alerts/events')"
        >
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Alertas 24h</span>
            <Bell class="w-3.5 h-3.5" :class="overview.alerts_active > 0 ? 'text-red-400 animate-pulse' : 'text-zinc-500'" />
          </div>
          <p class="text-xl font-bold tabular-nums leading-tight" :class="overview.alerts_active > 0 ? 'text-red-400' : 'text-slate-100'">
            {{ overview.alerts_24h }}
          </p>
          <p class="text-[11px] text-zinc-500 mt-0.5">{{ overview.alerts_active }} na última hora</p>
        </div>
      </div>

      <!-- Charts row -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Live line chart -->
        <div class="lg:col-span-2 bg-zinc-900 border border-zinc-800 rounded-xl p-6">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-base font-semibold text-slate-200">Tráfego em tempo real</h2>
              <p class="text-xs text-zinc-500 mt-0.5">
                {{ liveTotalBps.toFixed(1) }} Mbps agora — últimos 5 minutos
              </p>
            </div>
            <span class="relative flex h-2 w-2">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-60"></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
            </span>
          </div>
          <div class="h-64">
            <Line :data="lineChartData" :options="chartOptions" />
          </div>
        </div>

        <!-- Doughnut -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
          <div class="mb-4">
            <h2 class="text-base font-semibold text-slate-200">Protocolos L4</h2>
            <p class="text-xs text-zinc-500 mt-0.5">Distribuição por bytes (5 min)</p>
          </div>
          <div class="h-64 flex items-center justify-center">
            <Doughnut :data="donutChartData" :options="donutOptions" />
          </div>
        </div>
      </div>

      <!-- Middle row: top talkers + operational state -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Top talkers -->
        <div class="lg:col-span-2 bg-zinc-900 border border-zinc-800 rounded-xl p-6">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-base font-semibold text-slate-200 flex items-center gap-2">
                <BarChart2 class="w-4 h-4 text-emerald-500" /> Top Talkers
              </h2>
              <p class="text-xs text-zinc-500 mt-0.5">Maiores IPs por volume (5 min)</p>
            </div>
            <button class="text-xs text-emerald-400 hover:text-emerald-300" @click="router.push('/top-talkers')">
              ver todos →
            </button>
          </div>
          <div v-if="topTalkers.length === 0" class="text-sm text-zinc-500 py-8 text-center">Aguardando dados…</div>
          <div v-else class="space-y-2.5">
            <div v-for="t in topTalkers" :key="t.src_ip" class="flex items-center gap-3">
              <span class="font-mono text-xs text-zinc-300 w-32 truncate" :title="t.src_ip">{{ t.src_ip }}</span>
              <div class="flex-1 h-4 bg-zinc-800/60 rounded overflow-hidden flex">
                <!-- in/out split within the bar; 2px gap via margin -->
                <div
                  class="h-full rounded-l"
                  :style="{ width: (t.in_bytes / maxTalkerBytes * 100) + '%', backgroundColor: COLOR_IN }"
                  :title="'Entrada: ' + formatBytes(t.in_bytes)"
                ></div>
                <div
                  class="h-full"
                  :style="{ width: (t.out_bytes / maxTalkerBytes * 100) + '%', backgroundColor: COLOR_OUT, marginLeft: t.in_bytes > 0 && t.out_bytes > 0 ? '2px' : '0' }"
                  :title="'Saída: ' + formatBytes(t.out_bytes)"
                ></div>
                <div
                  class="h-full bg-zinc-600"
                  :style="{ width: (Math.max(0, t.total_bytes - t.in_bytes - t.out_bytes) / maxTalkerBytes * 100) + '%' }"
                  title="Sem direção"
                ></div>
              </div>
              <span class="text-xs text-zinc-400 tabular-nums w-20 text-right">{{ formatBytes(t.total_bytes) }}</span>
            </div>
            <div class="flex items-center gap-4 pt-2 text-[11px] text-zinc-500">
              <span class="flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-sm inline-block" :style="{ backgroundColor: COLOR_IN }"></span> Entrada</span>
              <span class="flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-sm inline-block" :style="{ backgroundColor: COLOR_OUT }"></span> Saída</span>
              <span class="flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-sm inline-block bg-zinc-600"></span> Sem direção</span>
            </div>
          </div>
        </div>

        <!-- Operational state -->
        <div class="space-y-6">
          <!-- BGP -->
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 cursor-pointer hover:border-zinc-700 transition-colors" @click="router.push('/bgp')">
            <h2 class="text-base font-semibold text-slate-200 flex items-center gap-2 mb-3">
              <Radio class="w-4 h-4 text-blue-400" /> Sessões BGP
            </h2>
            <div v-if="overview" class="flex items-baseline gap-2">
              <span class="text-3xl font-bold tabular-nums" :class="overview.bgp_sessions_total > 0 && overview.bgp_sessions_up < overview.bgp_sessions_total ? 'text-amber-400' : 'text-slate-100'">
                {{ overview.bgp_sessions_up }}<span class="text-zinc-500 text-lg">/{{ overview.bgp_sessions_total }}</span>
              </span>
              <span class="text-xs text-zinc-500">estabelecidas</span>
            </div>
            <p v-if="overview && overview.bgp_sessions_total === 0" class="text-xs text-zinc-500 mt-1">nenhum peer configurado</p>
          </div>

          <!-- Recent alerts -->
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
            <div class="flex items-center justify-between mb-3">
              <h2 class="text-base font-semibold text-slate-200 flex items-center gap-2">
                <Bell class="w-4 h-4 text-amber-400" /> Alertas recentes
              </h2>
              <button v-if="canSeeAlerts" class="text-xs text-emerald-400 hover:text-emerald-300" @click="router.push('/alerts/events')">
                ver todos →
              </button>
            </div>
            <div v-if="!canSeeAlerts && overview" class="text-sm text-zinc-400">
              {{ overview.alerts_24h }} eventos nas últimas 24h
            </div>
            <div v-else-if="recentAlerts.length === 0" class="text-sm text-zinc-500 py-4 text-center">
              Nenhum alerta — tudo tranquilo 👌
            </div>
            <ul v-else class="space-y-2">
              <li v-for="a in recentAlerts" :key="a.id" class="flex items-start gap-2 text-xs">
                <span :class="['px-1.5 py-0.5 rounded font-medium shrink-0', severityClass(a.severity)]">
                  {{ a.severity === 'critical' ? 'CRIT' : 'WARN' }}
                </span>
                <div class="min-w-0">
                  <p class="text-zinc-300 truncate" :title="a.message">
                    <span class="font-mono">{{ a.src_ip }}</span> — {{ a.alert_type }}
                  </p>
                  <p class="text-zinc-600">{{ formatAlertTime(a.created_at) }}</p>
                </div>
              </li>
            </ul>
          </div>
        </div>
      </div>

      <!-- Weekly heatmap -->
      <div v-if="heatmap.length > 0" class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
        <div class="mb-4">
          <h2 class="text-base font-semibold text-slate-200 flex items-center gap-2">
            <Flame class="w-4 h-4 text-emerald-500" /> Ritmo semanal
          </h2>
          <p class="text-xs text-zinc-500 mt-0.5">Volume por hora × dia — últimos 7 dias</p>
        </div>
        <div class="overflow-x-auto">
          <div class="min-w-[640px]">
            <div v-for="(row, day) in heatmap" :key="day" class="flex items-center gap-[3px] mb-[3px]">
              <span class="text-[10px] text-zinc-500 w-8 shrink-0">{{ dayNames[day] }}</span>
              <div
                v-for="(v, hour) in row"
                :key="hour"
                class="h-5 flex-1 rounded-[3px]"
                :style="{ backgroundColor: heatColor(v) }"
                :title="`${dayNames[day]} ${String(hour).padStart(2, '0')}h — ${formatBytes(v)}`"
              ></div>
            </div>
            <div class="flex items-center gap-[3px] mt-1">
              <span class="w-8 shrink-0"></span>
              <span v-for="h in 24" :key="h" class="flex-1 text-center text-[9px] text-zinc-600">
                {{ (h - 1) % 3 === 0 ? String(h - 1).padStart(2, '0') : '' }}
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- Per-exporter table -->
      <div v-if="exporterStats.length > 0" class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-6 py-4 border-b border-zinc-800 flex items-center justify-between">
          <div>
            <h2 class="text-base font-semibold text-slate-200">Resumo por dispositivo</h2>
            <p class="text-xs text-zinc-500 mt-0.5">Últimos 5 minutos</p>
          </div>
          <span v-if="topExporter" class="text-xs text-zinc-500">
            maior emissor: <span class="text-slate-300">{{ topExporterName }}</span>
          </span>
        </div>
        <table class="w-full text-sm">
          <thead>
            <tr class="text-zinc-400 text-left border-b border-zinc-800">
              <th class="px-6 py-3 font-medium">Dispositivo</th>
              <th class="px-6 py-3 font-medium">IP</th>
              <th class="px-6 py-3 font-medium text-right">Entrada</th>
              <th class="px-6 py-3 font-medium text-right">Saída</th>
              <th class="px-6 py-3 font-medium text-right">Volume</th>
              <th class="px-6 py-3 font-medium text-right">Flows</th>
              <th class="px-6 py-3 font-medium text-right">Origens únicas</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="stat in exporterStats"
              :key="stat.exporter_ip"
              class="border-b border-zinc-800/50 hover:bg-zinc-800/20 transition-colors cursor-pointer"
              @click="selectDevice(stat.exporter_ip)"
            >
              <td class="px-6 py-3 font-medium text-slate-200">{{ exporterName(stat.exporter_ip) }}</td>
              <td class="px-6 py-3 font-mono text-xs text-zinc-400">{{ stat.exporter_ip }}</td>
              <td class="px-6 py-3 text-right tabular-nums" :style="{ color: COLOR_IN }">
                {{ stat.in_bytes > 0 ? formatBytes(stat.in_bytes) : '—' }}
              </td>
              <td class="px-6 py-3 text-right tabular-nums" :style="{ color: COLOR_OUT }">
                {{ stat.out_bytes > 0 ? formatBytes(stat.out_bytes) : '—' }}
              </td>
              <td class="px-6 py-3 text-right text-emerald-400 font-medium">{{ formatBytes(stat.total_bytes) }}</td>
              <td class="px-6 py-3 text-right text-zinc-300">{{ stat.flow_count.toLocaleString('pt-BR') }}</td>
              <td class="px-6 py-3 text-right text-zinc-400">{{ stat.unique_sources.toLocaleString('pt-BR') }}</td>
            </tr>
          </tbody>
        </table>
      </div>

    </div>
  </div>
</template>
