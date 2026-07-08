<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { formatBytes } from '../utils/format'
import { Cpu, MemoryStick, HardDrive, Server, Activity } from 'lucide-vue-next'

const authStore = useAuthStore()

interface HealthData {
  cpu: {
    cores: number
    usage_percent: number[]
    usage_total_percent: number
  }
  memory: {
    total_bytes: number
    used_bytes: number
    available_bytes: number
    used_percent: number
  }
  disk: {
    total_bytes: number
    used_bytes: number
    available_bytes: number
    used_percent: number
    reads_per_sec: number
    writes_per_sec: number
  }
  system: {
    uptime_seconds: number
    uptime_human: string
  }
  process: {
    rss_bytes: number
    cpu_percent: number
    threads: number
    uptime_seconds: number
  }
  collector: {
    flows_received: number
    flows_decoded: number
    packets_dropped: number
    template_cache_size: number
  }
}

const data = ref<HealthData | null>(null)
const loading = ref(false)
const error = ref('')
let intervalId: ReturnType<typeof setInterval> | null = null

function barColor(pct: number): string {
  if (pct >= 80) return 'bg-red-500'
  if (pct >= 60) return 'bg-amber-400'
  return 'bg-emerald-500'
}

function barColorClass(pct: number): string {
  if (pct >= 80) return 'text-red-400'
  if (pct >= 60) return 'text-amber-400'
  return 'text-emerald-400'
}

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h ${m}m`
  return `${h}h ${m}m`
}

function formatNumber(n: number): string {
  return new Intl.NumberFormat('pt-BR').format(n)
}

async function fetchHealth() {
  loading.value = true
  error.value = ''
  try {
    const res = await fetch('/api/system/health', {
      headers: { Authorization: `Bearer ${authStore.token}` },
    })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    data.value = await res.json()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : 'Erro ao carregar métricas'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchHealth()
  intervalId = setInterval(fetchHealth, 5000)
})

onUnmounted(() => {
  if (intervalId !== null) clearInterval(intervalId)
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-6xl mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Server class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">Servidor</h1>
      </div>
      <div v-if="loading && !data" class="flex items-center gap-2 text-sm text-zinc-400">
        <svg class="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/>
        </svg>
        Carregando...
      </div>
      <div v-else-if="data" class="text-xs text-zinc-500">Atualiza a cada 5s</div>
    </div>

    <div v-if="error" class="p-4 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
      {{ error }}
    </div>

    <template v-if="data">
      <!-- CPU card — full width -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
        <div class="flex items-center justify-between mb-4">
          <div class="flex items-center gap-2">
            <Cpu class="w-5 h-5 text-emerald-500" />
            <span class="font-semibold text-slate-200">CPU</span>
          </div>
          <span :class="barColorClass(data.cpu.usage_total_percent)" class="text-lg font-bold font-mono">
            {{ data.cpu.usage_total_percent.toFixed(1) }}%
          </span>
        </div>

        <!-- Per-core grid -->
        <div class="grid gap-2" :class="data.cpu.cores > 8 ? 'grid-cols-4' : 'grid-cols-2 sm:grid-cols-4'">
          <div v-for="(usage, i) in data.cpu.usage_percent" :key="i" class="space-y-1">
            <div class="flex justify-between text-xs text-zinc-400">
              <span>Core {{ i }}</span>
              <span :class="barColorClass(usage)">{{ usage.toFixed(1) }}%</span>
            </div>
            <div class="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
              <div
                :class="barColor(usage)"
                class="h-full rounded-full transition-all duration-500"
                :style="{ width: Math.min(100, usage) + '%' }"
              />
            </div>
          </div>
        </div>

        <!-- Total bar -->
        <div class="mt-4 space-y-1">
          <div class="flex justify-between text-xs text-zinc-500">
            <span>Total ({{ data.cpu.cores }} cores)</span>
          </div>
          <div class="h-2.5 bg-zinc-800 rounded-full overflow-hidden">
            <div
              :class="barColor(data.cpu.usage_total_percent)"
              class="h-full rounded-full transition-all duration-500"
              :style="{ width: Math.min(100, data.cpu.usage_total_percent) + '%' }"
            />
          </div>
        </div>
      </div>

      <!-- Memory + Disk row -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6">

        <!-- Memory -->
        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2">
              <MemoryStick class="w-5 h-5 text-sky-400" />
              <span class="font-semibold text-slate-200">Memória</span>
            </div>
            <span :class="barColorClass(data.memory.used_percent)" class="text-lg font-bold font-mono">
              {{ data.memory.used_percent.toFixed(1) }}%
            </span>
          </div>
          <div class="h-3 bg-zinc-800 rounded-full overflow-hidden mb-4">
            <div
              :class="barColor(data.memory.used_percent)"
              class="h-full rounded-full transition-all duration-500"
              :style="{ width: Math.min(100, data.memory.used_percent) + '%' }"
            />
          </div>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between">
              <dt class="text-zinc-400">Usada</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.memory.used_bytes) }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Disponível</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.memory.available_bytes) }}</dd>
            </div>
            <div class="flex justify-between border-t border-zinc-800 pt-2">
              <dt class="text-zinc-400">Total</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.memory.total_bytes) }}</dd>
            </div>
          </dl>
        </div>

        <!-- Disk -->
        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2">
              <HardDrive class="w-5 h-5 text-violet-400" />
              <span class="font-semibold text-slate-200">Disco (/)</span>
            </div>
            <span :class="barColorClass(data.disk.used_percent)" class="text-lg font-bold font-mono">
              {{ data.disk.used_percent.toFixed(1) }}%
            </span>
          </div>
          <div class="h-3 bg-zinc-800 rounded-full overflow-hidden mb-4">
            <div
              :class="barColor(data.disk.used_percent)"
              class="h-full rounded-full transition-all duration-500"
              :style="{ width: Math.min(100, data.disk.used_percent) + '%' }"
            />
          </div>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between">
              <dt class="text-zinc-400">Usado</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.disk.used_bytes) }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Disponível</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.disk.available_bytes) }}</dd>
            </div>
            <div class="flex justify-between border-t border-zinc-800 pt-2">
              <dt class="text-zinc-400">Total</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.disk.total_bytes) }}</dd>
            </div>
            <div class="flex justify-between border-t border-zinc-800 pt-2">
              <dt class="text-zinc-400">
                Leituras/s
                <span class="text-zinc-600 text-xs ml-1" title="Pode não refletir disco físico em containers">ⓘ</span>
              </dt>
              <dd class="text-slate-200 font-mono">{{ data.disk.reads_per_sec.toFixed(0) }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Escritas/s</dt>
              <dd class="text-slate-200 font-mono">{{ data.disk.writes_per_sec.toFixed(0) }}</dd>
            </div>
          </dl>
        </div>
      </div>

      <!-- System + Process row -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6">

        <!-- System -->
        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
          <div class="flex items-center gap-2 mb-4">
            <Server class="w-5 h-5 text-zinc-400" />
            <span class="font-semibold text-slate-200">Sistema</span>
          </div>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between">
              <dt class="text-zinc-400">Uptime</dt>
              <dd class="text-slate-200 font-mono">{{ data.system.uptime_human }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Cores disponíveis</dt>
              <dd class="text-slate-200 font-mono">{{ data.cpu.cores }}</dd>
            </div>
          </dl>
        </div>

        <!-- Process -->
        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
          <div class="flex items-center gap-2 mb-4">
            <Activity class="w-5 h-5 text-emerald-400" />
            <span class="font-semibold text-slate-200">Processo Collector</span>
          </div>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between">
              <dt class="text-zinc-400">CPU</dt>
              <dd :class="barColorClass(data.process.cpu_percent)" class="font-mono">
                {{ data.process.cpu_percent.toFixed(1) }}%
              </dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Memória (RSS)</dt>
              <dd class="text-slate-200 font-mono">{{ formatBytes(data.process.rss_bytes) }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Threads</dt>
              <dd class="text-slate-200 font-mono">{{ data.process.threads }}</dd>
            </div>
            <div class="flex justify-between">
              <dt class="text-zinc-400">Uptime</dt>
              <dd class="text-slate-200 font-mono">{{ formatUptime(data.process.uptime_seconds) }}</dd>
            </div>
          </dl>
        </div>
      </div>

      <!-- Collector metrics — full width -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
        <div class="flex items-center gap-2 mb-4">
          <Activity class="w-5 h-5 text-emerald-500" />
          <span class="font-semibold text-slate-200">Flow Collector — Métricas</span>
        </div>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div class="text-center">
            <p class="text-2xl font-bold font-mono text-slate-100">
              {{ formatNumber(data.collector.flows_received) }}
            </p>
            <p class="text-xs text-zinc-400 mt-1">Recebidos</p>
          </div>
          <div class="text-center">
            <p class="text-2xl font-bold font-mono text-emerald-400">
              {{ formatNumber(data.collector.flows_decoded) }}
            </p>
            <p class="text-xs text-zinc-400 mt-1">Decodificados</p>
          </div>
          <div class="text-center">
            <p class="text-2xl font-bold font-mono" :class="data.collector.packets_dropped > 0 ? 'text-red-400' : 'text-slate-400'">
              {{ formatNumber(data.collector.packets_dropped) }}
            </p>
            <p class="text-xs text-zinc-400 mt-1">Descartados</p>
          </div>
          <div class="text-center">
            <p class="text-2xl font-bold font-mono text-sky-400">
              {{ formatNumber(data.collector.template_cache_size) }}
            </p>
            <p class="text-xs text-zinc-400 mt-1">Templates em cache</p>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
