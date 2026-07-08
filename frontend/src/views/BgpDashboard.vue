<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useBgpStore } from '../stores/bgp'
import { Radio, Activity, Plus, Trash2, Server, Bot } from 'lucide-vue-next'

const router = useRouter()
const bgp = useBgpStore()

// Announce modal
const showAnnounceModal = ref(false)
const announceForm = ref({ prefix: '', next_hop: 'self', community_id: null as number | null, peer_id: null as number | null })
const announceLoading = ref(false)
const announceError = ref('')
const lastCommand = ref('')
let commandTimer: ReturnType<typeof setTimeout> | null = null

// Withdraw confirm
const withdrawConfirm = ref<number | null>(null)
const withdrawLoading = ref(false)

let refreshInterval: ReturnType<typeof setInterval> | null = null

function stateColor(state: string) {
  if (state === 'up')   return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
  if (state === 'down') return 'bg-red-500/10 text-red-400 border-red-500/20'
  return 'bg-zinc-700/50 text-zinc-400 border-zinc-700'
}
function stateDot(state: string) {
  if (state === 'up')   return 'bg-emerald-500'
  if (state === 'down') return 'bg-red-500'
  return 'bg-zinc-500'
}

function formatDate(s: string | null) {
  if (!s) return '—'
  return new Date(s + 'Z').toLocaleString('pt-BR')
}

async function doAnnounce() {
  announceLoading.value = true
  announceError.value = ''
  try {
    const result = await bgp.announce({
      prefix: announceForm.value.prefix,
      next_hop: announceForm.value.next_hop,
      community_id: announceForm.value.community_id,
      peer_id: announceForm.value.peer_id,
    })
    lastCommand.value = result.command
    showAnnounceModal.value = false
    announceForm.value = { prefix: '', next_hop: 'self', community_id: null, peer_id: null }
    await bgp.loadAnnouncements()
    if (commandTimer) clearTimeout(commandTimer)
    commandTimer = setTimeout(() => { lastCommand.value = '' }, 5000)
  } catch (e: unknown) {
    announceError.value = (e as Error).message
  } finally {
    announceLoading.value = false
  }
}

async function doWithdraw(id: number) {
  withdrawLoading.value = true
  try {
    await bgp.withdraw(id)
    await bgp.loadAnnouncements()
  } finally {
    withdrawLoading.value = false
    withdrawConfirm.value = null
  }
}

function openAnnounceModal(prefill?: string) {
  announceForm.value = { prefix: prefill ?? '', next_hop: 'self', community_id: null, peer_id: null }
  announceError.value = ''
  showAnnounceModal.value = true
}

onMounted(async () => {
  await Promise.all([
    bgp.loadSessions(),
    bgp.loadAnnouncements(),
    bgp.loadCommunities(),
    bgp.loadPeers(),
  ])
  refreshInterval = setInterval(() => bgp.loadSessions(), 10000)
})

onUnmounted(() => {
  if (refreshInterval) clearInterval(refreshInterval)
  if (commandTimer) clearTimeout(commandTimer)
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-6xl mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Radio class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">BGP</h1>
      </div>
      <button
        @click="openAnnounceModal()"
        class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-medium transition-colors"
      >
        <Plus class="w-4 h-4" />
        Novo Anúncio
      </button>
    </div>

    <!-- Command feedback -->
    <div v-if="lastCommand" class="flex items-center gap-2 p-3 rounded-lg bg-zinc-800 border border-zinc-700 text-xs font-mono text-emerald-400">
      <Activity class="w-3.5 h-3.5 flex-shrink-0" />
      Enviado ao ExaBGP: <span class="text-slate-200">{{ lastCommand }}</span>
    </div>

    <!-- Sessions -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <Activity class="w-5 h-5 text-sky-400" />
          <span class="font-semibold text-slate-200">Sessões BGP</span>
        </div>
        <button @click="router.push('/bgp/peers')" class="text-xs text-zinc-400 hover:text-zinc-200 transition-colors">
          Gerenciar Peers →
        </button>
      </div>

      <div v-if="bgp.sessions.length === 0" class="text-center py-8 text-zinc-500 text-sm">
        Nenhum peer configurado. <router-link to="/bgp/peers" class="text-emerald-400 hover:underline">Adicionar peer →</router-link>
      </div>
      <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        <div
          v-for="s in bgp.sessions" :key="s.peer_id"
          class="border rounded-lg p-4 space-y-2"
          :class="stateColor(s.state)"
        >
          <div class="flex items-center justify-between">
            <span class="font-medium text-slate-100 text-sm">{{ s.peer_name }}</span>
            <span class="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider px-2 py-0.5 rounded-full border" :class="stateColor(s.state)">
              <span class="w-1.5 h-1.5 rounded-full" :class="stateDot(s.state)" />
              {{ s.state }}
            </span>
          </div>
          <p class="text-xs font-mono text-zinc-400">{{ s.neighbor_ip }}</p>
          <div class="text-xs text-zinc-500 space-y-0.5">
            <div v-if="s.last_up">Último UP: {{ formatDate(s.last_up) }}</div>
            <div v-if="s.last_down">Último DOWN: {{ formatDate(s.last_down) }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Active announcements -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5">
      <div class="flex items-center gap-2 mb-4">
        <Server class="w-5 h-5 text-violet-400" />
        <span class="font-semibold text-slate-200">Anúncios Ativos</span>
        <span class="ml-auto text-xs text-zinc-500">{{ bgp.announcements.length }} ativo(s)</span>
      </div>

      <div v-if="bgp.announcements.length === 0" class="text-center py-8 text-zinc-500 text-sm">
        Nenhum anúncio ativo.
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs text-zinc-500 border-b border-zinc-800">
              <th class="pb-2 pr-4">Prefixo</th>
              <th class="pb-2 pr-4">Next-hop</th>
              <th class="pb-2 pr-4">Community</th>
              <th class="pb-2 pr-4">Peer</th>
              <th class="pb-2 pr-4">Origem</th>
              <th class="pb-2 pr-4">Anunciado</th>
              <th class="pb-2"></th>
            </tr>
          </thead>
          <tbody class="divide-y divide-zinc-800/50">
            <tr v-for="a in bgp.announcements" :key="a.id" class="hover:bg-zinc-800/30 transition-colors">
              <td class="py-2.5 pr-4 font-mono text-slate-200">{{ a.prefix }}</td>
              <td class="py-2.5 pr-4 font-mono text-zinc-400">{{ a.next_hop }}</td>
              <td class="py-2.5 pr-4">
                <span v-if="a.community_name" class="px-2 py-0.5 rounded bg-violet-500/10 text-violet-400 text-xs border border-violet-500/20">
                  {{ a.community_name }}
                </span>
                <span v-else class="text-zinc-600">—</span>
              </td>
              <td class="py-2.5 pr-4 text-zinc-400">{{ a.peer_name ?? 'Todos' }}</td>
              <td class="py-2.5 pr-4">
                <span v-if="a.origin === 'anomaly_detector'" class="flex items-center gap-1 text-amber-400 text-xs">
                  <Bot class="w-3 h-3" /> Auto
                </span>
                <span v-else class="text-zinc-400 text-xs">Manual</span>
              </td>
              <td class="py-2.5 pr-4 text-zinc-500 text-xs">{{ formatDate(a.announced_at) }}</td>
              <td class="py-2.5">
                <div v-if="withdrawConfirm === a.id" class="flex items-center gap-2">
                  <span class="text-xs text-red-400">Confirmar?</span>
                  <button @click="doWithdraw(a.id)" :disabled="withdrawLoading" class="text-xs px-2 py-0.5 rounded bg-red-500 hover:bg-red-400 text-white transition-colors disabled:opacity-50">Sim</button>
                  <button @click="withdrawConfirm = null" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
                </div>
                <button
                  v-else
                  @click="withdrawConfirm = a.id"
                  class="flex items-center gap-1 text-xs text-zinc-500 hover:text-red-400 transition-colors"
                >
                  <Trash2 class="w-3.5 h-3.5" /> Retirar
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <!-- Announce Modal -->
  <Teleport to="body">
    <div v-if="showAnnounceModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showAnnounceModal = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl shadow-2xl p-6 w-full max-w-md mx-4 space-y-4">
        <h3 class="font-semibold text-slate-100">Novo Anúncio BGP</h3>
        <div class="space-y-3">
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Prefixo</label>
            <input v-model="announceForm.prefix" placeholder="203.0.113.0/24"
              class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Next-hop</label>
            <input v-model="announceForm.next_hop" placeholder="self"
              class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Community</label>
            <select v-model="announceForm.community_id"
              class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500">
              <option :value="null">Nenhuma</option>
              <option v-for="c in bgp.communities" :key="c.id" :value="c.id">{{ c.name }} ({{ c.community }})</option>
            </select>
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Peer</label>
            <select v-model="announceForm.peer_id"
              class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500">
              <option :value="null">Todos os peers</option>
              <option v-for="p in bgp.peers" :key="p.id" :value="p.id">{{ p.name }} ({{ p.neighbor_ip }})</option>
            </select>
          </div>
        </div>
        <div v-if="announceError" class="text-xs text-red-400 px-1">{{ announceError }}</div>
        <div class="flex gap-3">
          <button @click="showAnnounceModal = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="doAnnounce" :disabled="!announceForm.prefix || announceLoading"
            class="flex-1 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors flex items-center justify-center gap-2">
            <svg v-if="announceLoading" class="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/></svg>
            Anunciar
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
