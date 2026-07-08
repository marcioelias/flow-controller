<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useAiStore } from '../stores/ai'
import { Brain, RefreshCw, AlertTriangle, CheckCircle2, Loader2 } from 'lucide-vue-next'

const store = useAiStore()
const page = ref(0)
const perPage = 50
let timer: ReturnType<typeof setInterval> | null = null

async function loadAll() {
  await Promise.all([store.loadStatus(), store.loadStats(), store.loadEvents(page.value, perPage)])
}

function statusLabel(s: string) {
  return s === 'active' ? 'Ativo' : 'Aquecendo'
}

function statusColor(s: string) {
  return s === 'active'
    ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
    : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
}

function severityColor(s: string) {
  return s === 'critical'
    ? 'bg-red-500/10 text-red-400 border-red-500/20'
    : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
}

function scoreFromMessage(msg: string): number | null {
  const m = msg.match(/score=([\d.]+)/)
  return m ? parseFloat(m[1]) : null
}

function scoreBadgeColor(score: number | null): string {
  if (score === null) return 'text-zinc-500'
  if (score >= 0.80) return 'text-red-400'
  if (score >= 0.65) return 'text-amber-400'
  return 'text-zinc-400'
}

function formatDate(s: string | null) {
  if (!s) return '—'
  return new Date(s + 'Z').toLocaleString('pt-BR')
}

function warmupPct(s: { samples_collected: number; samples_needed: number }) {
  return Math.min(100, Math.round((s.samples_collected / s.samples_needed) * 100))
}

const totalPages = () => Math.ceil(store.eventsTotal / perPage)

onMounted(async () => {
  await loadAll()
  timer = setInterval(loadAll, 30_000)
})

onUnmounted(() => { if (timer) clearInterval(timer) })
</script>

<template>
  <div class="p-6 space-y-6 max-w-7xl mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Brain class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">IA Insights</h1>
      </div>
      <button @click="loadAll"
        class="p-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 transition-colors">
        <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': store.loading }" />
      </button>
    </div>

    <!-- LLM + stats strip -->
    <div class="grid grid-cols-2 sm:grid-cols-4 gap-4">
      <!-- LLM status -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
        <div class="text-xs text-zinc-500 mb-1">LLM</div>
        <div v-if="store.status?.llm_enabled" class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full bg-emerald-500"></span>
          <span class="text-sm text-slate-200 font-mono">{{ store.status?.llm_model }}</span>
        </div>
        <div v-else class="text-sm text-zinc-500 italic">desabilitado</div>
      </div>

      <!-- Total anomalies -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
        <div class="text-xs text-zinc-500 mb-1">Total Anomalias</div>
        <div class="text-2xl font-bold text-slate-100">{{ store.stats?.total_ml_anomalies ?? '—' }}</div>
      </div>

      <!-- Last 24h -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
        <div class="text-xs text-zinc-500 mb-1">Últimas 24h</div>
        <div class="text-2xl font-bold" :class="(store.stats?.anomalies_last_24h ?? 0) > 0 ? 'text-amber-400' : 'text-slate-100'">
          {{ store.stats?.anomalies_last_24h ?? '—' }}
        </div>
      </div>

      <!-- Severity -->
      <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-4">
        <div class="text-xs text-zinc-500 mb-1">Severidade</div>
        <div class="flex items-center gap-3 text-sm">
          <span class="text-amber-400">{{ store.stats?.severity_breakdown?.warning ?? 0 }} warn</span>
          <span class="text-red-400">{{ store.stats?.severity_breakdown?.critical ?? 0 }} crit</span>
        </div>
      </div>
    </div>

    <!-- Model status per exporter -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div class="px-4 py-3 border-b border-zinc-800">
        <h2 class="text-sm font-semibold text-zinc-300">Status por Exporter</h2>
      </div>
      <div v-if="!store.status || store.status.exporters.length === 0"
        class="text-center py-8 text-zinc-500 text-sm">
        Nenhum exporter com dados ML ainda.
        <div class="mt-1 text-xs">O modelo começa após 5.000 amostras por exporter (~83 min de tráfego).</div>
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs text-zinc-500 border-b border-zinc-800 bg-zinc-900/80">
              <th class="px-4 py-3">Exporter</th>
              <th class="px-4 py-3">Status</th>
              <th class="px-4 py-3">Amostras</th>
              <th class="px-4 py-3">Pontuações</th>
              <th class="px-4 py-3">Anomalias</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/50">
            <tr v-for="exp in store.status?.exporters" :key="exp.exporter_ip"
              class="hover:bg-zinc-800/30 transition-colors">
              <td class="px-4 py-3 font-mono text-slate-200 text-xs">{{ exp.exporter_ip }}</td>
              <td class="px-4 py-3">
                <span class="px-2 py-0.5 rounded-full border text-xs font-semibold"
                  :class="statusColor(exp.status)">
                  {{ statusLabel(exp.status) }}
                </span>
              </td>
              <td class="px-4 py-3">
                <div v-if="exp.status === 'warming_up'" class="min-w-[120px]">
                  <div class="flex justify-between text-xs text-zinc-500 mb-1">
                    <span>{{ exp.samples_collected.toLocaleString() }}</span>
                    <span>{{ exp.samples_needed.toLocaleString() }}</span>
                  </div>
                  <div class="w-full bg-zinc-700 rounded-full h-1.5">
                    <div class="bg-amber-500 h-1.5 rounded-full transition-all"
                      :style="{ width: warmupPct(exp) + '%' }"></div>
                  </div>
                </div>
                <span v-else class="text-zinc-400 text-xs">{{ exp.samples_collected.toLocaleString() }}</span>
              </td>
              <td class="px-4 py-3 text-zinc-400 text-xs">{{ exp.n_scored.toLocaleString() }}</td>
              <td class="px-4 py-3">
                <span :class="exp.anomalies_total > 0 ? 'text-amber-400 font-semibold' : 'text-zinc-500'">
                  {{ exp.anomalies_total }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Top offenders -->
    <div v-if="store.stats?.top_offenders?.length"
      class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div class="px-4 py-3 border-b border-zinc-800">
        <h2 class="text-sm font-semibold text-zinc-300">Top Ofensores</h2>
      </div>
      <div class="px-4 py-3 flex flex-wrap gap-3">
        <div v-for="o in store.stats.top_offenders" :key="o.src_ip"
          class="flex items-center gap-2 bg-zinc-800 rounded-lg px-3 py-1.5">
          <AlertTriangle class="w-3.5 h-3.5 text-amber-400" />
          <span class="font-mono text-xs text-slate-200">{{ o.src_ip }}</span>
          <span class="text-xs text-zinc-400">{{ o.count }}×</span>
        </div>
      </div>
    </div>

    <!-- ML anomaly events table -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div class="px-4 py-3 border-b border-zinc-800 flex items-center justify-between">
        <h2 class="text-sm font-semibold text-zinc-300">Anomalias Detectadas</h2>
        <span class="text-xs text-zinc-500">{{ store.eventsTotal }} total</span>
      </div>

      <div v-if="store.events.length === 0" class="text-center py-10 text-zinc-500 text-sm">
        Nenhuma anomalia detectada ainda.
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs text-zinc-500 border-b border-zinc-800 bg-zinc-900/80">
              <th class="px-4 py-3">Hora</th>
              <th class="px-4 py-3">Exporter</th>
              <th class="px-4 py-3">IP</th>
              <th class="px-4 py-3">Sev.</th>
              <th class="px-4 py-3">Score</th>
              <th class="px-4 py-3">PPS</th>
              <th class="px-4 py-3">Explicação IA</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/50">
            <tr v-for="ev in store.events" :key="ev.id"
              class="hover:bg-zinc-800/30 transition-colors">
              <td class="px-4 py-2.5 text-xs text-zinc-500 whitespace-nowrap">
                {{ formatDate(ev.created_at) }}
              </td>
              <td class="px-4 py-2.5 font-mono text-zinc-400 text-xs">{{ ev.exporter_ip }}</td>
              <td class="px-4 py-2.5 font-mono text-slate-200 text-xs">{{ ev.src_ip }}</td>
              <td class="px-4 py-2.5">
                <span class="px-2 py-0.5 rounded-full border text-xs font-semibold uppercase"
                  :class="severityColor(ev.severity)">{{ ev.severity }}</span>
              </td>
              <td class="px-4 py-2.5 font-mono text-xs">
                <span :class="scoreBadgeColor(scoreFromMessage(ev.message))">
                  {{ scoreFromMessage(ev.message)?.toFixed(3) ?? '—' }}
                </span>
              </td>
              <td class="px-4 py-2.5 text-xs text-zinc-400">
                {{ ev.pps != null ? ev.pps.toFixed(0) : '—' }}
              </td>
              <td class="px-4 py-2.5 text-xs max-w-xs">
                <div v-if="ev.explanation" class="text-zinc-300 line-clamp-2" :title="ev.explanation">
                  {{ ev.explanation }}
                </div>
                <div v-else class="flex items-center gap-1.5 text-zinc-600 italic">
                  <Loader2 class="w-3 h-3 animate-spin" />
                  gerando...
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div v-if="totalPages() > 1"
        class="flex items-center justify-between px-4 py-3 border-t border-zinc-800 text-xs text-zinc-500">
        <span>{{ store.eventsTotal }} eventos</span>
        <div class="flex items-center gap-2">
          <button :disabled="page === 0" @click="page--; store.loadEvents(page, perPage)"
            class="px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 transition-colors">
            ← Anterior
          </button>
          <span>{{ page + 1 }} / {{ totalPages() }}</span>
          <button :disabled="page + 1 >= totalPages()" @click="page++; store.loadEvents(page, perPage)"
            class="px-3 py-1 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 transition-colors">
            Próxima →
          </button>
        </div>
      </div>
    </div>

    <!-- No model yet — help text -->
    <div v-if="!store.status?.exporters?.length"
      class="bg-zinc-900/40 border border-zinc-800/60 rounded-xl p-5 text-center">
      <CheckCircle2 class="w-8 h-8 text-zinc-600 mx-auto mb-2" />
      <p class="text-zinc-400 text-sm">
        O motor de ML está iniciando. Ele aprende o perfil normal de cada assinante
        automaticamente a partir dos flows coletados — sem configuração necessária.
      </p>
      <p class="text-zinc-600 text-xs mt-2">
        Após ~83 minutos de tráfego, o primeiro modelo será treinado e começará a pontuar flows.
      </p>
    </div>
  </div>
</template>
