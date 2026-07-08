<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { Line } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  Filler,
} from 'chart.js'
import { TrendingUp, RefreshCw } from 'lucide-vue-next'
import { formatBytes } from '../utils/format'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, Filler)

const authStore = useAuthStore()

interface Exporter {
  id: number
  ip_address: string
  name: string
}

interface TimelinePoint {
  minute: number
  total_bytes: number
  total_packets: number
}

const exporters = ref<Exporter[]>([])
const selectedDevice = ref<string>('')
const selectedHours = ref(1)
const points = ref<TimelinePoint[]>([])
const loading = ref(false)

const hourOptions = [
  { label: 'Última 1h', value: 1 },
  { label: 'Últimas 6h', value: 6 },
  { label: 'Últimas 12h', value: 12 },
  { label: 'Últimas 24h', value: 24 },
]

function toMbps(bytes: number): number {
  return (bytes * 8) / 1e6 / 60
}

function formatLabel(ts: number): string {
  const d = new Date(ts * 1000)
  return selectedHours.value <= 6
    ? d.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleString('pt-BR', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

const lineChartData = computed(() => ({
  labels: points.value.map((p) => formatLabel(p.minute)),
  datasets: [
    {
      label: 'Tráfego (Mbps)',
      borderColor: '#10b981',
      backgroundColor: 'rgba(16,185,129,0.1)',
      borderWidth: 2,
      data: points.value.map((p) => parseFloat(toMbps(p.total_bytes).toFixed(3))),
      tension: 0.3,
      fill: true,
      pointRadius: 0,
      pointHoverRadius: 4,
    },
  ],
}))

const lineChartOptions = {
  responsive: true,
  maintainAspectRatio: false,
  animation: { duration: 0 },
  interaction: { mode: 'index' as const, intersect: false },
  plugins: {
    legend: { display: false },
    tooltip: {
      callbacks: {
        label: (ctx: any) => ` ${ctx.parsed.y.toFixed(2)} Mbps`,
      },
    },
  },
  scales: {
    x: {
      ticks: {
        color: '#9ca3af',
        maxRotation: 0,
        autoSkip: true,
        maxTicksLimit: 8,
      },
      grid: { display: false },
    },
    y: {
      ticks: {
        color: '#9ca3af',
        callback: (v: any) => v.toFixed(1) + ' Mbps',
      },
      grid: { color: '#374151' },
      beginAtZero: true,
    },
  },
}

const peak = computed(() => {
  if (points.value.length === 0) return 0
  return Math.max(...points.value.map((p) => p.total_bytes))
})

const average = computed(() => {
  if (points.value.length === 0) return 0
  return points.value.reduce((s, p) => s + p.total_bytes, 0) / points.value.length
})

const total = computed(() => points.value.reduce((s, p) => s + p.total_bytes, 0))

async function loadExporters() {
  try {
    const res = await fetch('/api/exporters/enabled', {
      headers: authStore.getAuthHeaders(),
    })
    if (res.ok) exporters.value = await res.json()
  } catch {}
}

async function loadData() {
  loading.value = true
  try {
    const params = new URLSearchParams({ hours: String(selectedHours.value) })
    if (selectedDevice.value) params.set('exporter_ip', selectedDevice.value)
    const res = await fetch(`/api/stats/timeline?${params}`, {
      headers: authStore.getAuthHeaders(),
    })
    if (res.ok) points.value = await res.json()
  } catch {
    points.value = []
  } finally {
    loading.value = false
  }
}

let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  loadExporters()
  loadData()
  timer = setInterval(loadData, 60000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-7xl mx-auto space-y-6">
      <!-- Header -->
      <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold tracking-tight flex items-center gap-2">
            <TrendingUp class="w-6 h-6 text-emerald-500" />
            Histórico de Tráfego
          </h1>
          <p class="text-zinc-400 mt-1">Série temporal com buckets de 1 minuto</p>
        </div>

        <div class="flex items-center gap-3">
          <select
            v-model="selectedDevice"
            @change="loadData"
            class="appearance-none pl-3 pr-8 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer"
          >
            <option value="">Todos os Dispositivos</option>
            <option v-for="exp in exporters" :key="exp.id" :value="exp.ip_address">
              {{ exp.name }} ({{ exp.ip_address }})
            </option>
          </select>

          <select
            v-model="selectedHours"
            @change="loadData"
            class="appearance-none pl-3 pr-8 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer"
          >
            <option v-for="opt in hourOptions" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </option>
          </select>

          <button
            @click="loadData"
            :disabled="loading"
            class="p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors"
          >
            <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': loading }" />
          </button>
        </div>
      </header>

      <!-- Chart -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
        <div v-if="points.length === 0 && !loading" class="flex flex-col items-center justify-center py-20 text-zinc-500 gap-2">
          <TrendingUp class="w-10 h-10" />
          <p>Nenhum dado disponível para este período.</p>
        </div>
        <div v-else class="h-72">
          <Line :data="lineChartData" :options="lineChartOptions" />
        </div>
      </div>

      <!-- Summary stats -->
      <div class="grid grid-cols-3 gap-4">
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <p class="text-xs text-zinc-500 uppercase tracking-wider mb-1">Pico</p>
          <p class="text-2xl font-bold text-slate-100">{{ toMbps(peak).toFixed(1) }} <span class="text-base font-normal text-zinc-400">Mbps</span></p>
        </div>
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <p class="text-xs text-zinc-500 uppercase tracking-wider mb-1">Média</p>
          <p class="text-2xl font-bold text-slate-100">{{ toMbps(average).toFixed(1) }} <span class="text-base font-normal text-zinc-400">Mbps</span></p>
        </div>
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <p class="text-xs text-zinc-500 uppercase tracking-wider mb-1">Total</p>
          <p class="text-2xl font-bold text-slate-100">{{ formatBytes(total) }}</p>
        </div>
      </div>
    </div>
  </div>
</template>
