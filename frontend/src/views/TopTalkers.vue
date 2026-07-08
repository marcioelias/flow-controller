<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { Bar } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Title,
  Tooltip,
  Legend,
} from 'chart.js'
import { Users, RefreshCw } from 'lucide-vue-next'
import { formatBytes, formatNumber } from '../utils/format'

ChartJS.register(CategoryScale, LinearScale, BarElement, Title, Tooltip, Legend)

const authStore = useAuthStore()

interface Exporter {
  id: number
  ip_address: string
  name: string
}

interface TopTalker {
  src_ip: string
  total_bytes: number
  total_packets: number
  flow_count: number
}

const exporters = ref<Exporter[]>([])
const selectedDevice = ref<string>('')
const selectedMinutes = ref(5)
const rows = ref<TopTalker[]>([])
const loading = ref(false)

const minuteOptions = [
  { label: 'Últimos 5m', value: 5 },
  { label: 'Últimos 15m', value: 15 },
  { label: 'Última 1h', value: 60 },
  { label: 'Últimas 6h', value: 360 },
  { label: 'Últimas 24h', value: 1440 },
]

const top10 = computed(() => rows.value.slice(0, 10))

const barChartData = computed(() => ({
  labels: top10.value.map((r) => r.src_ip),
  datasets: [
    {
      label: 'Tráfego (bytes)',
      backgroundColor: '#10b981',
      data: top10.value.map((r) => r.total_bytes),
      borderRadius: 4,
    },
  ],
}))

const barChartOptions = {
  indexAxis: 'y' as const,
  responsive: true,
  maintainAspectRatio: false,
  animation: { duration: 0 },
  plugins: {
    legend: { display: false },
    tooltip: {
      callbacks: {
        label: (ctx: any) => ' ' + formatBytes(ctx.parsed.x),
      },
    },
  },
  scales: {
    x: {
      ticks: {
        color: '#9ca3af',
        callback: (v: any) => formatBytes(Number(v)),
      },
      grid: { color: '#374151' },
    },
    y: {
      ticks: { color: '#e5e7eb' },
      grid: { display: false },
    },
  },
}

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
    const params = new URLSearchParams({
      minutes: String(selectedMinutes.value),
      limit: '50',
    })
    if (selectedDevice.value) params.set('exporter_ip', selectedDevice.value)
    const res = await fetch(`/api/stats/top-talkers?${params}`, {
      headers: authStore.getAuthHeaders(),
    })
    if (res.ok) rows.value = await res.json()
  } catch {
    rows.value = []
  } finally {
    loading.value = false
  }
}

let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  loadExporters()
  loadData()
  timer = setInterval(loadData, 30000)
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
            <Users class="w-6 h-6 text-emerald-500" />
            Top Talkers
          </h1>
          <p class="text-zinc-400 mt-1">Maiores emissores de tráfego na rede</p>
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
            v-model="selectedMinutes"
            @change="loadData"
            class="appearance-none pl-3 pr-8 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer"
          >
            <option v-for="opt in minuteOptions" :key="opt.value" :value="opt.value">
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

      <!-- Bar chart -->
      <div
        v-if="top10.length > 0"
        class="bg-zinc-900 border border-zinc-800 rounded-xl p-6"
      >
        <h2 class="text-base font-semibold text-zinc-300 mb-4">Top 10 por volume</h2>
        <div :style="{ height: Math.max(200, top10.length * 36) + 'px' }">
          <Bar :data="barChartData" :options="barChartOptions" />
        </div>
      </div>

      <!-- Table -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div v-if="loading && rows.length === 0" class="flex items-center justify-center py-16 text-zinc-500">
          <RefreshCw class="w-5 h-5 animate-spin mr-2" /> Carregando…
        </div>
        <div v-else-if="rows.length === 0" class="flex flex-col items-center justify-center py-16 text-zinc-500 gap-2">
          <Users class="w-10 h-10" />
          <p>Nenhum dado disponível para este período.</p>
        </div>
        <table v-else class="w-full text-sm">
          <thead>
            <tr class="border-b border-zinc-800 text-zinc-400 text-left">
              <th class="px-6 py-3 font-medium w-12">#</th>
              <th class="px-6 py-3 font-medium">IP Origem</th>
              <th class="px-6 py-3 font-medium text-right">Tráfego</th>
              <th class="px-6 py-3 font-medium text-right">Pacotes</th>
              <th class="px-6 py-3 font-medium text-right">Flows</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(row, i) in rows"
              :key="row.src_ip"
              class="border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors"
            >
              <td class="px-6 py-3 text-zinc-500 font-mono">{{ i + 1 }}</td>
              <td class="px-6 py-3 font-mono text-slate-200">{{ row.src_ip }}</td>
              <td class="px-6 py-3 text-right text-emerald-400 font-medium">{{ formatBytes(row.total_bytes) }}</td>
              <td class="px-6 py-3 text-right text-zinc-300">{{ formatNumber(row.total_packets) }}</td>
              <td class="px-6 py-3 text-right text-zinc-400">{{ formatNumber(row.flow_count) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
