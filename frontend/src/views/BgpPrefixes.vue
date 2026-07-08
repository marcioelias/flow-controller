<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { useBgpStore, type BgpPrefix } from '../stores/bgp'
import { ListFilter, Plus, Edit2, Trash2, Radio } from 'lucide-vue-next'

const bgp = useBgpStore()

interface PrefixForm { prefix: string; description: string }
const emptyForm = (): PrefixForm => ({ prefix: '', description: '' })

const showForm = ref(false)
const editingId = ref<number | null>(null)
const form = reactive<PrefixForm>(emptyForm())
const formError = ref('')
const formLoading = ref(false)
const deleteConfirm = ref<number | null>(null)

// Announce modal — triggered by "Anunciar" button
const showAnnounceModal = ref(false)
const announcePrefix = ref('')
const announceForm = reactive({ next_hop: 'self', community_id: null as number | null, peer_id: null as number | null })
const announceLoading = ref(false)
const announceError = ref('')
const lastCommand = ref('')
let commandTimer: ReturnType<typeof setTimeout> | null = null

function isValidCidr(s: string) { return /^\d+\.\d+\.\d+\.\d+\/\d+$/.test(s) }

function openCreate() {
  editingId.value = null; Object.assign(form, emptyForm()); formError.value = ''; showForm.value = true
}
function openEdit(p: BgpPrefix) {
  editingId.value = p.id; Object.assign(form, { prefix: p.prefix, description: p.description ?? '' }); formError.value = ''; showForm.value = true
}
function openAnnounce(prefix: string) {
  announcePrefix.value = prefix; announceForm.next_hop = 'self'; announceForm.community_id = null; announceForm.peer_id = null
  announceError.value = ''; showAnnounceModal.value = true
}

async function submit() {
  if (!isValidCidr(form.prefix)) { formError.value = 'Formato CIDR inválido (ex: 203.0.113.0/24)'; return }
  formLoading.value = true; formError.value = ''
  try {
    const data = { prefix: form.prefix, description: form.description || undefined }
    if (editingId.value !== null) await bgp.updatePrefix(editingId.value, data)
    else await bgp.createPrefix(data)
    showForm.value = false
  } catch (e: unknown) { formError.value = (e as Error).message }
  finally { formLoading.value = false }
}

async function doDelete(id: number) {
  try { await bgp.deletePrefix(id) } finally { deleteConfirm.value = null }
}

async function doAnnounce() {
  announceLoading.value = true; announceError.value = ''
  try {
    const result = await bgp.announce({ prefix: announcePrefix.value, next_hop: announceForm.next_hop, community_id: announceForm.community_id, peer_id: announceForm.peer_id })
    lastCommand.value = result.command
    showAnnounceModal.value = false
    if (commandTimer) clearTimeout(commandTimer)
    commandTimer = setTimeout(() => { lastCommand.value = '' }, 5000)
  } catch (e: unknown) { announceError.value = (e as Error).message }
  finally { announceLoading.value = false }
}

onMounted(async () => {
  await Promise.all([bgp.loadPrefixes(), bgp.loadCommunities(), bgp.loadPeers()])
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-3xl mx-auto">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <ListFilter class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">Catálogo de Prefixos</h1>
      </div>
      <button @click="openCreate" class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-medium transition-colors">
        <Plus class="w-4 h-4" /> Novo Prefixo
      </button>
    </div>

    <div v-if="lastCommand" class="flex items-center gap-2 p-3 rounded-lg bg-zinc-800 border border-zinc-700 text-xs font-mono text-emerald-400">
      Enviado ao ExaBGP: <span class="text-slate-200">{{ lastCommand }}</span>
    </div>

    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="bgp.prefixes.length === 0" class="text-center py-12 text-zinc-500 text-sm">Nenhum prefixo cadastrado.</div>
      <table v-else class="w-full text-sm">
        <thead class="border-b border-zinc-800">
          <tr class="text-left text-xs text-zinc-500">
            <th class="px-4 py-3">Prefixo</th>
            <th class="px-4 py-3">Descrição</th>
            <th class="px-4 py-3"></th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50">
          <tr v-for="p in bgp.prefixes" :key="p.id" class="hover:bg-zinc-800/30 transition-colors">
            <td class="px-4 py-3 font-mono text-slate-200">{{ p.prefix }}</td>
            <td class="px-4 py-3 text-zinc-400">{{ p.description ?? '—' }}</td>
            <td class="px-4 py-3">
              <div v-if="deleteConfirm === p.id" class="flex items-center gap-2">
                <span class="text-xs text-red-400">Excluir?</span>
                <button @click="doDelete(p.id)" class="text-xs px-2 py-0.5 rounded bg-red-500 hover:bg-red-400 text-white">Sim</button>
                <button @click="deleteConfirm = null" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
              </div>
              <div v-else class="flex items-center gap-3">
                <button @click="openAnnounce(p.prefix)" class="flex items-center gap-1 text-xs text-emerald-400 hover:text-emerald-300 transition-colors font-medium">
                  <Radio class="w-3.5 h-3.5" /> Anunciar
                </button>
                <button @click="openEdit(p)" class="text-zinc-400 hover:text-zinc-200"><Edit2 class="w-4 h-4" /></button>
                <button @click="deleteConfirm = p.id" class="text-zinc-400 hover:text-red-400"><Trash2 class="w-4 h-4" /></button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <Teleport to="body">
    <!-- Prefix form modal -->
    <div v-if="showForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showForm = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-md mx-4 space-y-4">
        <h3 class="font-semibold text-slate-100">{{ editingId ? 'Editar Prefixo' : 'Novo Prefixo' }}</h3>
        <div class="space-y-3">
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Prefixo CIDR *</label>
            <input v-model="form.prefix" placeholder="203.0.113.0/24" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Descrição</label>
            <input v-model="form.description" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
        </div>
        <div v-if="formError" class="text-xs text-red-400">{{ formError }}</div>
        <div class="flex gap-3">
          <button @click="showForm = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="submit" :disabled="formLoading || !form.prefix" class="flex-1 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white text-sm font-medium transition-colors">
            {{ editingId ? 'Salvar' : 'Criar' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Announce modal -->
    <div v-if="showAnnounceModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showAnnounceModal = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-md mx-4 space-y-4">
        <h3 class="font-semibold text-slate-100">Anunciar Prefixo</h3>
        <p class="text-sm font-mono text-emerald-400">{{ announcePrefix }}</p>
        <div class="space-y-3">
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Next-hop</label>
            <input v-model="announceForm.next_hop" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Community</label>
            <select v-model="announceForm.community_id" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500">
              <option :value="null">Nenhuma</option>
              <option v-for="c in bgp.communities" :key="c.id" :value="c.id">{{ c.name }}</option>
            </select>
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Peer</label>
            <select v-model="announceForm.peer_id" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500">
              <option :value="null">Todos os peers</option>
              <option v-for="p in bgp.peers" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
          </div>
        </div>
        <div v-if="announceError" class="text-xs text-red-400">{{ announceError }}</div>
        <div class="flex gap-3">
          <button @click="showAnnounceModal = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="doAnnounce" :disabled="announceLoading" class="flex-1 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white text-sm font-medium transition-colors">
            Anunciar
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
