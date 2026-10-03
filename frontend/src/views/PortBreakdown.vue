<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import DeviceSelect from '../components/DeviceSelect.vue'
import { applyDevice, DEFAULT_DEVICE, type DeviceValue } from '../utils/device'
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
import { Plug, RefreshCw } from 'lucide-vue-next'
import { formatBps } from '../utils/format'
import { useSort } from '../composables/useSort'
import SortTh from '../components/SortTh.vue'

ChartJS.register(CategoryScale, LinearScale, BarElement, Title, Tooltip, Legend)

const authStore = useAuthStore()

interface PortRow {
  port: number
  service: string
  p95_bps: number
  avg_bps: number
  total_bytes: number
  total_packets: number
  share_pct: number
}

const selectedDevice = ref<DeviceValue>(DEFAULT_DEVICE)
const selectedMinutes = ref(5)
const rows = ref<PortRow[]>([])
const loading = ref(false)
const paywalled = ref(false)

const minuteOptions = [
  { label: 'Últimos 5m', value: 5 },
  { label: 'Últimos 15m', value: 15 },
  { label: 'Última 1h', value: 60 },
]

const top15 = computed(() => rows.value.slice(0, 15))
const { sorted, sort } = useSort(() => rows.value, 'p95_bps')

const barChartData = computed(() => ({
  labels: top15.value.map((r) => `${r.port} (${r.service})`),
  datasets: [
    {
      label: 'p95',
      backgroundColor: top15.value.map((r) =>
        r.service !== 'Other' ? '#10b981' : '#6b7280',
      ),
      data: top15.value.map((r) => r.p95_bps),
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
        label: (ctx: any) => ' p95 ' + formatBps(ctx.parsed.x),
      },
    },
  },
  scales: {
    x: {
      ticks: {
        color: '#9ca3af',
        callback: (v: any) => formatBps(Number(v)),
      },
      grid: { color: '#374151' },
    },
    y: {
      ticks: { color: '#e5e7eb' },
      grid: { display: false },
    },
  },
}

async function loadData() {
  loading.value = true
  try {
    const params = new URLSearchParams({
      minutes: String(selectedMinutes.value),
      limit: '20',
    })
    applyDevice(params, selectedDevice.value)
    const res = await fetch(`/api/stats/ports?${params}`, {
      headers: authStore.getAuthHeaders(),
    })
    paywalled.value = res.status === 402
    if (res.ok) rows.value = await res.json()
  } catch {
    rows.value = []
  } finally {
    loading.value = false
  }
}

let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
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
      <!-- Paywall: excedente sustentado da licença (HTTP 402) -->
      <div v-if="paywalled" class="bg-amber-500/5 border border-amber-500/30 rounded-xl p-10 text-center space-y-3">
        <p class="text-amber-400 font-semibold text-lg">Limite da licença excedido há mais de 7 dias</p>
        <p class="text-sm text-zinc-400 max-w-lg mx-auto">
          A coleta continua completa — nenhum dado foi perdido. As views analíticas ficam
          bloqueadas até aplicar uma licença adequada ao seu tráfego ou o volume voltar ao limite.
        </p>
        <router-link to="/license" class="inline-block mt-2 px-4 py-2 rounded-lg bg-amber-500/15 text-amber-400 border border-amber-500/30 hover:bg-amber-500/25 transition-colors text-sm font-medium">
          Aplicar licença
        </router-link>
      </div>

      <template v-if="!paywalled">
      <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold tracking-tight flex items-center gap-2">
            <Plug class="w-6 h-6 text-emerald-500" />
            Aplicações / Portas
          </h1>
          <p class="text-zinc-400 mt-1">Taxa por porta de serviço — os dois sentidos da conversa somados</p>
        </div>

        <div class="flex items-center gap-3">
          <DeviceSelect v-model="selectedDevice" @change="loadData" />

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
        v-if="top15.length > 0"
        class="bg-zinc-900 border border-zinc-800 rounded-xl p-6"
      >
        <div class="flex items-center gap-4 mb-4">
          <h2 class="text-base font-semibold text-zinc-300">Top 15 portas (p95)</h2>
          <div class="flex items-center gap-3 text-xs text-zinc-500">
            <span class="flex items-center gap-1.5"><span class="w-3 h-3 rounded-sm bg-emerald-500 inline-block"></span>Serviço conhecido</span>
            <span class="flex items-center gap-1.5"><span class="w-3 h-3 rounded-sm bg-zinc-500 inline-block"></span>Outro</span>
          </div>
        </div>
        <div :style="{ height: Math.max(200, top15.length * 36) + 'px' }">
          <Bar :data="barChartData" :options="barChartOptions" />
        </div>
      </div>

      <!-- Table -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div v-if="loading && rows.length === 0" class="flex items-center justify-center py-16 text-zinc-500">
          <RefreshCw class="w-5 h-5 animate-spin mr-2" /> Carregando…
        </div>
        <div v-else-if="rows.length === 0" class="flex flex-col items-center justify-center py-16 text-zinc-500 gap-2">
          <Plug class="w-10 h-10" />
          <p>Nenhum dado disponível para este período.</p>
        </div>
        <table v-else class="w-full text-sm">
          <thead>
            <tr class="border-b border-zinc-800 text-zinc-400 text-left">
              <SortTh :sort="sort" k="port" class="px-6 py-3">Porta</SortTh>
              <SortTh :sort="sort" k="service" class="px-6 py-3">Serviço</SortTh>
              <SortTh :sort="sort" k="p95_bps" align="right" class="px-6 py-3">95º perc.</SortTh>
              <SortTh :sort="sort" k="avg_bps" align="right" class="px-6 py-3">Média</SortTh>
              <SortTh :sort="sort" k="share_pct" align="right" class="px-6 py-3">% do total</SortTh>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in sorted"
              :key="row.port"
              class="border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors"
            >
              <td class="px-6 py-3 font-mono text-zinc-400">{{ row.port }}</td>
              <td class="px-6 py-3">
                <span
                  :class="row.service !== 'Other' ? 'text-emerald-400' : 'text-zinc-400'"
                  class="font-medium"
                >{{ row.service }}</span>
              </td>
              <td class="px-6 py-3 text-right text-emerald-400 font-medium tabular-nums">{{ formatBps(row.p95_bps) }}</td>
              <td class="px-6 py-3 text-right text-slate-200 tabular-nums">{{ formatBps(row.avg_bps) }}</td>
              <td class="px-6 py-3 text-right text-zinc-400 tabular-nums">{{ row.share_pct.toFixed(1) }}%</td>
            </tr>
          </tbody>
        </table>
      </div>
      </template>
    </div>
  </div>
</template>
