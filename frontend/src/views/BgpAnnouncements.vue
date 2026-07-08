<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useBgpStore } from '../stores/bgp'
import { History, Trash2, Bot, User } from 'lucide-vue-next'

const bgp = useBgpStore()
const showOnlyActive = ref(true)
const withdrawConfirm = ref<number | null>(null)
const withdrawLoading = ref(false)
const error = ref('')

const filtered = computed(() =>
  showOnlyActive.value
    ? bgp.announcements.filter(a => !a.withdrawn_at)
    : bgp.announcements
)

function formatDate(s: string | null) {
  if (!s) return '—'
  return new Date(s + 'Z').toLocaleString('pt-BR')
}

async function load() {
  await bgp.loadAnnouncements(false) // sempre carrega tudo, filtramos no computed
}

async function doWithdraw(id: number) {
  withdrawLoading.value = true; error.value = ''
  try {
    await bgp.withdraw(id)
    await load()
  } catch (e: unknown) { error.value = (e as Error).message }
  finally { withdrawLoading.value = false; withdrawConfirm.value = null }
}

onMounted(load)
</script>

<template>
  <div class="p-6 space-y-6 max-w-6xl mx-auto">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <History class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">Histórico de Anúncios</h1>
      </div>
      <label class="flex items-center gap-2 text-sm text-zinc-400 cursor-pointer select-none">
        <input type="checkbox" v-model="showOnlyActive" class="rounded accent-emerald-500" />
        Somente ativos
      </label>
    </div>

    <div v-if="error" class="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-sm text-red-400">{{ error }}</div>

    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="filtered.length === 0" class="text-center py-12 text-zinc-500 text-sm">Nenhum anúncio encontrado.</div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead class="border-b border-zinc-800">
            <tr class="text-left text-xs text-zinc-500">
              <th class="px-4 py-3">Prefixo</th>
              <th class="px-4 py-3">Next-hop</th>
              <th class="px-4 py-3">Community</th>
              <th class="px-4 py-3">Peer</th>
              <th class="px-4 py-3">Origem</th>
              <th class="px-4 py-3">Anunciado</th>
              <th class="px-4 py-3">Retirado</th>
              <th class="px-4 py-3">Status</th>
              <th class="px-4 py-3"></th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/50">
            <tr v-for="a in filtered" :key="a.id" class="hover:bg-zinc-800/30 transition-colors" :class="a.withdrawn_at ? 'opacity-50' : ''">
              <td class="px-4 py-2.5 font-mono text-slate-200">{{ a.prefix }}</td>
              <td class="px-4 py-2.5 font-mono text-zinc-400">{{ a.next_hop }}</td>
              <td class="px-4 py-2.5">
                <span v-if="a.community_name" class="text-xs px-1.5 py-0.5 rounded bg-violet-500/10 text-violet-400 border border-violet-500/20">{{ a.community_name }}</span>
                <span v-else class="text-zinc-600">—</span>
              </td>
              <td class="px-4 py-2.5 text-zinc-400 text-xs">{{ a.peer_name ?? 'Todos' }}</td>
              <td class="px-4 py-2.5">
                <span v-if="a.origin === 'anomaly_detector'" class="flex items-center gap-1 text-xs text-amber-400">
                  <Bot class="w-3 h-3" /> Auto
                </span>
                <span v-else class="flex items-center gap-1 text-xs text-zinc-400">
                  <User class="w-3 h-3" /> Manual
                </span>
              </td>
              <td class="px-4 py-2.5 text-zinc-500 text-xs">{{ formatDate(a.announced_at) }}</td>
              <td class="px-4 py-2.5 text-zinc-500 text-xs">{{ formatDate(a.withdrawn_at) }}</td>
              <td class="px-4 py-2.5">
                <span v-if="!a.withdrawn_at" class="text-xs px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">ativo</span>
                <span v-else class="text-xs text-zinc-600">retirado</span>
              </td>
              <td class="px-4 py-2.5">
                <template v-if="!a.withdrawn_at">
                  <div v-if="withdrawConfirm === a.id" class="flex items-center gap-2">
                    <span class="text-xs text-red-400">Retirar?</span>
                    <button @click="doWithdraw(a.id)" :disabled="withdrawLoading" class="text-xs px-2 py-0.5 rounded bg-red-500 hover:bg-red-400 text-white">Sim</button>
                    <button @click="withdrawConfirm = null" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
                  </div>
                  <button v-else @click="withdrawConfirm = a.id" class="text-zinc-500 hover:text-red-400 transition-colors">
                    <Trash2 class="w-4 h-4" />
                  </button>
                </template>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
