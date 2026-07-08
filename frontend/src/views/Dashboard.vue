<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { Line, Doughnut } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale, LinearScale, PointElement, LineElement,
  Title, Tooltip, Legend, ArcElement, Filler,
} from 'chart.js'
import { LayoutDashboard, Activity, Wifi, ArrowUpDown, BarChart2 } from 'lucide-vue-next'
import { formatBytes } from '../utils/format'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler)

const authStore = useAuthStore()

interface Exporter { id: number; ip_address: string; name: string; enabled: boolean }
interface ExporterStat { exporter_ip: string; total_bytes: number; flow_count: number; unique_sources: number }

const exporters = ref<Exporter[]>([])
const exporterStats = ref<ExporterStat[]>([])
const selectedDevice = ref<string | null>(null)
const protocolStats = ref({ tcp: 0, udp: 0, icmp: 0, other: 0 })

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

// derived stat cards
const totalProtocolBytes = computed(() =>
  protocolStats.value.tcp + protocolStats.value.udp + protocolStats.value.icmp + protocolStats.value.other
)

const topExporter = computed(() =>
  exporterStats.value.length > 0 ? exporterStats.value[0] : null
)

const topExporterName = computed(() => {
  if (!topExporter.value) return '—'
  const found = exporters.value.find(e => e.ip_address === topExporter.value!.exporter_ip)
  return found ? found.name : topExporter.value.exporter_ip
})

// name lookup for stat table
function exporterName(ip: string) {
  const found = exporters.value.find(e => e.ip_address === ip)
  return found ? found.name : ip
}

function selectDevice(ip: string | null) {
  selectedDevice.value = ip
  lastTrafficValue = 0
  loadProtocolStats()
}

async function loadExporters() {
  try {
    const res = await fetch('/api/exporters/enabled', { headers: { Authorization: `Bearer ${authStore.token}` } })
    if (res.ok) exporters.value = await res.json()
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

onMounted(() => {
  loadExporters()
  loadProtocolStats()
  loadExporterStats()
  connectWs()
  pollTimer = setInterval(() => { loadExporters(); loadProtocolStats(); loadExporterStats() }, 30_000)
  chartTimer = setInterval(updateChart, 2000)
})

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer)
  if (chartTimer) clearInterval(chartTimer)
  if (ws) ws.close()
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-7xl mx-auto space-y-8">

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

      <!-- Stat cards -->
      <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <!-- Live Mbps -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 group hover:border-emerald-500/30 transition-colors">
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs font-medium text-zinc-500 uppercase tracking-wider">Velocidade atual</span>
            <Activity class="w-4 h-4 text-emerald-500" />
          </div>
          <p class="text-3xl font-bold text-slate-100 tabular-nums">
            {{ liveTotalBps.toFixed(1) }}
          </p>
          <p class="text-xs text-zinc-500 mt-1">Mbps</p>
        </div>

        <!-- Volume L7 (5min) -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 group hover:border-blue-500/30 transition-colors">
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs font-medium text-zinc-500 uppercase tracking-wider">Volume (5 min)</span>
            <ArrowUpDown class="w-4 h-4 text-blue-400" />
          </div>
          <p class="text-3xl font-bold text-slate-100">
            {{ formatBytes(totalProtocolBytes) }}
          </p>
          <p class="text-xs text-zinc-500 mt-1">total transferido</p>
        </div>

        <!-- Exporters online -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 group hover:border-amber-500/30 transition-colors">
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs font-medium text-zinc-500 uppercase tracking-wider">Dispositivos</span>
            <Wifi class="w-4 h-4 text-amber-400" />
          </div>
          <p class="text-3xl font-bold text-slate-100">{{ exporterStats.length || exporters.length }}</p>
          <p class="text-xs text-zinc-500 mt-1">enviando flows</p>
        </div>

        <!-- Top talker -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 group hover:border-purple-500/30 transition-colors">
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs font-medium text-zinc-500 uppercase tracking-wider">Maior emissor</span>
            <BarChart2 class="w-4 h-4 text-purple-400" />
          </div>
          <p class="text-lg font-bold text-slate-100 truncate" :title="topExporterName">
            {{ topExporterName }}
          </p>
          <p class="text-xs text-zinc-500 mt-1">
            {{ topExporter ? formatBytes(topExporter.total_bytes) + ' em 5 min' : 'Aguardando dados…' }}
          </p>
        </div>
      </div>

      <!-- Charts row -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Line chart -->
        <div class="lg:col-span-2 bg-zinc-900 border border-zinc-800 rounded-xl p-6">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-base font-semibold text-slate-200">Tráfego em tempo real</h2>
              <p class="text-xs text-zinc-500 mt-0.5">Bidirecional — últimos 5 minutos</p>
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

      <!-- Per-exporter table -->
      <div v-if="exporterStats.length > 0" class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-6 py-4 border-b border-zinc-800">
          <h2 class="text-base font-semibold text-slate-200">Resumo por dispositivo</h2>
          <p class="text-xs text-zinc-500 mt-0.5">Últimos 5 minutos</p>
        </div>
        <table class="w-full text-sm">
          <thead>
            <tr class="text-zinc-400 text-left border-b border-zinc-800">
              <th class="px-6 py-3 font-medium">Dispositivo</th>
              <th class="px-6 py-3 font-medium">IP</th>
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
