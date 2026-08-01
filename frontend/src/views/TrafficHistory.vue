<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { COLOR_IN, COLOR_OUT, COLOR_UNKNOWN, COLOR_V4, COLOR_V6, withAlpha, mirroredLegend, mirroredTooltip, mirroredYTicks } from '../lib/chartTheme'
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
import { TrendingUp, RefreshCw, ArrowDown, ArrowUp } from 'lucide-vue-next'
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
  in_bytes: number
  in_packets: number
  out_bytes: number
  out_packets: number
  unknown_bytes: number
  unknown_packets: number
  v4_bytes: number
  v6_bytes: number
}

const exporters = ref<Exporter[]>([])
const selectedDevice = ref<string>('')
const selectedHours = ref(1)
// Modo do gráfico: espelhado por direção ou comparativo por família de IP
const viewMode = ref<'direction' | 'family'>('direction')
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

// Exporters that don't report IE 61 leave everything in unknown_*.
// When the whole window is unknown, fall back to a single total series
// instead of a mirrored chart with an empty bottom half.
const hasDirection = computed(() =>
  points.value.some((p) => p.in_bytes > 0 || p.out_bytes > 0),
)

const hasUnknown = computed(() => points.value.some((p) => p.unknown_bytes > 0))

const lineChartData = computed(() => {
  const labels = points.value.map((p) => formatLabel(p.minute))

  if (viewMode.value === 'family') {
    return {
      labels,
      datasets: [
        {
          label: 'IPv4',
          borderColor: COLOR_V4,
          backgroundColor: withAlpha(COLOR_V4, '66'),
          borderWidth: 0,
          data: points.value.map((p) => +toMbps(p.v4_bytes).toFixed(3)),
          tension: 0,
          fill: true,
          pointRadius: 0,
          pointHoverRadius: 4,
        },
        {
          label: 'IPv6',
          borderColor: COLOR_V6,
          backgroundColor: withAlpha(COLOR_V6, '66'),
          borderWidth: 0,
          data: points.value.map((p) => +toMbps(p.v6_bytes).toFixed(3)),
          tension: 0,
          fill: true,
          pointRadius: 0,
          pointHoverRadius: 4,
        },
      ],
    }
  }

  if (!hasDirection.value) {
    return {
      labels,
      datasets: [
        {
          label: 'Tráfego',
          borderColor: COLOR_IN,
          backgroundColor: withAlpha(COLOR_IN, '66'),
          borderWidth: 0,
          data: points.value.map((p) => +toMbps(p.total_bytes).toFixed(3)),
          tension: 0,
          fill: true,
          pointRadius: 0,
          pointHoverRadius: 4,
        },
      ],
    }
  }

  const datasets: any[] = [
    {
      label: 'Entrada',
      borderColor: COLOR_IN,
      backgroundColor: withAlpha(COLOR_IN, '66'),
      borderWidth: 0,
      // mirrored: inbound above the axis
      data: points.value.map((p) => +toMbps(p.in_bytes).toFixed(3)),
      tension: 0,
      fill: true,
      pointRadius: 0,
      pointHoverRadius: 4,
    },
    {
      label: 'Saída',
      borderColor: COLOR_OUT,
      backgroundColor: withAlpha(COLOR_OUT, '66'),
      borderWidth: 0,
      // mirrored: outbound below the axis (negated; labels use abs)
      data: points.value.map((p) => -toMbps(p.out_bytes).toFixed(3)),
      tension: 0,
      fill: true,
      pointRadius: 0,
      pointHoverRadius: 4,
    },
  ]

  if (hasUnknown.value) {
    datasets.push({
      label: 'Sem direção',
      borderColor: COLOR_UNKNOWN,
      backgroundColor: withAlpha(COLOR_UNKNOWN, '14'),
      borderWidth: 1.5,
      borderDash: [4, 3],
      data: points.value.map((p) => +toMbps(p.unknown_bytes).toFixed(3)),
      tension: 0,
      fill: false,
      pointRadius: 0,
      pointHoverRadius: 4,
    })
  }

  return { labels, datasets }
})

const lineChartOptions = computed(() => ({
  responsive: true,
  maintainAspectRatio: false,
  animation: { duration: 0 },
  interaction: { mode: 'index' as const, intersect: false },
  plugins: {
    legend: { ...mirroredLegend, display: viewMode.value === 'family' || hasDirection.value },
    tooltip: mirroredTooltip,
  },
  scales: {
    x: {
      ticks: { color: '#9ca3af', maxRotation: 0, autoSkip: true, maxTicksLimit: 8 },
      grid: { display: false },
    },
    y: {
      ticks: mirroredYTicks(),
      grid: {
        color: (ctx: any) => (ctx.tick.value === 0 ? '#52525b' : '#374151'),
      },
    },
  },
}))

const peakIn = computed(() =>
  points.value.length ? Math.max(...points.value.map((p) => (hasDirection.value ? p.in_bytes : p.total_bytes))) : 0,
)
const peakOut = computed(() =>
  points.value.length ? Math.max(...points.value.map((p) => p.out_bytes)) : 0,
)
const average = computed(() =>
  points.value.length ? points.value.reduce((s, p) => s + p.total_bytes, 0) / points.value.length : 0,
)
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
  timer = setInterval(() => {
    if (document.visibilityState === 'visible') loadData()
  }, 60000)
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
          <p class="text-zinc-400 mt-1">
            Série temporal com buckets de 1 minuto
            <span v-if="hasDirection"> — entrada acima, saída abaixo do eixo</span>
          </p>
        </div>

        <div class="flex items-center gap-3">
          <div class="flex rounded-lg border border-zinc-800 overflow-hidden text-sm">
            <button
              class="px-3 py-2 transition-colors"
              :class="viewMode === 'direction' ? 'bg-emerald-500/15 text-emerald-400' : 'bg-zinc-900 text-zinc-400 hover:text-zinc-200'"
              @click="viewMode = 'direction'"
            >Direção</button>
            <button
              class="px-3 py-2 transition-colors border-l border-zinc-800"
              :class="viewMode === 'family' ? 'bg-emerald-500/15 text-emerald-400' : 'bg-zinc-900 text-zinc-400 hover:text-zinc-200'"
              @click="viewMode = 'family'"
            >Versão IP</button>
          </div>

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

      <!-- Hint: exporters not reporting direction -->
      <div
        v-if="hasDirection && hasUnknown"
        class="text-xs text-zinc-500 bg-zinc-900/60 border border-zinc-800 rounded-lg px-4 py-2"
      >
        Parte do tráfego aparece como "sem direção": um ou mais exporters não enviam o campo
        flowDirection (IE 61). Habilite-o no roteador para o gráfico espelhado completo.
      </div>

      <!-- Chart -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
        <div v-if="points.length === 0 && !loading" class="flex flex-col items-center justify-center py-20 text-zinc-500 gap-2">
          <TrendingUp class="w-10 h-10" />
          <p>Nenhum dado disponível para este período.</p>
        </div>
        <div v-else class="h-80">
          <Line :data="lineChartData" :options="lineChartOptions" />
        </div>
      </div>

      <!-- Summary stats -->
      <div class="grid gap-4" :class="hasDirection ? 'grid-cols-4' : 'grid-cols-3'">
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <p class="text-xs text-zinc-500 uppercase tracking-wider mb-1 flex items-center gap-1">
            <ArrowDown v-if="hasDirection" class="w-3 h-3" :style="{ color: COLOR_IN }" />
            {{ hasDirection ? 'Pico Entrada' : 'Pico' }}
          </p>
          <p class="text-2xl font-bold text-slate-100">{{ toMbps(peakIn).toFixed(1) }} <span class="text-base font-normal text-zinc-400">Mbps</span></p>
        </div>
        <div v-if="hasDirection" class="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <p class="text-xs text-zinc-500 uppercase tracking-wider mb-1 flex items-center gap-1">
            <ArrowUp class="w-3 h-3" :style="{ color: COLOR_OUT }" />
            Pico Saída
          </p>
          <p class="text-2xl font-bold text-slate-100">{{ toMbps(peakOut).toFixed(1) }} <span class="text-base font-normal text-zinc-400">Mbps</span></p>
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
