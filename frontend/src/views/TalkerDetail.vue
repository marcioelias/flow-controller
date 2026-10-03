<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { Line } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Tooltip,
  Legend,
  Filler,
} from 'chart.js'
import { ArrowLeft, ArrowDown, ArrowUp, ArrowUpDown, RefreshCw, Search } from 'lucide-vue-next'
import { bytesToBps, formatBps, formatBytes, formatNumber } from '../utils/format'
import {
  COLOR_IN,
  COLOR_OUT,
  withAlpha,
  mirroredLegend,
  loadMirrorFlip,
  saveMirrorFlip,
} from '../lib/chartTheme'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Legend, Filler)

const authStore = useAuthStore()
const route = useRoute()
const router = useRouter()

interface Exporter {
  id: number
  ip_address: string
  name: string
}

interface Conversation {
  peer: string
  peer_asn: number
  protocol: number
  port: number
  up_bytes: number
  down_bytes: number
  packets: number
  first_seen: number
  last_seen: number
}

interface PortRow {
  protocol: number
  port: number
  up_bytes: number
  down_bytes: number
  peers: number
}

interface TalkerDetail {
  ip: string
  bucket_secs: number
  from: number
  to: number
  up_bytes: number
  down_bytes: number
  up_p95_bps: number
  down_p95_bps: number
  series: { t: number; up_bps: number; down_bps: number }[]
  conversations: Conversation[]
  ports: PortRow[]
}

const ip = computed(() => String(route.params.ip))
const exporters = ref<Exporter[]>([])
const selectedDevice = ref<string>('')
const selectedMinutes = ref(5)
const detail = ref<TalkerDetail | null>(null)
const loading = ref(false)
const paywalled = ref(false)
const invalidIp = ref(false)
const flip = ref(loadMirrorFlip())

const minuteOptions = [
  { label: 'Últimos 5m', value: 5 },
  { label: 'Últimos 15m', value: 15 },
  { label: 'Última 1h', value: 60 },
  { label: 'Últimas 6h', value: 360 },
  { label: 'Últimas 24h', value: 1440 },
]

const PROTOCOLS: Record<number, string> = { 1: 'ICMP', 6: 'TCP', 17: 'UDP', 47: 'GRE', 50: 'ESP', 58: 'ICMPv6' }
function protoPort(protocol: number, port: number): string {
  const name = PROTOCOLS[protocol] ?? `IP/${protocol}`
  return protocol === 6 || protocol === 17 ? `${name}/${port}` : name
}

const windowSecs = computed(() => (detail.value ? detail.value.to - detail.value.from : 0))
function rate(bytes: number): string {
  return formatBps(bytesToBps(bytes, windowSecs.value))
}

function clock(sec: number): string {
  return new Date(sec * 1000).toTimeString().slice(0, 8)
}

// Série vem esparsa: preenche zeros de `from` a `to` no passo do bucket
const filledSeries = computed(() => {
  const d = detail.value
  if (!d) return { labels: [] as string[], up: [] as number[], down: [] as number[] }
  const byT = new Map(d.series.map((p) => [p.t, p]))
  const step = d.bucket_secs
  const labels: string[] = []
  const up: number[] = []
  const down: number[] = []
  for (let t = Math.floor(d.from / step) * step; t < d.to; t += step) {
    const p = byT.get(t)
    labels.push(step >= 60 ? clock(t).slice(0, 5) : clock(t))
    up.push(p?.up_bps ?? 0)
    down.push(p?.down_bps ?? 0)
  }
  return { labels, up, down }
})

const chartData = computed(() => {
  const { labels, up, down } = filledSeries.value
  const area = (label: string, color: string, data: number[]) => ({
    label,
    borderColor: color,
    backgroundColor: withAlpha(color, '73'),
    borderWidth: 0,
    data,
    tension: 0,
    fill: 'origin' as const,
    pointRadius: 0,
    pointHoverRadius: 4,
  })
  const pos = (d: number[]) => (flip.value ? d.map((v) => -v) : d)
  const neg = (d: number[]) => (flip.value ? d : d.map((v) => -v))
  return {
    labels,
    datasets: [area('Download', COLOR_IN, pos(down)), area('Upload', COLOR_OUT, neg(up))],
  }
})

const chartOptions = {
  responsive: true,
  maintainAspectRatio: false,
  animation: { duration: 0 },
  interaction: { mode: 'index' as const, intersect: false },
  plugins: {
    legend: mirroredLegend,
    tooltip: {
      callbacks: {
        label: (ctx: any) => ` ${ctx.dataset.label}: ${formatBps(Math.abs(ctx.parsed.y))}`,
      },
    },
  },
  scales: {
    x: {
      ticks: { color: '#6b7280', maxRotation: 0, autoSkip: true, maxTicksLimit: 6 },
      grid: { display: false },
    },
    y: {
      ticks: { color: '#6b7280', callback: (v: any) => formatBps(Math.abs(Number(v))) },
      grid: { color: (ctx: any) => (ctx.tick.value === 0 ? '#52525b' : '#27272a') },
    },
  },
}

function toggleFlip() {
  flip.value = !flip.value
  saveMirrorFlip(flip.value)
}

async function loadExporters() {
  try {
    const res = await fetch('/api/exporters/enabled', { headers: authStore.getAuthHeaders() })
    if (res.ok) exporters.value = await res.json()
  } catch {}
}

async function loadData() {
  loading.value = true
  try {
    const params = new URLSearchParams({ ip: ip.value, minutes: String(selectedMinutes.value) })
    if (selectedDevice.value) params.set('exporter_ip', selectedDevice.value)
    const res = await fetch(`/api/stats/talker?${params}`, { headers: authStore.getAuthHeaders() })
    paywalled.value = res.status === 402
    invalidIp.value = res.status === 400
    if (res.ok) detail.value = await res.json()
  } catch {
    detail.value = null
  } finally {
    loading.value = false
  }
}

const searchIp = ref('')
function openIp(target: string) {
  const v = target.trim()
  if (v && v !== ip.value) router.push(`/talkers/${encodeURIComponent(v)}`)
}

// Janelas curtas parecem "ao vivo"; longas não precisam martelar o ClickHouse
let timer: ReturnType<typeof setInterval> | null = null
function restartTimer() {
  if (timer) clearInterval(timer)
  timer = setInterval(loadData, selectedMinutes.value <= 15 ? 5_000 : 60_000)
}

function onWindowChange() {
  loadData()
  restartTimer()
}

watch(ip, () => {
  detail.value = null
  loadData()
})

onMounted(() => {
  loadExporters()
  loadData()
  restartTimer()
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-7xl mx-auto space-y-6">
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
      <header class="flex flex-wrap justify-between items-center gap-4 pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold tracking-tight flex items-center gap-3">
            <button @click="router.back()" class="text-zinc-500 hover:text-emerald-400 transition-colors" title="Voltar">
              <ArrowLeft class="w-6 h-6" />
            </button>
            <span class="font-mono">{{ ip }}</span>
          </h1>
          <p class="text-zinc-400 mt-1">Análise do talker — upload (como origem) e download (como destino)</p>
        </div>

        <div class="flex items-center gap-3">
          <form class="relative" @submit.prevent="openIp(searchIp)">
            <Search class="w-4 h-4 text-zinc-500 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              v-model="searchIp"
              placeholder="Analisar outro IP"
              class="pl-9 pr-3 py-2 w-48 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 font-mono focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
            />
          </form>

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
            @change="onWindowChange"
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

      <div v-if="invalidIp" class="flex flex-col items-center justify-center py-20 text-zinc-500 gap-3">
        <p>"{{ ip }}" não é um endereço IPv4 ou IPv6 válido.</p>
      </div>

      <div v-else-if="loading && !detail" class="flex items-center justify-center py-20 text-zinc-500">
        <RefreshCw class="w-5 h-5 animate-spin mr-2" /> Carregando…
      </div>

      <template v-else-if="detail">
        <!-- Tiles -->
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Download p95</span>
              <ArrowDown class="w-3.5 h-3.5" :style="{ color: COLOR_IN }" />
            </div>
            <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(detail.down_p95_bps) }}</p>
            <p class="text-[11px] text-zinc-500 mt-0.5">buckets de {{ detail.bucket_secs }}s</p>
          </div>
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
            <div class="flex items-center justify-between mb-2">
              <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Upload p95</span>
              <ArrowUp class="w-3.5 h-3.5" :style="{ color: COLOR_OUT }" />
            </div>
            <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight">{{ formatBps(detail.up_p95_bps) }}</p>
            <p class="text-[11px] text-zinc-500 mt-0.5">buckets de {{ detail.bucket_secs }}s</p>
          </div>
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Volume download</span>
            <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight mt-2">{{ formatBytes(detail.down_bytes) }}</p>
            <p class="text-[11px] text-zinc-500 mt-0.5">na janela</p>
          </div>
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
            <span class="text-[11px] font-medium text-zinc-500 uppercase tracking-wider">Volume upload</span>
            <p class="text-xl font-bold text-slate-100 tabular-nums leading-tight mt-2">{{ formatBytes(detail.up_bytes) }}</p>
            <p class="text-[11px] text-zinc-500 mt-0.5">na janela</p>
          </div>
        </div>

        <!-- Chart -->
        <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-base font-semibold text-slate-200">Tráfego do IP</h2>
              <p class="text-xs text-zinc-500 mt-0.5">
                Os últimos ~60s preenchem retroativamente conforme os flows expiram no roteador
              </p>
            </div>
            <button
              @click="toggleFlip"
              class="p-1.5 rounded-md border border-zinc-800 text-zinc-400 hover:text-emerald-400 transition-colors"
              title="Inverter lados"
            >
              <ArrowUpDown class="w-4 h-4" />
            </button>
          </div>
          <div class="h-64">
            <Line :data="chartData" :options="chartOptions" />
          </div>
        </div>

        <div v-if="detail.conversations.length === 0" class="flex flex-col items-center justify-center py-12 text-zinc-500">
          <p>Nenhum tráfego deste IP na janela.</p>
        </div>

        <div v-else class="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <!-- Conversations -->
          <div class="lg:col-span-2 bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
            <h2 class="px-6 pt-5 pb-3 text-base font-semibold text-slate-200">Conversas</h2>
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="border-b border-zinc-800 text-zinc-400 text-left">
                    <th class="px-4 py-2 font-medium">Peer</th>
                    <th class="px-4 py-2 font-medium">ASN</th>
                    <th class="px-4 py-2 font-medium">Serviço</th>
                    <th class="px-4 py-2 font-medium text-right">↓ Média</th>
                    <th class="px-4 py-2 font-medium text-right">↑ Média</th>
                    <th class="px-4 py-2 font-medium text-right">Pacotes</th>
                    <th class="px-4 py-2 font-medium text-right">Último</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="c in detail.conversations"
                    :key="`${c.peer}-${c.protocol}-${c.port}`"
                    class="border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors"
                  >
                    <td class="px-4 py-2 font-mono">
                      <router-link :to="`/talkers/${encodeURIComponent(c.peer)}`" class="text-slate-200 hover:text-emerald-400">
                        {{ c.peer }}
                      </router-link>
                    </td>
                    <td class="px-4 py-2 font-mono text-zinc-500">{{ c.peer_asn ? `AS${c.peer_asn}` : '—' }}</td>
                    <td class="px-4 py-2 font-mono text-zinc-300">{{ protoPort(c.protocol, c.port) }}</td>
                    <td class="px-4 py-2 text-right tabular-nums" :style="{ color: COLOR_IN }">{{ rate(c.down_bytes) }}</td>
                    <td class="px-4 py-2 text-right tabular-nums" :style="{ color: COLOR_OUT }">{{ rate(c.up_bytes) }}</td>
                    <td class="px-4 py-2 text-right tabular-nums text-zinc-300">{{ formatNumber(c.packets) }}</td>
                    <td class="px-4 py-2 text-right tabular-nums text-zinc-500">{{ clock(c.last_seen) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- Ports -->
          <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
            <h2 class="px-6 pt-5 pb-3 text-base font-semibold text-slate-200">Portas</h2>
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b border-zinc-800 text-zinc-400 text-left">
                  <th class="px-4 py-2 font-medium">Serviço</th>
                  <th class="px-4 py-2 font-medium text-right">↓</th>
                  <th class="px-4 py-2 font-medium text-right">↑</th>
                  <th class="px-4 py-2 font-medium text-right">Peers</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="p in detail.ports"
                  :key="`${p.protocol}-${p.port}`"
                  class="border-b border-zinc-800/50"
                >
                  <td class="px-4 py-2 font-mono text-zinc-300">{{ protoPort(p.protocol, p.port) }}</td>
                  <td class="px-4 py-2 text-right tabular-nums" :style="{ color: COLOR_IN }">{{ rate(p.down_bytes) }}</td>
                  <td class="px-4 py-2 text-right tabular-nums" :style="{ color: COLOR_OUT }">{{ rate(p.up_bytes) }}</td>
                  <td class="px-4 py-2 text-right tabular-nums text-zinc-400">{{ formatNumber(p.peers) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </template>
      </template>
    </div>
  </div>
</template>
