<script setup lang="ts">
import { ref } from 'vue'
import { useAuthStore } from '../stores/auth'
import { DatabaseBackup, Download, Upload, AlertTriangle, CheckCircle, XCircle } from 'lucide-vue-next'

const authStore = useAuthStore()

// --- Flows size estimate ---
interface FlowsSize {
  v4_rows: number
  v6_rows: number
  estimated_bytes_uncompressed: number
  estimated_bytes_compressed: number
  warning: string
}

const flowsSize = ref<FlowsSize | null>(null)
const flowsSizeLoading = ref(false)
const showFlowsModal = ref(false)

// --- Restore result ---
interface RestoreResult {
  users_imported: number
  users_skipped: number
  exporters_imported: number
  exporters_replaced: number
  settings_imported: number
  license: string
}

const restoreResult = ref<RestoreResult | null>(null)
const restoreError = ref('')
const restoreLoading = ref(false)
const dragOver = ref(false)
const selectedFile = ref<File | null>(null)

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
}

function formatNumber(n: number): string {
  return new Intl.NumberFormat('pt-BR').format(n)
}

// --- Config download ---
function downloadConfig() {
  const a = document.createElement('a')
  a.href = '/api/backup/config'
  a.setAttribute('Authorization', `Bearer ${authStore.token}`)
  // Use fetch + blob to send auth header
  fetch('/api/backup/config', {
    headers: { Authorization: `Bearer ${authStore.token}` },
  })
    .then(r => {
      const cd = r.headers.get('content-disposition') || ''
      const match = cd.match(/filename="?([^"]+)"?/)
      const filename = match ? match[1] : 'flow-collector-config.json'
      return r.blob().then(blob => ({ blob, filename }))
    })
    .then(({ blob, filename }) => {
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = filename
      document.body.appendChild(link)
      link.click()
      link.remove()
      URL.revokeObjectURL(url)
    })
    .catch(() => {})
}

// --- Flows download flow ---
async function checkFlowsSize() {
  flowsSizeLoading.value = true
  flowsSize.value = null
  try {
    const res = await fetch('/api/backup/flows/size', {
      headers: { Authorization: `Bearer ${authStore.token}` },
    })
    flowsSize.value = await res.json()
    showFlowsModal.value = true
  } catch {
    /* silently ignore */
  } finally {
    flowsSizeLoading.value = false
  }
}

function confirmFlowsDownload() {
  showFlowsModal.value = false
  fetch('/api/backup/flows', {
    headers: { Authorization: `Bearer ${authStore.token}` },
  })
    .then(r => {
      const cd = r.headers.get('content-disposition') || ''
      const match = cd.match(/filename="?([^"]+)"?/)
      const filename = match ? match[1] : 'flows.ndjson'
      return r.blob().then(blob => ({ blob, filename }))
    })
    .then(({ blob, filename }) => {
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = filename
      document.body.appendChild(link)
      link.click()
      link.remove()
      URL.revokeObjectURL(url)
    })
    .catch(() => {})
}

// --- File drag & drop ---
function onDrop(e: DragEvent) {
  dragOver.value = false
  const file = e.dataTransfer?.files[0]
  if (file && file.name.endsWith('.json')) {
    selectedFile.value = file
    restoreResult.value = null
    restoreError.value = ''
  }
}

function onFileInput(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (file) {
    selectedFile.value = file
    restoreResult.value = null
    restoreError.value = ''
  }
}

// --- Restore ---
async function doRestore() {
  if (!selectedFile.value) return
  restoreLoading.value = true
  restoreResult.value = null
  restoreError.value = ''

  // Quick client-side validation
  try {
    const text = await selectedFile.value.text()
    const parsed = JSON.parse(text)
    if (!parsed.version || !parsed.exported_at) {
      restoreError.value = 'Arquivo inválido: campos obrigatórios ausentes.'
      return
    }
  } catch {
    restoreError.value = 'Arquivo JSON inválido.'
    return
  } finally {
    if (restoreError.value) {
      restoreLoading.value = false
      return
    }
  }

  const form = new FormData()
  form.append('file', selectedFile.value)

  try {
    const res = await fetch('/api/restore/config', {
      method: 'POST',
      headers: { Authorization: `Bearer ${authStore.token}` },
      body: form,
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: 'Erro desconhecido' }))
      restoreError.value = body.error || `Erro ${res.status}`
    } else {
      restoreResult.value = await res.json()
    }
  } catch (e: unknown) {
    restoreError.value = e instanceof Error ? e.message : 'Erro de rede'
  } finally {
    restoreLoading.value = false
  }
}
</script>

<template>
  <div class="p-6 space-y-6 max-w-3xl mx-auto">
    <!-- Header -->
    <div class="flex items-center gap-3">
      <DatabaseBackup class="w-6 h-6 text-emerald-500" />
      <h1 class="text-2xl font-bold text-slate-100">Backup & Restauração</h1>
    </div>

    <!-- Export section -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-6 space-y-4">
      <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Exportar</h2>

      <!-- Config download -->
      <div class="flex items-start justify-between gap-4">
        <div>
          <p class="text-slate-200 font-medium">Configuração</p>
          <p class="text-sm text-zinc-400 mt-0.5">Usuários, exporters, settings e licença — arquivo JSON portável</p>
        </div>
        <button
          @click="downloadConfig"
          class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-sm font-medium transition-colors flex-shrink-0"
        >
          <Download class="w-4 h-4" />
          Baixar config
        </button>
      </div>

      <div class="border-t border-zinc-800" />

      <!-- Flows download -->
      <div class="flex items-start justify-between gap-4">
        <div>
          <p class="text-slate-200 font-medium">Flows <span class="text-xs text-zinc-500 ml-1">(opcional)</span></p>
          <p class="text-sm text-zinc-400 mt-0.5">Export completo das tabelas ClickHouse em NDJSON — pode ser grande</p>
        </div>
        <button
          @click="checkFlowsSize"
          :disabled="flowsSizeLoading"
          class="flex items-center gap-2 px-4 py-2 rounded-lg bg-zinc-700 hover:bg-zinc-600 disabled:opacity-50 text-slate-200 text-sm font-medium transition-colors flex-shrink-0"
        >
          <svg v-if="flowsSizeLoading" class="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/>
          </svg>
          <Download v-else class="w-4 h-4" />
          Baixar flows
        </button>
      </div>
    </div>

    <!-- Flows size confirmation modal -->
    <Teleport to="body">
      <div
        v-if="showFlowsModal"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
        @click.self="showFlowsModal = false"
      >
        <div class="bg-zinc-900 border border-zinc-700 rounded-xl shadow-2xl p-6 w-full max-w-md mx-4 space-y-4">
          <div class="flex items-center gap-3">
            <AlertTriangle class="w-5 h-5 text-amber-400 flex-shrink-0" />
            <h3 class="font-semibold text-slate-100">Confirmar download de flows</h3>
          </div>
          <div v-if="flowsSize" class="space-y-2 text-sm">
            <div class="flex justify-between">
              <span class="text-zinc-400">Rows IPv4</span>
              <span class="font-mono text-slate-200">{{ formatNumber(flowsSize.v4_rows) }}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-zinc-400">Rows IPv6</span>
              <span class="font-mono text-slate-200">{{ formatNumber(flowsSize.v6_rows) }}</span>
            </div>
            <div class="flex justify-between border-t border-zinc-800 pt-2">
              <span class="text-zinc-400">Tamanho estimado</span>
              <span class="font-mono text-amber-400">{{ formatBytes(flowsSize.estimated_bytes_compressed) }}</span>
            </div>
            <p class="text-zinc-400 pt-1">{{ flowsSize.warning }}</p>
          </div>
          <div class="flex gap-3 pt-2">
            <button
              @click="showFlowsModal = false"
              class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors"
            >
              Cancelar
            </button>
            <button
              @click="confirmFlowsDownload"
              class="flex-1 px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 text-white text-sm font-medium transition-colors"
            >
              Confirmar download
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Restore section -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-6 space-y-4">
      <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Importar configuração</h2>

      <!-- Drop zone -->
      <div
        class="border-2 border-dashed rounded-xl p-8 text-center transition-all cursor-pointer"
        :class="dragOver
          ? 'border-emerald-500 bg-emerald-500/5'
          : 'border-zinc-700 hover:border-zinc-500'"
        @dragover.prevent="dragOver = true"
        @dragleave="dragOver = false"
        @drop.prevent="onDrop"
        @click="($refs.fileInput as HTMLInputElement).click()"
      >
        <input
          ref="fileInput"
          type="file"
          accept=".json"
          class="hidden"
          @change="onFileInput"
        />
        <Upload class="w-8 h-8 mx-auto mb-2 text-zinc-500" />
        <p v-if="selectedFile" class="text-slate-200 font-medium">{{ selectedFile.name }}</p>
        <p v-else class="text-zinc-400 text-sm">
          Arraste o arquivo JSON ou clique para selecionar
        </p>
        <p v-if="selectedFile" class="text-xs text-zinc-500 mt-1">
          {{ formatBytes(selectedFile.size) }} — clique para trocar
        </p>
      </div>

      <button
        @click="doRestore"
        :disabled="!selectedFile || restoreLoading"
        class="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors"
      >
        <svg v-if="restoreLoading" class="animate-spin w-4 h-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z"/>
        </svg>
        <Upload v-else class="w-4 h-4" />
        Importar configuração
      </button>

      <!-- Error -->
      <div v-if="restoreError" class="flex items-start gap-2 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-sm text-red-400">
        <XCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
        {{ restoreError }}
      </div>

      <!-- Result -->
      <div v-if="restoreResult" class="space-y-2">
        <div class="flex items-start gap-2 text-sm text-emerald-400">
          <CheckCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
          <span>{{ restoreResult.users_imported }} usuário(s) importado(s)
            <span v-if="restoreResult.users_skipped" class="text-zinc-400">, {{ restoreResult.users_skipped }} ignorado(s) (já existia)</span>
          </span>
        </div>
        <div class="flex items-start gap-2 text-sm text-emerald-400">
          <CheckCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
          <span>{{ restoreResult.exporters_imported + restoreResult.exporters_replaced }} exporter(s) importado(s)
            <span v-if="restoreResult.exporters_replaced" class="text-zinc-400">({{ restoreResult.exporters_replaced }} substituído(s))</span>
          </span>
        </div>
        <div v-if="restoreResult.settings_imported > 0" class="flex items-start gap-2 text-sm text-emerald-400">
          <CheckCircle class="w-4 h-4 flex-shrink-0 mt-0.5" />
          <span>{{ restoreResult.settings_imported }} setting(s) restaurada(s)</span>
        </div>
        <div class="flex items-start gap-2 text-sm text-amber-400">
          <AlertTriangle class="w-4 h-4 flex-shrink-0 mt-0.5" />
          <span>{{ restoreResult.license }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
