<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { useUsersStore } from '../stores/users'
import { Save, Shield } from 'lucide-vue-next'

const route = useRoute()
const router = useRouter()
const authStore = useAuthStore()
const usersStore = useUsersStore()

const isEditing = computed(() => route.name === 'EditUser')
const userId = computed(() => Number(route.params.id))

const formData = ref({
  username: '',
  password: '',
  is_admin: false,
})

const isSaving = ref(false)
const errorMessage = ref('')

onMounted(async () => {
  if (!authStore.isAdmin) {
    router.push('/dashboard')
    return
  }

  if (isEditing.value) {
    await usersStore.loadUsers()
    const user = usersStore.getUser(userId.value)
    if (user) {
      formData.value.username = user.username
      formData.value.is_admin = user.is_admin
    } else {
      router.push('/users')
    }
  }
})

async function handleSave() {
  isSaving.value = true
  errorMessage.value = ''

  try {
    let success = false
    if (isEditing.value) {
      success = await usersStore.updateUser(
        userId.value,
        formData.value.username,
        formData.value.password || null, // Se vazio, envia null
        formData.value.is_admin
      )
    } else {
      if (!formData.value.password) {
        errorMessage.value = 'A senha é obrigatória para novos usuários.'
        isSaving.value = false
        return
      }
      success = await usersStore.createUser(
        formData.value.username,
        formData.value.password,
        formData.value.is_admin
      )
    }

    if (success) {
      router.push('/users')
    } else {
      errorMessage.value = usersStore.error || 'Erro ao salvar usuário'
    }
  } finally {
    isSaving.value = false
  }
}
</script>

<template>
  <div class="p-8 flex items-center justify-center">
    <div class="w-full max-w-lg space-y-8">
      
      <!-- Header -->
      <header class="flex items-center gap-4">
        <div>
          <h1 class="text-2xl font-bold tracking-tight">
            {{ isEditing ? 'Editar Usuário' : 'Novo Usuário' }}
          </h1>
          <p class="text-zinc-400 text-sm mt-1">
            {{ isEditing ? 'Altere as credenciais e acessos do usuário.' : 'Crie um novo acesso à plataforma.' }}
          </p>
        </div>
      </header>

      <!-- Form -->
      <form @submit.prevent="handleSave" class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 shadow-xl space-y-6">
        
        <div class="space-y-4">
          <!-- Username -->
          <div>
            <label class="block text-sm font-medium text-zinc-300 mb-2">Nome de Usuário</label>
            <input
              v-model="formData.username"
              type="text"
              required
              class="w-full px-4 py-2.5 bg-zinc-950 border border-zinc-800 rounded-lg text-slate-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
              placeholder="ex: joao.silva"
            />
          </div>

          <!-- Password -->
          <div>
            <label class="block text-sm font-medium text-zinc-300 mb-2">
              Senha 
              <span v-if="isEditing" class="text-zinc-500 font-normal ml-1">(deixe em branco para manter a atual)</span>
            </label>
            <input
              v-model="formData.password"
              type="password"
              :required="!isEditing"
              class="w-full px-4 py-2.5 bg-zinc-950 border border-zinc-800 rounded-lg text-slate-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
              placeholder="••••••••"
            />
          </div>

          <!-- Admin Checkbox -->
          <div class="pt-2">
            <label class="flex items-start gap-3 p-4 rounded-lg border border-zinc-800 bg-zinc-950 cursor-pointer hover:border-zinc-700 transition-colors">
              <div class="flex items-center h-5">
                <input
                  v-model="formData.is_admin"
                  type="checkbox"
                  class="w-4 h-4 text-emerald-500 bg-zinc-900 border-zinc-700 rounded focus:ring-emerald-500/50 focus:ring-2"
                />
              </div>
              <div class="flex flex-col">
                <span class="text-sm font-medium text-slate-200 flex items-center gap-2">
                  <Shield class="w-4 h-4 text-emerald-500" />
                  Privilégio de Administrador
                </span>
                <span class="text-xs text-zinc-500 mt-1">
                  Permite cadastrar exporters e gerenciar outros usuários do sistema.
                </span>
              </div>
            </label>
          </div>
        </div>

        <div v-if="errorMessage" class="p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
          {{ errorMessage }}
        </div>

        <!-- Actions -->
        <div class="pt-4 flex justify-end gap-3 border-t border-zinc-800">
          <button
            type="button"
            @click="router.push('/users')"
            class="px-4 py-2 text-sm font-medium text-zinc-400 hover:text-slate-200 hover:bg-zinc-800 rounded-lg transition-colors"
          >
            Cancelar
          </button>
          <button
            type="submit"
            :disabled="isSaving"
            class="flex items-center gap-2 px-6 py-2 bg-emerald-500 hover:bg-emerald-600 disabled:opacity-50 disabled:hover:bg-emerald-500 text-white font-medium rounded-lg transition-colors shadow-lg shadow-emerald-500/20"
          >
            <Save class="w-4 h-4" />
            {{ isSaving ? 'Salvando...' : 'Salvar Usuário' }}
          </button>
        </div>

      </form>
    </div>
  </div>
</template>
