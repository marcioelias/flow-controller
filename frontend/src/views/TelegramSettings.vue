<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useAlertsStore } from '../stores/alerts'
import { MessageCircle, Eye, EyeOff, CheckCircle, XCircle } from 'lucide-vue-next'

const store = useAlertsStore()

const botToken = ref('')
const chatId = ref('')
const enabled = ref(false)
const minSeverity = ref('warning')

const showToken = ref(false)
let hideTokenTimer: ReturnType<typeof setTimeout> | null = null

const saveStatus = ref<'idle' | 'saving' | 'saved' | 'error'>('idle')
const testStatus = ref<'idle' | 'sending' | 'ok' | 'error'>('idle')
const testMessage = ref('')

const isTokenMasked = ref(true) // true when the loaded token is the masked placeholder

function toggleToken() {
  showToken.value = !showToken.value
  if (showToken.value) {
    if (hideTokenTimer) clearTimeout(hideTokenTimer)
    hideTokenTimer = setTimeout(() => {
      showToken.value = false
    }, 30_000)
  }
}

async function load() {
  const cfg = await store.loadTelegram()
  if (cfg) {
    botToken.value = cfg.bot_token // masked from server
    chatId.value = cfg.chat_id
    enabled.value = cfg.enabled
    minSeverity.value = cfg.min_severity
    isTokenMasked.value = true
  }
}

async function save() {
  saveStatus.value = 'saving'
  try {
    await store.saveTelegram({
      bot_token: botToken.value,
      chat_id: chatId.value,
      enabled: enabled.value,
      min_severity: minSeverity.value,
    })
    saveStatus.value = 'saved'
    isTokenMasked.value = true
    // Reload to get masked token
    await load()
    setTimeout(() => { saveStatus.value = 'idle' }, 3000)
  } catch (e: unknown) {
    saveStatus.value = 'error'
  }
}

async function sendTest() {
  testStatus.value = 'sending'
  testMessage.value = ''
  try {
    const result = await store.testTelegram()
    testStatus.value = result.ok ? 'ok' : 'error'
    testMessage.value = result.message
  } catch (e: unknown) {
    testStatus.value = 'error'
    testMessage.value = (e as Error).message
  }
  setTimeout(() => { testStatus.value = 'idle' }, 8000)
}

onMounted(load)
</script>

<template>
  <div class="p-6 max-w-2xl mx-auto">
    <div class="flex items-center gap-3 mb-6">
      <MessageCircle class="w-6 h-6 text-sky-400" />
      <h1 class="text-2xl font-bold text-slate-100">Notificações Telegram</h1>
    </div>

    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-6 space-y-5">
      <!-- Bot Token -->
      <div>
        <label class="text-xs text-zinc-400 mb-1.5 block">Bot Token</label>
        <div class="flex gap-2">
          <input
            v-model="botToken"
            :type="showToken ? 'text' : 'password'"
            placeholder="7123456789:AAF..."
            @input="isTokenMasked = false"
            class="flex-1 bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-sky-500"
          />
          <button @click="toggleToken"
            class="px-3 py-2 rounded-lg bg-zinc-800 border border-zinc-700 hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 transition-colors">
            <Eye v-if="!showToken" class="w-4 h-4" />
            <EyeOff v-else class="w-4 h-4" />
          </button>
        </div>
        <p v-if="showToken && isTokenMasked" class="mt-1 text-xs text-zinc-500">
          Token mascarado — insira o token completo para alterar.
        </p>
      </div>

      <!-- Chat ID -->
      <div>
        <label class="text-xs text-zinc-400 mb-1.5 block">Chat ID</label>
        <input v-model="chatId" placeholder="-100123456789"
          class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-sky-500" />
        <p class="mt-1 text-xs text-zinc-500">ID de um grupo ou canal. Use @userinfobot para descobrir.</p>
      </div>

      <!-- Enabled toggle -->
      <div class="flex items-center justify-between">
        <div>
          <div class="text-sm text-slate-200">Notificações ativas</div>
          <div class="text-xs text-zinc-500">Quando desativado, nenhuma mensagem é enviada.</div>
        </div>
        <button @click="enabled = !enabled"
          class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm transition-colors"
          :class="enabled ? 'border-emerald-500/30 bg-emerald-500/5 text-emerald-400' : 'border-zinc-700 text-zinc-500'">
          <span class="w-2 h-2 rounded-full" :class="enabled ? 'bg-emerald-400' : 'bg-zinc-600'"></span>
          {{ enabled ? 'ON' : 'OFF' }}
        </button>
      </div>

      <!-- Min severity -->
      <div>
        <label class="text-xs text-zinc-400 mb-1.5 block">Severidade mínima</label>
        <select v-model="minSeverity"
          class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-sky-500">
          <option value="warning">Warning e acima</option>
          <option value="critical">Apenas Critical</option>
        </select>
      </div>

      <!-- Actions -->
      <div class="flex items-center gap-3 pt-2">
        <button @click="save" :disabled="saveStatus === 'saving'"
          class="flex-1 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors">
          <span v-if="saveStatus === 'saving'">Salvando…</span>
          <span v-else>Salvar</span>
        </button>
        <button @click="sendTest" :disabled="testStatus === 'sending'"
          class="px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed text-zinc-200 text-sm font-medium transition-colors border border-zinc-700">
          <span v-if="testStatus === 'sending'">Enviando…</span>
          <span v-else>Enviar teste</span>
        </button>
      </div>

      <!-- Feedback messages -->
      <div v-if="saveStatus === 'saved'"
        class="flex items-center gap-2 text-xs text-emerald-400 px-1">
        <CheckCircle class="w-3.5 h-3.5" /> Configurações salvas com sucesso.
      </div>
      <div v-if="saveStatus === 'error'"
        class="flex items-center gap-2 text-xs text-red-400 px-1">
        <XCircle class="w-3.5 h-3.5" /> Falha ao salvar. Verifique o console.
      </div>

      <div v-if="testStatus === 'ok'"
        class="flex items-center gap-2 text-xs text-emerald-400 px-1">
        <CheckCircle class="w-3.5 h-3.5" /> {{ testMessage }}
      </div>
      <div v-if="testStatus === 'error'"
        class="flex items-center gap-2 text-xs text-red-400 px-1">
        <XCircle class="w-3.5 h-3.5" /> {{ testMessage }}
      </div>
    </div>
  </div>
</template>
