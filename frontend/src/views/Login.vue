<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'

const router = useRouter()
const authStore = useAuthStore()

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

async function handleLogin() {
  error.value = ''
  loading.value = true

  try {
    await authStore.login(username.value, password.value)
    router.push('/dashboard')
  } catch (e) {
    error.value = 'Usuário ou senha inválidos'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="min-h-screen bg-zinc-950 flex items-center justify-center p-4">
    <Card class="w-full max-w-md bg-zinc-900 border-zinc-800">
      <CardHeader class="text-center">
        <CardTitle class="text-2xl font-bold">Flow Collector</CardTitle>
        <p class="text-zinc-400 text-sm mt-2">Faça login para acessar o dashboard</p>
      </CardHeader>
      <CardContent>
        <form @submit.prevent="handleLogin" class="space-y-4">
          <div>
            <label for="username" class="block text-sm font-medium text-zinc-300 mb-2">
              Usuário
            </label>
            <input
              id="username"
              v-model="username"
              type="text"
              required
              class="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500"
              placeholder="Digite seu usuário"
            />
          </div>

          <div>
            <label for="password" class="block text-sm font-medium text-zinc-300 mb-2">
              Senha
            </label>
            <input
              id="password"
              v-model="password"
              type="password"
              required
              class="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500"
              placeholder="Digite sua senha"
            />
          </div>

          <div v-if="error" class="text-red-400 text-sm">
            {{ error }}
          </div>

          <button
            type="submit"
            :disabled="loading"
            class="w-full py-2 px-4 bg-emerald-500 hover:bg-emerald-600 text-white font-medium rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {{ loading ? 'Entrando...' : 'Entrar' }}
          </button>

          <div class="text-center text-xs text-zinc-500 mt-4">
            Usuário padrão: <strong>admin</strong> | Senha: <strong>admin123</strong>
          </div>
        </form>
      </CardContent>
    </Card>
  </div>
</template>
