<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { Doughnut } from 'vue-chartjs'
import {
  Chart as ChartJS,
  ArcElement,
  Tooltip,
  Legend,
} from 'chart.js'
import { Globe, RefreshCw } from 'lucide-vue-next'
import { formatBytes, formatNumber } from '../utils/format'

ChartJS.register(ArcElement, Tooltip, Legend)

const authStore = useAuthStore()

interface Exporter {
  id: number
  ip_address: string
  name: string
}

interface AsnRow {
  asn: number
  label: string
  total_bytes: number
  total_packets: number
}

const CHART_COLORS = [
  '#10b981', '#3b82f6', '#f59e0b', '#ef4444',
  '#8b5cf6', '#06b6d4', '#f97316', '#ec4899',
  '#6b7280',
]

const exporters = ref<Exporter[]>([])
const selectedDevice = ref<string>('')
const selectedMinutes = ref(60)
const selectedDirection = ref<'src' | 'dst' | 'both'>('both')
const rows = ref<AsnRow[]>([])
const loading = ref(false)

const minuteOptions = [
  { label: 'Últimos 15m', value: 15 },
  { label: 'Última 1h', value: 60 },
  { label: 'Últimas 6h', value: 360 },
  { label: 'Últimas 24h', value: 1440 },
]

const top8 = computed(() => rows.value.slice(0, 8))

const donutChartData = computed(() => {
  const top = top8.value
  const otherBytes = rows.value.slice(8).reduce((s, r) => s + r.total_bytes, 0)
  const labels = [...top.map((r) => r.label), ...(otherBytes > 0 ? ['Other'] : [])]
  const data = [...top.map((r) => r.total_bytes), ...(otherBytes > 0 ? [otherBytes] : [])]
  return {
    labels,
    datasets: [
      {
        backgroundColor: CHART_COLORS.slice(0, labels.length),
        data,
        borderWidth: 0,
      },
    ],
  }
})

const donutOptions = {
  responsive: true,
  maintainAspectRatio: false,
  plugins: {
    legend: { position: 'bottom' as const, labels: { color: '#e5e7eb' } },
    tooltip: {
      callbacks: {
        label: (ctx: any) => ` ${formatBytes(ctx.parsed)}`,
      },
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
      limit: '20',
      direction: selectedDirection.value,
    })
    if (selectedDevice.value) params.set('exporter_ip', selectedDevice.value)
    const res = await fetch(`/api/stats/asn?${params}`, {
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
            <Globe class="w-6 h-6 text-emerald-500" />
            ASN Traffic
          </h1>
          <p class="text-zinc-400 mt-1">Tráfego por Autonomous System</p>
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

          <select
            v-model="selectedDirection"
            @change="loadData"
            class="appearance-none pl-3 pr-8 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer"
          >
            <option value="both">Ambos</option>
            <option value="src">Origem</option>
            <option value="dst">Destino</option>
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

      <!-- Doughnut + Table -->
      <div v-if="rows.length === 0 && !loading" class="flex flex-col items-center justify-center py-20 text-zinc-500 gap-3">
        <Globe class="w-12 h-12" />
        <p class="text-center max-w-sm">
          Sem dados de ASN disponíveis. Verifique se o equipamento está enviando informações de BGP no template NetFlow/IPFIX.
        </p>
      </div>

      <div v-else class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Doughnut chart -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 flex flex-col items-center">
          <h2 class="text-base font-semibold text-zinc-300 mb-4 self-start">Top ASNs</h2>
          <div class="h-64 w-full">
            <Doughnut v-if="top8.length > 0" :data="donutChartData" :options="donutOptions" />
          </div>
        </div>

        <!-- Table -->
        <div class="lg:col-span-2 bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
          <div v-if="loading && rows.length === 0" class="flex items-center justify-center py-16 text-zinc-500">
            <RefreshCw class="w-5 h-5 animate-spin mr-2" /> Carregando…
          </div>
          <table v-else class="w-full text-sm">
            <thead>
              <tr class="border-b border-zinc-800 text-zinc-400 text-left">
                <th class="px-6 py-3 font-medium">ASN</th>
                <th class="px-6 py-3 font-medium">Label</th>
                <th class="px-6 py-3 font-medium text-right">Tráfego</th>
                <th class="px-6 py-3 font-medium text-right">Pacotes</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in rows"
                :key="row.asn"
                class="border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors"
              >
                <td class="px-6 py-3 font-mono text-zinc-400">{{ row.asn }}</td>
                <td class="px-6 py-3 text-slate-200 font-medium">{{ row.label }}</td>
                <td class="px-6 py-3 text-right text-emerald-400 font-medium">{{ formatBytes(row.total_bytes) }}</td>
                <td class="px-6 py-3 text-right text-zinc-300">{{ formatNumber(row.total_packets) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </div>
</template>
