<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ShieldCheck, ShieldX, Copy, Check, KeyRound, RefreshCw } from 'lucide-vue-next'

interface LicenseStatus {
  valid: boolean
  licensee: string | null
  tier_label: string
  max_bps: number | null
  max_talkers: number | null
  expires_at: string | null
  fingerprint: string
  message: string | null
}

const status = ref<LicenseStatus | null>(null)
const loading = ref(false)
const loadError = ref<string | null>(null)

const licenseInput = ref('')
const applying = ref(false)
const applySuccess = ref(false)
const applyError = ref<string | null>(null)
let successTimer: ReturnType<typeof setTimeout> | null = null

const copied = ref(false)
let copyTimer: ReturnType<typeof setTimeout> | null = null

const isFree = (label: string) => label.toLowerCase().startsWith('free')

async function fetchStatus() {
  loading.value = true
  loadError.value = null
  try {
    const res = await fetch('/api/license')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    status.value = await res.json()
  } catch (e: any) {
    loadError.value = e?.message ?? 'Failed to load license status'
  } finally {
    loading.value = false
  }
}

async function applyLicense() {
  if (!licenseInput.value.trim()) return
  applying.value = true
  applySuccess.value = false
  applyError.value = null
  try {
    const res = await fetch('/api/license', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ license: licenseInput.value.trim() }),
    })
    const data: LicenseStatus = await res.json()
    if (!res.ok || !data.valid) {
      applyError.value = data.message ?? `HTTP ${res.status}`
    } else {
      status.value = data
      licenseInput.value = ''
      applySuccess.value = true
      if (successTimer) clearTimeout(successTimer)
      successTimer = setTimeout(() => {
        applySuccess.value = false
      }, 4000)
    }
  } catch (e: any) {
    applyError.value = e?.message ?? 'Network error'
  } finally {
    applying.value = false
  }
}

function copyFingerprint() {
  if (!status.value) return
  navigator.clipboard.writeText(status.value.fingerprint).then(() => {
    copied.value = true
    if (copyTimer) clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copied.value = false
    }, 2000)
  })
}

function formatExpiry(expires_at: string | null): string {
  if (!expires_at) return 'Perpétua'
  return new Date(expires_at).toLocaleDateString('pt-BR', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })
}

onMounted(() => {
  fetchStatus()
})
</script>

<template>
  <div class="p-8">
    <div class="max-w-3xl mx-auto space-y-6">

      <!-- Header -->
      <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold tracking-tight flex items-center gap-2">
            <KeyRound class="w-6 h-6 text-emerald-500" />
            Licença
          </h1>
          <p class="text-zinc-400 mt-1">Gerencie a licença de uso do Flow Collector</p>
        </div>
        <button
          @click="fetchStatus"
          :disabled="loading"
          class="p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors"
        >
          <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': loading }" />
        </button>
      </header>

      <!-- Loading state -->
      <div v-if="loading && !status" class="bg-zinc-900 border border-zinc-800 rounded-xl flex items-center justify-center py-16 text-zinc-500">
        <RefreshCw class="w-5 h-5 animate-spin mr-2" /> Carregando…
      </div>

      <!-- Load error -->
      <div v-else-if="loadError" class="bg-zinc-900 border border-red-800/50 rounded-xl p-6 text-red-400 flex items-center gap-3">
        <ShieldX class="w-5 h-5 flex-shrink-0" />
        <span>Erro ao carregar status da licença: {{ loadError }}</span>
      </div>

      <!-- License status card -->
      <div v-else-if="status" class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <!-- Card header -->
        <div class="flex items-center justify-between px-6 py-4 border-b border-zinc-800">
          <h2 class="text-base font-semibold text-zinc-300">Status da Licença</h2>
          <!-- Tier badge -->
          <span
            class="px-3 py-1 rounded-full text-xs font-semibold tracking-wide"
            :class="isFree(status.tier_label)
              ? 'bg-zinc-800 text-zinc-400 border border-zinc-700'
              : 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30'"
          >
            {{ isFree(status.tier_label) ? 'Free' : 'Licenciado' }}
          </span>
        </div>

        <!-- Card body -->
        <div class="px-6 py-5 space-y-4">
          <!-- Status row -->
          <div class="flex items-center gap-3">
            <ShieldCheck v-if="status.valid" class="w-5 h-5 text-emerald-400 flex-shrink-0" />
            <ShieldX v-else class="w-5 h-5 text-red-400 flex-shrink-0" />
            <span
              class="text-sm font-semibold"
              :class="status.valid ? 'text-emerald-400' : 'text-red-400'"
            >
              {{ status.valid ? 'Válida' : 'Inválida' }}
            </span>
            <span v-if="!status.valid && status.message" class="text-sm text-red-300/70">
              — {{ status.message }}
            </span>
          </div>

          <!-- Details grid -->
          <dl class="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-3 text-sm">
            <div v-if="status.licensee">
              <dt class="text-zinc-500 font-medium mb-0.5">Licenciado</dt>
              <dd class="text-slate-100">{{ status.licensee }}</dd>
            </div>

            <div>
              <dt class="text-zinc-500 font-medium mb-0.5">Plano</dt>
              <dd class="text-slate-100">{{ status.tier_label }}</dd>
            </div>

            <div>
              <dt class="text-zinc-500 font-medium mb-0.5">Validade</dt>
              <dd class="text-slate-100">{{ formatExpiry(status.expires_at) }}</dd>
            </div>

            <div v-if="status.max_bps !== null">
              <dt class="text-zinc-500 font-medium mb-0.5">Largura de banda máx.</dt>
              <dd class="text-slate-100">{{ (status.max_bps / 1e9).toFixed(1) }} Gbps</dd>
            </div>

            <div v-if="status.max_talkers !== null">
              <dt class="text-zinc-500 font-medium mb-0.5">Talkers máx.</dt>
              <dd class="text-slate-100">{{ status.max_talkers }}</dd>
            </div>
          </dl>

          <!-- Fingerprint section -->
          <div class="pt-3 border-t border-zinc-800">
            <p class="text-xs font-semibold text-zinc-600 uppercase tracking-wider mb-2">Fingerprint da máquina</p>
            <div class="flex items-center gap-3 bg-zinc-950 border border-zinc-800 rounded-lg px-4 py-2.5">
              <code class="flex-1 font-mono text-sm text-zinc-300 truncate">
                {{ status.fingerprint.slice(0, 16) }}…
              </code>
              <button
                @click="copyFingerprint"
                class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all duration-200"
                :class="copied
                  ? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30'
                  : 'bg-zinc-800 text-zinc-400 border border-zinc-700 hover:text-zinc-200 hover:border-zinc-600'"
              >
                <Check v-if="copied" class="w-3.5 h-3.5" />
                <Copy v-else class="w-3.5 h-3.5" />
                {{ copied ? 'Copiado!' : 'Copiar' }}
              </button>
            </div>
            <p class="text-xs text-zinc-600 mt-1.5">Envie este código ao fornecedor para gerar uma licença.</p>
          </div>
        </div>
      </div>

      <!-- Apply license card -->
      <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden">
        <div class="px-6 py-4 border-b border-zinc-800">
          <h2 class="text-base font-semibold text-zinc-300">Aplicar Licença</h2>
        </div>

        <div class="px-6 py-5 space-y-4">
          <p class="text-sm text-zinc-400">Cole a string de licença recebida do fornecedor:</p>

          <textarea
            v-model="licenseInput"
            rows="4"
            placeholder="eyJmaW5nZXJwcmludCI6ImEzZjh..."
            class="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-4 py-3 font-mono text-sm text-slate-100 placeholder-zinc-600 resize-none focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500/50 transition-colors"
            :disabled="applying"
          />

          <!-- Feedback messages -->
          <div v-if="applySuccess" class="flex items-center gap-2 text-sm text-emerald-400">
            <Check class="w-4 h-4 flex-shrink-0" />
            Licença aplicada com sucesso!
          </div>
          <div v-if="applyError" class="flex items-center gap-2 text-sm text-red-400">
            <ShieldX class="w-4 h-4 flex-shrink-0" />
            Erro: {{ applyError }}
          </div>

          <button
            @click="applyLicense"
            :disabled="applying || !licenseInput.trim()"
            class="flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-semibold transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
            :class="applying || !licenseInput.trim()
              ? 'bg-zinc-800 text-zinc-500 cursor-not-allowed'
              : 'bg-emerald-600 hover:bg-emerald-500 text-white'"
          >
            <RefreshCw v-if="applying" class="w-4 h-4 animate-spin" />
            <KeyRound v-else class="w-4 h-4" />
            {{ applying ? 'Aplicando…' : 'Aplicar Licença' }}
          </button>
        </div>
      </div>

    </div>
  </div>
</template>
