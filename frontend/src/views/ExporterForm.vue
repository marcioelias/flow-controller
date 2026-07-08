<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { Card, CardContent } from '@/components/ui/card'
import { Save } from 'lucide-vue-next'

const router = useRouter()
const route = useRoute()
const authStore = useAuthStore()

const isEdit = computed(() => route.params.id !== undefined)
const exporterId = computed(() => route.params.id ? parseInt(route.params.id as string) : null)

const form = ref({
  ip_address: '',
  name: '',
  description: '',
  location: '',
  enabled: true
})

const loading = ref(false)
const saving = ref(false)
const error = ref('')

async function loadExporter() {
  if (!exporterId.value) return

  loading.value = true
  error.value = ''

  try {
    const response = await fetch(`/api/exporters/${exporterId.value}`, {
      headers: {
        'Authorization': `Bearer ${authStore.token}`
      }
    })

    if (!response.ok) {
      throw new Error('Failed to load exporter')
    }

    const data = await response.json()
    form.value = {
      ip_address: data.ip_address,
      name: data.name,
      description: data.description || '',
      location: data.location || '',
      enabled: data.enabled
    }
  } catch (e: any) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

async function save() {
  saving.value = true
  error.value = ''

  try {
    const url = isEdit.value
      ? `/api/exporters/${exporterId.value}`
      : '/api/exporters'

    const method = isEdit.value ? 'PUT' : 'POST'

    const payload = isEdit.value
      ? {
          // For update, only send non-empty fields
          ...(form.value.ip_address && { ip_address: form.value.ip_address }),
          ...(form.value.name && { name: form.value.name }),
          description: form.value.description || null,
          location: form.value.location || null,
          enabled: form.value.enabled
        }
      : {
          // For create, send all required fields
          ip_address: form.value.ip_address,
          name: form.value.name,
          description: form.value.description || null,
          location: form.value.location || null,
          enabled: form.value.enabled
        }

    const response = await fetch(url, {
      method,
      headers: {
        'Authorization': `Bearer ${authStore.token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(payload)
    })

    if (!response.ok) {
      if (response.status === 409) {
        throw new Error('Este IP já está cadastrado')
      }
      if (response.status === 400) {
        throw new Error('IP inválido')
      }
      throw new Error('Failed to save exporter')
    }

    router.push('/exporters')
  } catch (e: any) {
    error.value = e.message
  } finally {
    saving.value = false
  }
}

function goBack() {
  router.push('/exporters')
}

onMounted(() => {
  if (isEdit.value) {
    loadExporter()
  }
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-2xl mx-auto">
      <!-- Header -->
      <div class="flex items-center gap-4 mb-8">
        <div>
          <h1 class="text-3xl font-bold tracking-tight">
            {{ isEdit ? 'Editar Exporter' : 'Novo Exporter' }}
          </h1>
          <p class="text-zinc-400 mt-1">
            {{ isEdit ? 'Atualizar informações do exporter' : 'Adicionar novo dispositivo autorizado' }}
          </p>
        </div>
      </div>

      <!-- Error message -->
      <div v-if="error" class="mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400">
        {{ error }}
      </div>

      <!-- Loading -->
      <div v-if="loading" class="text-center py-12 text-zinc-400">
        Carregando...
      </div>

      <!-- Form -->
      <Card v-else class="bg-zinc-900 border-zinc-800">
        <CardContent class="p-6">
          <form @submit.prevent="save" class="space-y-6">
            <!-- IP Address -->
            <div>
              <label for="ip_address" class="block text-sm font-medium text-zinc-300 mb-2">
                Endereço IP *
              </label>
              <input
                id="ip_address"
                v-model="form.ip_address"
                type="text"
                required
                placeholder="192.168.1.1"
                class="w-full px-4 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-slate-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent font-mono"
              />
              <p class="mt-1 text-xs text-zinc-500">IP do roteador/switch que enviará flows</p>
            </div>

            <!-- Name -->
            <div>
              <label for="name" class="block text-sm font-medium text-zinc-300 mb-2">
                Nome *
              </label>
              <input
                id="name"
                v-model="form.name"
                type="text"
                required
                placeholder="Router Principal"
                class="w-full px-4 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-slate-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent"
              />
            </div>

            <!-- Location -->
            <div>
              <label for="location" class="block text-sm font-medium text-zinc-300 mb-2">
                Localização
              </label>
              <input
                id="location"
                v-model="form.location"
                type="text"
                placeholder="Datacenter SP1 - Rack 42"
                class="w-full px-4 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-slate-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent"
              />
              <p class="mt-1 text-xs text-zinc-500">POP, datacenter, sala, rack, etc.</p>
            </div>

            <!-- Description -->
            <div>
              <label for="description" class="block text-sm font-medium text-zinc-300 mb-2">
                Descrição
              </label>
              <textarea
                id="description"
                v-model="form.description"
                rows="3"
                placeholder="Informações adicionais sobre o exporter..."
                class="w-full px-4 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-slate-100 placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent resize-none"
              ></textarea>
            </div>

            <!-- Enabled -->
            <div class="flex items-center gap-3">
              <input
                id="enabled"
                v-model="form.enabled"
                type="checkbox"
                class="w-5 h-5 rounded border-zinc-700 bg-zinc-800 text-emerald-500 focus:ring-2 focus:ring-emerald-500 focus:ring-offset-0 focus:ring-offset-zinc-900"
              />
              <label for="enabled" class="text-sm font-medium text-zinc-300">
                Exporter ativo (aceitar flows deste IP)
              </label>
            </div>

            <!-- Actions -->
            <div class="flex gap-3 justify-end pt-4 border-t border-zinc-800">
              <button
                type="button"
                @click="goBack"
                class="px-6 py-2 bg-zinc-800 hover:bg-zinc-700 rounded-lg transition-colors"
              >
                Cancelar
              </button>
              <button
                type="submit"
                :disabled="saving"
                class="flex items-center gap-2 px-6 py-2 bg-emerald-500 hover:bg-emerald-600 rounded-lg font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Save class="w-4 h-4" />
                {{ saving ? 'Salvando...' : (isEdit ? 'Atualizar' : 'Criar') }}
              </button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  </div>
</template>
