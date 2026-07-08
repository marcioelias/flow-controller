<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Monitor, Plus, Edit, Trash2 } from 'lucide-vue-next'

const router = useRouter()
const authStore = useAuthStore()

interface Exporter {
  id: number
  ip_address: string
  name: string
  description: string | null
  location: string | null
  enabled: boolean
  created_at: string
  updated_at: string
}

const exporters = ref<Exporter[]>([])
const loading = ref(false)
const error = ref('')
const showDeleteConfirm = ref(false)
const exporterToDelete = ref<Exporter | null>(null)

async function loadExporters() {
  loading.value = true
  error.value = ''

  try {
    const response = await fetch('/api/exporters', {
      headers: {
        'Authorization': `Bearer ${authStore.token}`
      }
    })

    if (!response.ok) {
      throw new Error('Failed to load exporters')
    }

    exporters.value = await response.json()
  } catch (e: any) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

function goToCreate() {
  router.push('/exporters/new')
}

function goToEdit(id: number) {
  router.push(`/exporters/${id}/edit`)
}

function confirmDelete(exporter: Exporter) {
  exporterToDelete.value = exporter
  showDeleteConfirm.value = true
}

async function deleteExporter() {
  if (!exporterToDelete.value) return

  try {
    const response = await fetch(`/api/exporters/${exporterToDelete.value.id}`, {
      method: 'DELETE',
      headers: {
        'Authorization': `Bearer ${authStore.token}`
      }
    })

    if (!response.ok) {
      throw new Error('Failed to delete exporter')
    }

    await loadExporters()
  } catch (e: any) {
    error.value = e.message
  } finally {
    showDeleteConfirm.value = false
    exporterToDelete.value = null
  }
}

onMounted(() => {
  loadExporters()
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-6xl mx-auto">
      <!-- Header -->
      <div class="flex items-center justify-between mb-8">
        <div class="flex items-center gap-4">
          <div>
            <h1 class="text-3xl font-bold tracking-tight flex items-center gap-3">
              <Monitor class="w-8 h-8 text-emerald-500" />
              Flow Exporters
            </h1>
            <p class="text-zinc-400 mt-1">Gerenciar dispositivos autorizados a enviar flows</p>
          </div>
        </div>
        <button
          @click="goToCreate"
          class="flex items-center gap-2 px-4 py-2 bg-emerald-500 hover:bg-emerald-600 rounded-lg font-medium transition-colors"
        >
          <Plus class="w-5 h-5" />
          Novo Exporter
        </button>
      </div>

      <!-- Error message -->
      <div v-if="error" class="mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400">
        {{ error }}
      </div>

      <!-- Loading -->
      <div v-if="loading" class="text-center py-12 text-zinc-400">
        Carregando...
      </div>

      <!-- Empty state -->
      <Card v-else-if="exporters.length === 0" class="bg-zinc-900 border-zinc-800">
        <CardContent class="text-center py-12">
          <Monitor class="w-16 h-16 mx-auto mb-4 text-zinc-600" />
          <h3 class="text-xl font-semibold mb-2">Nenhum exporter cadastrado</h3>
          <p class="text-zinc-400 mb-6">Adicione dispositivos autorizados a enviar flows NetFlow/IPFIX</p>
          <button
            @click="goToCreate"
            class="inline-flex items-center gap-2 px-6 py-3 bg-emerald-500 hover:bg-emerald-600 rounded-lg font-medium transition-colors"
          >
            <Plus class="w-5 h-5" />
            Adicionar Primeiro Exporter
          </button>
        </CardContent>
      </Card>

      <!-- Exporters list -->
      <div v-else class="space-y-4">
        <Card
          v-for="exporter in exporters"
          :key="exporter.id"
          class="bg-zinc-900 border-zinc-800 hover:border-zinc-700 transition-colors"
        >
          <CardContent class="p-6">
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="flex items-center gap-3 mb-2">
                  <h3 class="text-xl font-semibold">{{ exporter.name }}</h3>
                  <span
                    :class="[
                      'px-2 py-1 text-xs font-medium rounded',
                      exporter.enabled
                        ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                        : 'bg-zinc-700 text-zinc-400 border border-zinc-600'
                    ]"
                  >
                    {{ exporter.enabled ? 'Ativo' : 'Inativo' }}
                  </span>
                </div>

                <div class="grid grid-cols-2 gap-4 text-sm">
                  <div>
                    <span class="text-zinc-500">IP:</span>
                    <span class="ml-2 font-mono text-emerald-400">{{ exporter.ip_address }}</span>
                  </div>
                  <div v-if="exporter.location">
                    <span class="text-zinc-500">Localização:</span>
                    <span class="ml-2 text-zinc-300">{{ exporter.location }}</span>
                  </div>
                </div>

                <p v-if="exporter.description" class="mt-3 text-zinc-400 text-sm">
                  {{ exporter.description }}
                </p>
              </div>

              <div class="flex items-center gap-2 ml-4">
                <button
                  @click="goToEdit(exporter.id)"
                  class="p-2 hover:bg-zinc-800 rounded-lg transition-colors text-blue-400 hover:text-blue-300"
                  title="Editar"
                >
                  <Edit class="w-5 h-5" />
                </button>
                <button
                  @click="confirmDelete(exporter)"
                  class="p-2 hover:bg-zinc-800 rounded-lg transition-colors text-red-400 hover:text-red-300"
                  title="Deletar"
                >
                  <Trash2 class="w-5 h-5" />
                </button>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>

    <!-- Delete Confirmation Modal -->
    <div
      v-if="showDeleteConfirm"
      class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      @click.self="showDeleteConfirm = false"
    >
      <Card class="w-full max-w-md bg-zinc-900 border-zinc-800">
        <CardHeader>
          <CardTitle>Confirmar Exclusão</CardTitle>
        </CardHeader>
        <CardContent class="space-y-4">
          <p class="text-zinc-300">
            Deseja realmente remover o exporter <strong class="text-slate-100">{{ exporterToDelete?.name }}</strong>?
          </p>
          <p class="text-sm text-zinc-400">
            Flows do IP <span class="font-mono text-emerald-400">{{ exporterToDelete?.ip_address }}</span> serão bloqueados.
          </p>
          <div class="flex gap-3 justify-end">
            <button
              @click="showDeleteConfirm = false"
              class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 rounded-lg transition-colors"
            >
              Cancelar
            </button>
            <button
              @click="deleteExporter"
              class="px-4 py-2 bg-red-500 hover:bg-red-600 rounded-lg font-medium transition-colors"
            >
              Confirmar Exclusão
            </button>
          </div>
        </CardContent>
      </Card>
    </div>
  </div>
</template>
