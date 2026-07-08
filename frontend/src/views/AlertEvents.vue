<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAlertsStore } from '../stores/alerts'
import { Bell, Trash2, RefreshCw } from 'lucide-vue-next'

const store = useAlertsStore()

const severityFilter = ref('')
const notifiedFilter = ref('')
const page = ref(0)
const perPage = 50
const confirmClear = ref(false)

let refreshTimer: ReturnType<typeof setInterval> | null = null

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

function formatBytes(n: number | null) {
  if (n == null) return '—'
  const mb = Math.floor(n / (60 * 125_000))
  return `${mb} Mbps`
}

function severityClass(s: string) {
  return s === 'critical'
    ? 'bg-red-500/10 text-red-400 border-red-500/20'
    : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
}

const totalPages = computed(() => Math.ceil(store.eventsTotal / perPage))

onMounted(async () => {
  await load()
  refreshTimer = setInterval(load, 30_000)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
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
              <th class="px-4 py-3">Hora</th>
              <th class="px-4 py-3">Severidade</th>
              <th class="px-4 py-3">Tipo</th>
              <th class="px-4 py-3">Exporter</th>
              <th class="px-4 py-3">IP</th>
              <th class="px-4 py-3">Mensagem</th>
              <th class="px-4 py-3">BGP</th>
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
              class="hover:bg-zinc-800/30 transition-colors"
              :class="ev.notified ? 'opacity-60' : ''"
            >
              <td class="px-4 py-2.5 text-zinc-500 text-xs whitespace-nowrap">{{ formatDate(ev.created_at) }}</td>
              <td class="px-4 py-2.5">
                <span class="px-2 py-0.5 rounded-full border text-xs font-semibold uppercase" :class="severityClass(ev.severity)">
                  {{ ev.severity }}
                </span>
              </td>
              <td class="px-4 py-2.5">
                <span class="text-xs text-zinc-300 font-mono">{{ ev.alert_type }}</span>
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
                  <span v-if="ev.alert_type === 'upload_inversion'">{{ formatBytes(ev.upload_bytes) }}</span>
                  <span v-else class="text-zinc-700">—</span>
                </td>
                <td class="px-4 py-2.5 text-xs text-zinc-400">
                  <span v-if="ev.alert_type === 'upload_inversion'">{{ formatBytes(ev.download_bytes) }}</span>
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
  </div>
</template>
