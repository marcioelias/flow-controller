import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useAuthStore } from './auth'

export interface User {
  id: number
  username: string
  is_admin: boolean
}

export const useUsersStore = defineStore('users', () => {
  const authStore = useAuthStore()

  // State
  const users = ref<User[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  // Actions
  async function loadUsers() {
    if (!authStore.isAdmin) return

    loading.value = true
    error.value = null

    try {
      const res = await fetch('/api/users', {
        headers: authStore.getAuthHeaders()
      })

      if (res.ok) {
        users.value = await res.json()
      } else {
        error.value = 'Falha ao carregar usuários'
      }
    } catch (e) {
      error.value = 'Erro ao carregar usuários'
      console.error('Failed to load users', e)
    } finally {
      loading.value = false
    }
  }

  async function createUser(username: string, password: string, isAdmin: boolean) {
    loading.value = true
    error.value = null

    try {
      const res = await fetch('/api/users', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...authStore.getAuthHeaders()
        },
        body: JSON.stringify({
          username,
          password,
          is_admin: isAdmin
        })
      })

      if (res.ok) {
        await loadUsers()
        return true
      } else {
        error.value = 'Falha ao criar usuário (username já existe?)'
        return false
      }
    } catch (e) {
      error.value = 'Erro ao criar usuário'
      console.error('Failed to create user', e)
      return false
    } finally {
      loading.value = false
    }
  }

  async function deleteUser(userId: number) {
    loading.value = true
    error.value = null

    try {
      const res = await fetch(`/api/users/${userId}`, {
        method: 'DELETE',
        headers: authStore.getAuthHeaders()
      })

      if (res.ok) {
        await loadUsers()
        return true
      } else {
        error.value = 'Falha ao deletar usuário'
        return false
      }
    } catch (e) {
      error.value = 'Erro ao deletar usuário'
      console.error('Failed to delete user', e)
      return false
    } finally {
      loading.value = false
    }
  }

  async function updateUser(userId: number, username: string, password: string | null, isAdmin: boolean) {
    loading.value = true
    error.value = null

    try {
      const payload: any = { username, is_admin: isAdmin }
      if (password) {
        payload.password = password
      }

      const res = await fetch(`/api/users/${userId}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          ...authStore.getAuthHeaders()
        },
        body: JSON.stringify(payload)
      })

      if (res.ok) {
        await loadUsers()
        return true
      } else {
        error.value = 'Falha ao atualizar usuário'
        return false
      }
    } catch (e) {
      error.value = 'Erro ao atualizar usuário'
      console.error('Failed to update user', e)
      return false
    } finally {
      loading.value = false
    }
  }

  function getUser(userId: number): User | undefined {
    return users.value.find(u => u.id === userId)
  }

  return {
    users,
    loading,
    error,
    loadUsers,
    createUser,
    deleteUser,
    updateUser,
    getUser,
  }
})
