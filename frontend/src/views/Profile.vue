<script setup lang="ts">
import { ref } from 'vue'
import { useAuthStore } from '../stores/auth'
import { KeyRound, Shield, User as UserIcon, Check, AlertCircle } from 'lucide-vue-next'

const authStore = useAuthStore()

const currentPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const saving = ref(false)
const message = ref<{ type: 'success' | 'error'; text: string } | null>(null)

async function handleSave() {
  message.value = null

  if (!currentPassword.value || !newPassword.value) {
    message.value = { type: 'error', text: 'Preencha a senha atual e a nova senha.' }
    return
  }
  if (newPassword.value !== confirmPassword.value) {
    message.value = { type: 'error', text: 'A nova senha e a confirmação não coincidem.' }
    return
  }
  if (newPassword.value.length < 6) {
    message.value = { type: 'error', text: 'A nova senha deve ter pelo menos 6 caracteres.' }
    return
  }

  saving.value = true
  try {
    // Re-authenticate to validate current password
    await authStore.login(authStore.user!.username, currentPassword.value)

    // Update password via PUT /api/users/:id
    const res = await fetch(`/api/users/${authStore.user!.id}`, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        ...authStore.getAuthHeaders(),
      },
      body: JSON.stringify({
        username: authStore.user!.username,
        password: newPassword.value,
        is_admin: authStore.user!.is_admin,
      }),
    })

    if (res.ok) {
      message.value = { type: 'success', text: 'Senha alterada com sucesso.' }
      currentPassword.value = ''
      newPassword.value = ''
      confirmPassword.value = ''
    } else {
      message.value = { type: 'error', text: 'Não foi possível salvar a nova senha.' }
    }
  } catch {
    message.value = { type: 'error', text: 'Senha atual incorreta.' }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="p-8 flex items-start justify-center">
    <div class="w-full max-w-lg space-y-6">

      <!-- Header -->
      <header class="pb-4 border-b border-zinc-800">
        <h1 class="text-2xl font-bold tracking-tight flex items-center gap-2">
          <UserIcon class="w-5 h-5 text-emerald-500" />
          Meu Perfil
        </h1>
        <p class="text-zinc-400 text-sm mt-1">Informações da sua conta e segurança.</p>
      </header>

      <!-- Account info card -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 flex items-center gap-4">
        <div class="w-12 h-12 rounded-full bg-zinc-800 border border-zinc-700 flex items-center justify-center text-zinc-400 flex-shrink-0">
          <UserIcon class="w-6 h-6" />
        </div>
        <div class="flex-1 min-w-0">
          <p class="font-semibold text-slate-100 text-lg truncate">{{ authStore.user?.username }}</p>
          <span
            class="inline-flex items-center gap-1 text-xs font-medium px-2 py-0.5 rounded-md border mt-1"
            :class="authStore.isAdmin
              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
              : 'bg-zinc-800 text-zinc-400 border-zinc-700'"
          >
            <Shield class="w-3 h-3" />
            {{ authStore.isAdmin ? 'Administrador' : 'Usuário padrão' }}
          </span>
        </div>
      </div>

      <!-- Change password form -->
      <form @submit.prevent="handleSave" class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-5">
        <div class="flex items-center gap-2 mb-1">
          <KeyRound class="w-4 h-4 text-zinc-400" />
          <h2 class="font-semibold text-slate-200">Alterar senha</h2>
        </div>

        <div>
          <label class="block text-sm font-medium text-zinc-300 mb-1.5">Senha atual</label>
          <input
            v-model="currentPassword"
            type="password"
            autocomplete="current-password"
            required
            class="w-full px-4 py-2.5 bg-zinc-950 border border-zinc-800 rounded-lg text-slate-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
            placeholder="••••••••"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-zinc-300 mb-1.5">Nova senha</label>
          <input
            v-model="newPassword"
            type="password"
            autocomplete="new-password"
            required
            class="w-full px-4 py-2.5 bg-zinc-950 border border-zinc-800 rounded-lg text-slate-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
            placeholder="Mínimo 6 caracteres"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-zinc-300 mb-1.5">Confirmar nova senha</label>
          <input
            v-model="confirmPassword"
            type="password"
            autocomplete="new-password"
            required
            class="w-full px-4 py-2.5 bg-zinc-950 border border-zinc-800 rounded-lg text-slate-100 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
            :class="confirmPassword && confirmPassword !== newPassword ? 'border-red-500/50 focus:ring-red-500/50' : ''"
            placeholder="••••••••"
          />
          <p v-if="confirmPassword && confirmPassword !== newPassword" class="text-xs text-red-400 mt-1">
            As senhas não coincidem.
          </p>
        </div>

        <!-- Feedback -->
        <transition
          enter-active-class="transition duration-200 ease-out"
          enter-from-class="opacity-0 -translate-y-1"
          enter-to-class="opacity-100 translate-y-0"
        >
          <div
            v-if="message"
            class="flex items-center gap-2 p-3 rounded-lg text-sm border"
            :class="message.type === 'success'
              ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400'
              : 'bg-red-500/10 border-red-500/20 text-red-400'"
          >
            <Check v-if="message.type === 'success'" class="w-4 h-4 flex-shrink-0" />
            <AlertCircle v-else class="w-4 h-4 flex-shrink-0" />
            {{ message.text }}
          </div>
        </transition>

        <div class="pt-2 flex justify-end border-t border-zinc-800">
          <button
            type="submit"
            :disabled="saving || (!!confirmPassword && confirmPassword !== newPassword)"
            class="flex items-center gap-2 px-6 py-2 bg-emerald-500 hover:bg-emerald-600 disabled:opacity-40 disabled:cursor-not-allowed text-white font-medium rounded-lg transition-colors shadow-lg shadow-emerald-500/20"
          >
            <KeyRound class="w-4 h-4" />
            {{ saving ? 'Salvando…' : 'Alterar senha' }}
          </button>
        </div>
      </form>

    </div>
  </div>
</template>
