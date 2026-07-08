<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { useBgpStore, type BgpCommunity } from '../stores/bgp'
import { Tag, Plus, Edit2, Trash2 } from 'lucide-vue-next'

const bgp = useBgpStore()

interface CommunityForm { name: string; community: string; description: string }
const emptyForm = (): CommunityForm => ({ name: '', community: '', description: '' })

const showForm = ref(false)
const editingId = ref<number | null>(null)
const form = reactive<CommunityForm>(emptyForm())
const formError = ref('')
const formLoading = ref(false)
const deleteConfirm = ref<number | null>(null)
const deleteError = ref('')

function openCreate() {
  editingId.value = null
  Object.assign(form, emptyForm())
  formError.value = ''
  showForm.value = true
}
function openEdit(c: BgpCommunity) {
  editingId.value = c.id
  Object.assign(form, { name: c.name, community: c.community, description: c.description ?? '' })
  formError.value = ''
  showForm.value = true
}

async function submit() {
  formLoading.value = true; formError.value = ''
  try {
    const data = { name: form.name, community: form.community, description: form.description || undefined }
    if (editingId.value !== null) await bgp.updateCommunity(editingId.value, data)
    else await bgp.createCommunity(data)
    showForm.value = false
  } catch (e: unknown) { formError.value = (e as Error).message }
  finally { formLoading.value = false }
}

async function doDelete(id: number) {
  deleteError.value = ''
  try {
    await bgp.deleteCommunity(id)
  } catch (e: unknown) { deleteError.value = (e as Error).message }
  finally { deleteConfirm.value = null }
}

// Color badge by community value for visual distinction
const colors = ['bg-violet-500/10 text-violet-400', 'bg-sky-500/10 text-sky-400', 'bg-amber-500/10 text-amber-400', 'bg-pink-500/10 text-pink-400']
function communityColor(id: number) { return colors[id % colors.length] }

onMounted(() => bgp.loadCommunities())
</script>

<template>
  <div class="p-6 space-y-6 max-w-3xl mx-auto">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Tag class="w-6 h-6 text-emerald-500" />
        <h1 class="text-2xl font-bold text-slate-100">BGP Communities</h1>
      </div>
      <button @click="openCreate" class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-medium transition-colors">
        <Plus class="w-4 h-4" /> Nova Community
      </button>
    </div>

    <div v-if="deleteError" class="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-sm text-red-400">{{ deleteError }}</div>

    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="bgp.communities.length === 0" class="text-center py-12 text-zinc-500 text-sm">Nenhuma community cadastrada.</div>
      <table v-else class="w-full text-sm">
        <thead class="border-b border-zinc-800">
          <tr class="text-left text-xs text-zinc-500">
            <th class="px-4 py-3">Nome</th>
            <th class="px-4 py-3">Community</th>
            <th class="px-4 py-3">Descrição</th>
            <th class="px-4 py-3"></th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50">
          <tr v-for="c in bgp.communities" :key="c.id" class="hover:bg-zinc-800/30 transition-colors">
            <td class="px-4 py-3 font-medium text-slate-200">
              <span class="inline-flex items-center px-2 py-0.5 rounded text-xs border" :class="communityColor(c.id)">{{ c.name }}</span>
            </td>
            <td class="px-4 py-3 font-mono text-zinc-300 text-xs">{{ c.community }}</td>
            <td class="px-4 py-3 text-zinc-400">{{ c.description ?? '—' }}</td>
            <td class="px-4 py-3">
              <div v-if="deleteConfirm === c.id" class="flex items-center gap-2">
                <span class="text-xs text-red-400">Excluir?</span>
                <button @click="doDelete(c.id)" class="text-xs px-2 py-0.5 rounded bg-red-500 hover:bg-red-400 text-white">Sim</button>
                <button @click="deleteConfirm = null" class="text-xs text-zinc-500 hover:text-zinc-300">Não</button>
              </div>
              <div v-else class="flex items-center gap-2">
                <button @click="openEdit(c)" class="text-zinc-400 hover:text-zinc-200"><Edit2 class="w-4 h-4" /></button>
                <button @click="deleteConfirm = c.id" class="text-zinc-400 hover:text-red-400"><Trash2 class="w-4 h-4" /></button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <Teleport to="body">
    <div v-if="showForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showForm = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl p-6 w-full max-w-md mx-4 space-y-4">
        <h3 class="font-semibold text-slate-100">{{ editingId ? 'Editar Community' : 'Nova Community' }}</h3>
        <div class="space-y-3">
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Nome *</label>
            <input v-model="form.name" placeholder="Blackhole RTBH" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Community *</label>
            <input v-model="form.community" placeholder="65000:9999 ou 65000:100 no-export" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-emerald-500" />
            <p class="text-xs text-zinc-600 mt-1">Aceita múltiplos valores separados por espaço</p>
          </div>
          <div>
            <label class="text-xs text-zinc-400 mb-1 block">Descrição</label>
            <input v-model="form.description" class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-emerald-500" />
          </div>
        </div>
        <div v-if="formError" class="text-xs text-red-400">{{ formError }}</div>
        <div class="flex gap-3">
          <button @click="showForm = false" class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">Cancelar</button>
          <button @click="submit" :disabled="formLoading || !form.name || !form.community" class="flex-1 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white text-sm font-medium transition-colors">
            {{ editingId ? 'Salvar' : 'Criar' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
