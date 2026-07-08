<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { useBgpStore, type BgpPeer } from '../stores/bgp'
import { Server, Plus, Edit2, Trash2, AlertTriangle, CheckCircle, ToggleLeft, ToggleRight } from 'lucide-vue-next'

const bgp = useBgpStore()

interface PeerForm {
  name: string
  description: string
  neighbor_ip: string
  local_ip: string
  local_as: number | ''
  peer_as: number | ''
  hold_time: number
  md5_password: string
  enabled: boolean
}

const emptyForm = (): PeerForm => ({ name: '', description: '', neighbor_ip: '', local_ip: '', local_as: '', peer_as: '', hold_time: 90, md5_password: '', enabled: true })

const showForm = ref(false)
const editingId = ref<number | null>(null)
const form = reactive<PeerForm>(emptyForm())
const formError = ref('')
const formLoading = ref(false)
const deleteConfirm = ref<number | null>(null)
const deleteLoading = ref(false)

// Apply config
const applyLoading = ref(false)
const applyResult = ref<{ peers_configured: number } | null>(null)
const applyError = ref('')
const showApplyConfirm = ref(false)

function openCreate() {
  editingId.value = null
  Object.assign(form, emptyForm())
  formError.value = ''
  showForm.value = true
}

function openEdit(p: BgpPeer) {
  editingId.value = p.id
  Object.assign(form, {
    name: p.name, description: p.description ?? '', neighbor_ip: p.neighbor_ip,
    local_ip: p.local_ip, local_as: p.local_as, peer_as: p.peer_as,
    hold_time: p.hold_time, md5_password: '', enabled: p.enabled,
  })
  formError.value = ''
  showForm.value = true
}

async function submit() {
  formLoading.value = true
  formError.value = ''
  try {
    const payload = {
      name: form.name, description: form.description || null,
      neighbor_ip: form.neighbor_ip, local_ip: form.local_ip,
      local_as: Number(form.local_as), peer_as: Number(form.peer_as),
      hold_time: form.hold_time, enabled: form.enabled,
      md5_password: form.md5_password || undefined,
    }
    if (editingId.value !== null) {
      await bgp.updatePeer(editingId.value, payload)
    } else {
      await bgp.createPeer(payload)
    }
    showForm.value = false
  } catch (e: unknown) {
    formError.value = (e as Error).message
  } finally {
    formLoading.value = false
  }
}

async function doDelete(id: number) {
  deleteLoading.value = true
  try {
    await bgp.deletePeer(id)
  } catch (e: unknown) {
    formError.value = (e as Error).message
  } finally {
    deleteLoading.value = false
    deleteConfirm.value = null
  }
}

async function doApply() {
  applyLoading.value = true
  applyError.value = ''
  applyResult.value = null
  showApplyConfirm.value = false
  try {
    applyResult.value = await bgp.applyConfig()
  } catch (e: unknown) {
    applyError.value = (e as Error).message
  } finally {
    applyLoading.value = false
  }
}

onMounted(() => bgp.loadPeers())
</script>

<template>
  <div class="p-6 space-y-6 max-w-5xl mx-auto">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Server class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">BGP Peers</h1>
      </div>
      <div class="flex items-center gap-3">
        <!-- Apply config button -->
        <button
          @click="showApplyConfirm = true"
          class="flex items-center gap-2 px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 text-white text-sm font-medium transition-colors"
        >
          <CheckCircle class="w-4 h-4" />
          Aplicar Configuração
        </button>
        <button @click="openCreate" class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-medium transition-colors">
          <Plus class="w-4 h-4" /> Novo Peer
        </button>
      </div>
    </div>

    <!-- Apply feedback -->
    <div v-if="applyResult" class="flex items-center gap-2 p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-sm text-emerald-400">
      <CheckCircle class="w-4 h-4" />
      Configuração aplicada — {{ applyResult.peers_configured }} peer(s) configurado(s). ExaBGP reiniciará as sessões.
    </div>
    <div v-if="applyError" class="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-sm text-red-400">{{ applyError }}</div>

    <!-- Peer table -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="bgp.peers.length === 0" class="text-center py-12 text-zinc-500 text-sm">
        Nenhum peer configurado.
      </div>
      <table v-else class="w-full text-sm">
        <thead class="border-b border-zinc-800">
          <tr class="text-left text-xs text-zinc-500">
            <th class="px-4 py-3">Nome</th>
            <th class="px-4 py-3">Neighbor IP</th>
            <th class="px-4 py-3">Local IP</th>
            <th class="px-4 py-3">ASN local</th>
            <th class="px-4 py-3">ASN remoto</th>
            <th class="px-4 py-3">Hold</th>
            <th class="px-4 py-3">MD5</th>
            <th class="px-4 py-3">Status</th>
            <th class="px-4 py-3"></th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50">
          <tr v-for="p in bgp.peers" :key="p.id" class="hover:bg-zinc-800/30 transition-colors">
            <td class="px-4 py-3 text-slate-200 font-medium">
              {{ p.name }}
              <p v-if="p.description" class="text-xs text-zinc-500 font-normal">{{ p.description }}</p>
            </td>
            <td class="px-4 py-3 font-mono text-zinc-300">{{ p.neighbor_ip }}</td>
            <td class="px-4 py-3 font-mono text-zinc-400">{{ p.local_ip }}</td>
            <td class="px-4 py-3 font-mono text-zinc-400">{{ p.local_as }}</td>
            <td class="px-4 py-3 font-mono text-zinc-400">{{ p.peer_as }}</td>
            <td class="px-4 py-3 text-zinc-400">{{ p.hold_time }}s</td>
            <td class="px-4 py-3">
              <span v-if="p.has_md5" class="text-xs px-1.5 py-0.5 rounded bg-zinc-700 text-zinc-300">sim</span>
              <span v-else class="text-zinc-600">—</span>
            </td>
            <td class="px-4 py-3">
              <span class="flex items-center gap-1 text-xs" :class="p.enabled ? 'text-emerald-400' : 'text-zinc-500'">
                <ToggleRight v-if="p.enabled" class="w-4 h-4" />
                <ToggleLeft v-else class="w-4 h-4" />
                {{ p.enabled ? 'ativo' : 'inativo' }}
              </span>
            </td>
            <td class="px-4 py-3">
              <div v-if="deleteConfirm === p.id" class="flex items-center gap-2">
                <span class="text-xs text-red-400">Excluir?</span>
                <button @click="doDelete(p.id)" :disabled="deleteLoading" class="text-xs px-2 py-0.5 rounded bg-red-500 hover:bg-red-400 text-white">Sim</button>
                <button @click="deleteConfirm = null" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
              </div>
              <div v-else class="flex items-center gap-2">
                <button @click="openEdit(p)" class="text-zinc-400 hover:text-zinc-200 transition-colors"><Edit2 class="w-4 h-4" /></button>
                <button @click="deleteConfirm = p.id" class="text-zinc-400 hover:text-red-400 transition-colors"><Trash2 class="w-4 h-4" /></button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <!-- Apply confirm modal -->
  <Teleport to="body">
    <div v-if="showApplyConfirm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showApplyConfirm = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-md mx-4 space-y-4">
        <div class="flex items-start gap-3">
          <AlertTriangle class="w-5 h-5 text-amber-400 flex-shrink-0 mt-0.5" />
          <div>
            <h3 class="font-semibold text-slate-100">Aplicar Configuração BGP</h3>
            <p class="text-sm text-zinc-400 mt-1">Isso regenera o <span class="font-mono text-zinc-300">exabgp.conf</span> e reinicia as sessões BGP. Peers ativos serão reconectados.</p>
          </div>
        </div>
        <div class="flex gap-3">
          <button @click="showApplyConfirm = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="doApply" :disabled="applyLoading" class="flex-1 px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-50 text-white text-sm font-medium transition-colors">
            Confirmar
          </button>
        </div>
      </div>
    </div>

    <!-- Peer form modal -->
    <div v-if="showForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showForm = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-lg mx-4 space-y-4 max-h-[90vh] overflow-y-auto">
        <h3 class="font-semibold text-slate-100">{{ editingId ? 'Editar Peer' : 'Novo Peer' }}</h3>
        <div class="grid grid-cols-2 gap-3">
          <div class="col-span-2">
            <label class="text-xs text-zinc-400 mb-1 block">Nome *</label>
            <input v-model="form.name" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
          <div class="col-span-2">
            <label class="text-xs text-zinc-400 mb-1 block">Descrição</label>
            <input v-model="form.description" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Neighbor IP *</label>
            <input v-model="form.neighbor_ip" placeholder="10.0.0.1" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Local IP *</label>
            <input v-model="form.local_ip" placeholder="10.0.0.2" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">ASN Local *</label>
            <input v-model="form.local_as" type="number" placeholder="65001" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">ASN Remoto *</label>
            <input v-model="form.peer_as" type="number" placeholder="65000" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Hold Time (s)</label>
            <input v-model="form.hold_time" type="number" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Senha MD5 <span class="text-zinc-600">(opcional)</span></label>
            <input v-model="form.md5_password" type="password" placeholder="deixe vazio para não alterar" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
          <div class="col-span-2 flex items-center gap-3">
            <label class="text-sm text-zinc-400">Habilitado</label>
            <button type="button" @click="form.enabled = !form.enabled" class="transition-colors">
              <ToggleRight v-if="form.enabled" class="w-6 h-6 text-emerald-500" />
              <ToggleLeft v-else class="w-6 h-6 text-zinc-600" />
            </button>
          </div>
        </div>
        <div v-if="formError" class="text-xs text-red-400">{{ formError }}</div>
        <div class="flex gap-3 pt-1">
          <button @click="showForm = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="submit" :disabled="formLoading || !form.name || !form.neighbor_ip || !form.local_ip" class="flex-1 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white text-sm font-medium transition-colors">
            {{ editingId ? 'Salvar' : 'Criar' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
