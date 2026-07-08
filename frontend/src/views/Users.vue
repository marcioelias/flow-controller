<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { useUsersStore } from '../stores/users'
import { Search, Plus, Edit, Trash2, Shield, User as UserIcon } from 'lucide-vue-next'

const router = useRouter()
const authStore = useAuthStore()
const usersStore = useUsersStore()

const searchQuery = ref('')
const sortKey = ref<'username' | 'is_admin' | 'id'>('id')
const sortDesc = ref(false)

onMounted(async () => {
  if (!authStore.isAdmin) {
    router.push('/dashboard')
    return
  }
  await usersStore.loadUsers()
})

const filteredAndSortedUsers = computed(() => {
  let result = usersStore.users

  // Filtro
  if (searchQuery.value) {
    const q = searchQuery.value.toLowerCase()
    result = result.filter(u => u.username.toLowerCase().includes(q))
  }

  // Ordenação
  result = [...result].sort((a, b) => {
    let valA = a[sortKey.value]
    let valB = b[sortKey.value]

    if (valA < valB) return sortDesc.value ? 1 : -1
    if (valA > valB) return sortDesc.value ? -1 : 1
    return 0
  })

  return result
})

function sortBy(key: 'username' | 'is_admin' | 'id') {
  if (sortKey.value === key) {
    sortDesc.value = !sortDesc.value
  } else {
    sortKey.value = key
    sortDesc.value = false
  }
}

async function handleDelete(id: number) {
  if (!confirm('Tem certeza que deseja remover este usuário?')) return
  await usersStore.deleteUser(id)
}
</script>

<template>
  <div class="p-8">
    <div class="max-w-5xl mx-auto space-y-8">
      
      <!-- Header -->
      <header class="flex items-center justify-between">
        <div class="flex items-center gap-4">
          <div>
            <h1 class="text-3xl font-bold tracking-tight">Gerenciar Usuários</h1>
            <p class="text-zinc-400 mt-1">Administre o acesso e privilégios da plataforma.</p>
          </div>
        </div>
        
        <button
          @click="router.push('/users/new')"
          class="flex items-center gap-2 px-4 py-2 bg-emerald-500 hover:bg-emerald-600 text-white font-medium rounded-lg transition-colors shadow-lg shadow-emerald-500/20"
        >
          <Plus class="w-4 h-4" />
          Novo Usuário
        </button>
      </header>

      <!-- Toolbar (Search) -->
      <div class="flex items-center gap-4 bg-zinc-900/50 p-4 rounded-xl border border-zinc-800">
        <div class="relative flex-1 max-w-md">
          <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <input 
            v-model="searchQuery"
            type="text"
            placeholder="Buscar por nome de usuário..."
            class="w-full pl-9 pr-4 py-2 bg-zinc-900 border border-zinc-700 rounded-lg text-sm text-slate-100 placeholder:text-zinc-500 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
          >
        </div>
      </div>

      <!-- Table -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden shadow-xl">
        <div class="overflow-x-auto">
          <table class="w-full text-left text-sm whitespace-nowrap">
            <thead class="bg-zinc-950/50 border-b border-zinc-800 text-zinc-400">
              <tr>
                <th scope="col" class="px-6 py-4 font-semibold cursor-pointer hover:text-emerald-400 select-none" @click="sortBy('username')">
                  <div class="flex items-center gap-2">
                    Usuário
                    <span v-if="sortKey === 'username'" class="text-xs">{{ sortDesc ? '↓' : '↑' }}</span>
                  </div>
                </th>
                <th scope="col" class="px-6 py-4 font-semibold cursor-pointer hover:text-emerald-400 select-none" @click="sortBy('is_admin')">
                  <div class="flex items-center gap-2">
                    Privilégio
                    <span v-if="sortKey === 'is_admin'" class="text-xs">{{ sortDesc ? '↓' : '↑' }}</span>
                  </div>
                </th>
                <th scope="col" class="px-6 py-4 font-semibold cursor-pointer hover:text-emerald-400 select-none" @click="sortBy('id')">
                  <div class="flex items-center gap-2">
                    ID
                    <span v-if="sortKey === 'id'" class="text-xs">{{ sortDesc ? '↓' : '↑' }}</span>
                  </div>
                </th>
                <th scope="col" class="px-6 py-4 font-semibold text-right">Ações</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-zinc-800/50">
              <tr 
                v-for="user in filteredAndSortedUsers" 
                :key="user.id"
                class="hover:bg-zinc-800/30 transition-colors group"
              >
                <td class="px-6 py-4">
                  <div class="flex items-center gap-3">
                    <div class="w-8 h-8 rounded-full bg-zinc-800 flex items-center justify-center text-zinc-400 border border-zinc-700">
                      <UserIcon class="w-4 h-4" />
                    </div>
                    <span class="font-medium text-slate-200">{{ user.username }}</span>
                  </div>
                </td>
                <td class="px-6 py-4">
                  <span 
                    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium border"
                    :class="user.is_admin ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20' : 'bg-zinc-800 text-zinc-400 border-zinc-700'"
                  >
                    <Shield v-if="user.is_admin" class="w-3 h-3" />
                    {{ user.is_admin ? 'Administrador' : 'Padrão' }}
                  </span>
                </td>
                <td class="px-6 py-4 text-zinc-500 font-mono text-xs">
                  #{{ user.id }}
                </td>
                <td class="px-6 py-4 text-right">
                  <div class="flex items-center justify-end gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button 
                      @click="router.push(`/users/${user.id}/edit`)"
                      class="p-1.5 text-zinc-400 hover:text-emerald-400 hover:bg-emerald-400/10 rounded-md transition-colors"
                      title="Editar"
                    >
                      <Edit class="w-4 h-4" />
                    </button>
                    <button 
                      v-if="user.id !== authStore.user?.id"
                      @click="handleDelete(user.id)"
                      class="p-1.5 text-zinc-400 hover:text-red-400 hover:bg-red-400/10 rounded-md transition-colors"
                      title="Excluir"
                    >
                      <Trash2 class="w-4 h-4" />
                    </button>
                  </div>
                </td>
              </tr>
              
              <tr v-if="filteredAndSortedUsers.length === 0">
                <td colspan="4" class="px-6 py-12 text-center text-zinc-500">
                  Nenhum usuário encontrado.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

    </div>
  </div>
</template>
