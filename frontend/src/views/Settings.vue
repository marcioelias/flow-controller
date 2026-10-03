<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import {
  Settings as SettingsIcon, Save, Check, AlertCircle, Loader2, Plus, X,
  Network, Building2, LineChart, Bell, Cpu, Brain, KeyRound, FlaskConical,
} from 'lucide-vue-next'
import { useSettingsStore } from '../stores/settings'
import { useAuthStore } from '../stores/auth'

const store = useSettingsStore()
const authStore = useAuthStore()

// Per-key state
const drafts = ref<Record<string, string>>({})
const saving = ref<Record<string, boolean>>({})
const saved = ref<Record<string, boolean>>({})
const failed = ref<Record<string, boolean>>({})

// Abas por grupo (task 15.4)
const TABS = [
  { name: 'Rede', icon: Network },
  { name: 'Organização', icon: Building2 },
  { name: 'Análise', icon: LineChart },
  { name: 'Alertas', icon: Bell },
  { name: 'Coletor', icon: Cpu },
  { name: 'IA', icon: Brain },
  { name: 'Sessão', icon: KeyRound },
]
const activeTab = ref('Rede')

// Chaves com editor dedicado — ficam fora da listagem genérica
const LIST_KEYS = ['OWN_ASN_LIST', 'INTERNAL_PREFIXES']
const IA_KEYS = ['LLM_ENABLED', 'LLM_ENDPOINT', 'LLM_MODEL']
const LANGUAGE_KEY = 'APP_LANGUAGE'

onMounted(async () => {
  await store.loadSettings()
  for (const s of store.settings) drafts.value[s.key] = s.value
  if (llmEnabled.value && drafts.value['LLM_ENDPOINT']) probeModels()
})

const groups = computed(() => {
  const map = new Map<string, typeof store.settings>()
  for (const s of store.settings) {
    if (!map.has(s.group_name)) map.set(s.group_name, [])
    map.get(s.group_name)!.push(s)
  }
  return map
})

function genericItems(tab: string) {
  const items = groups.value.get(tab) ?? []
  if (tab === 'Rede') return items.filter((s) => !LIST_KEYS.includes(s.key))
  if (tab === 'IA') return items.filter((s) => !IA_KEYS.includes(s.key))
  return items
}

function settingByKey(key: string) {
  return store.settings.find((s) => s.key === key)
}

async function save(key: string) {
  saving.value[key] = true
  saved.value[key] = false
  failed.value[key] = false
  const ok = await store.updateSetting(key, drafts.value[key] ?? '')
  saving.value[key] = false
  if (ok) {
    saved.value[key] = true
    setTimeout(() => (saved.value[key] = false), 2500)
  } else {
    failed.value[key] = true
    setTimeout(() => (failed.value[key] = false), 3000)
  }
}

function isDirty(key: string) {
  const original = store.settings.find((s) => s.key === key)?.value ?? ''
  return drafts.value[key] !== original
}

// ── CRUD de listas (prefixos / ASNs) — storage segue nas mesmas keys CSV ──
const listInput = ref<Record<string, string>>({})

function listItems(key: string): string[] {
  return (drafts.value[key] ?? '')
    .split(',')
    .map((v) => v.trim())
    .filter(Boolean)
}

function validListEntry(key: string, v: string): boolean {
  if (key === 'OWN_ASN_LIST') return /^\d{1,10}$/.test(v)
  // CIDR v4 ou v6 simples
  return /^([0-9a-fA-F:.]+)\/\d{1,3}$/.test(v)
}

async function listAdd(key: string) {
  const v = (listInput.value[key] ?? '').trim()
  if (!v || !validListEntry(key, v)) {
    failed.value[key] = true
    setTimeout(() => (failed.value[key] = false), 2000)
    return
  }
  const items = listItems(key)
  if (!items.includes(v)) items.push(v)
  drafts.value[key] = items.join(',')
  listInput.value[key] = ''
  await save(key)
}

async function listRemove(key: string, v: string) {
  drafts.value[key] = listItems(key)
    .filter((i) => i !== v)
    .join(',')
  await save(key)
}

// ── IA guiada ──────────────────────────────────────────────────────────────
const llmEnabled = computed(() => (drafts.value['LLM_ENABLED'] ?? '') === 'true')
const models = ref<string[]>([])
const probing = ref(false)
const probeError = ref('')
const testResult = ref<{ ok: boolean; text: string } | null>(null)
const testing = ref(false)

async function toggleLlm() {
  drafts.value['LLM_ENABLED'] = llmEnabled.value ? 'false' : 'true'
  await save('LLM_ENABLED')
  if (llmEnabled.value) probeModels()
}

async function probeModels() {
  probing.value = true
  probeError.value = ''
  models.value = []
  try {
    const res = await fetch('/api/llm/models', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${authStore.token}` },
      body: JSON.stringify({ endpoint: drafts.value['LLM_ENDPOINT'] ?? '' }),
    })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    models.value = (await res.json()).models ?? []
    if (models.value.length === 0) probeError.value = 'Endpoint respondeu, mas sem modelos instalados (ollama pull <modelo>).'
  } catch {
    probeError.value = 'Não foi possível alcançar o endpoint — verifique URL e se o Ollama está rodando.'
  } finally {
    probing.value = false
  }
}

async function selectModel(m: string) {
  drafts.value['LLM_MODEL'] = m
  await save('LLM_MODEL')
}

async function saveEndpoint() {
  await save('LLM_ENDPOINT')
  probeModels()
}

async function testLlm() {
  testing.value = true
  testResult.value = null
  try {
    const res = await fetch('/api/llm/test', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${authStore.token}` },
      body: JSON.stringify({ endpoint: drafts.value['LLM_ENDPOINT'] ?? '', model: drafts.value['LLM_MODEL'] ?? '' }),
    })
    const body = await res.json()
    testResult.value = body.ok
      ? { ok: true, text: body.response }
      : { ok: false, text: body.error ?? 'falhou' }
  } catch {
    testResult.value = { ok: false, text: 'Erro de rede ao testar.' }
  } finally {
    testing.value = false
  }
}

const LANGUAGES = [
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
]

async function saveLanguage(v: string) {
  drafts.value[LANGUAGE_KEY] = v
  await save(LANGUAGE_KEY)
}
</script>

<template>
  <div class="p-8">
    <div class="w-full space-y-6">
      <!-- Header -->
      <header class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center">
          <SettingsIcon class="w-5 h-5 text-emerald-400" />
        </div>
        <div>
          <h1 class="text-xl font-bold text-slate-100">Configurações</h1>
          <p class="text-sm text-zinc-500">Parâmetros globais do sistema</p>
        </div>
      </header>

      <!-- Loading / error -->
      <div v-if="store.loading" class="flex items-center justify-center py-16 text-zinc-500 gap-2">
        <Loader2 class="w-5 h-5 animate-spin" />
        <span class="text-sm">Carregando...</span>
      </div>
      <div v-else-if="store.error" class="flex items-center gap-2 p-4 bg-red-500/10 border border-red-500/20 rounded-xl text-red-400 text-sm">
        <AlertCircle class="w-4 h-4 flex-shrink-0" />
        {{ store.error }}
      </div>

      <template v-else>
        <!-- Abas -->
        <nav class="flex flex-wrap gap-1 border-b border-zinc-800">
          <button
            v-for="tab in TABS"
            :key="tab.name"
            class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 -mb-px whitespace-nowrap transition-colors"
            :class="activeTab === tab.name
              ? 'border-emerald-500 text-emerald-400'
              : 'border-transparent text-zinc-400 hover:text-zinc-200 hover:border-zinc-700'"
            @click="activeTab = tab.name"
          >
            <component :is="tab.icon" class="w-4 h-4" />
            {{ tab.name }}
          </button>
        </nav>

        <!-- ── Rede: CRUD de prefixos e ASNs ── -->
        <template v-if="activeTab === 'Rede'">
          <div class="grid grid-cols-1 xl:grid-cols-2 gap-6">
            <section
              v-for="key in LIST_KEYS"
              :key="key"
              class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden"
            >
              <div class="px-6 py-4 border-b border-zinc-800 bg-zinc-950/40">
                <h2 class="text-sm font-semibold text-slate-200">{{ settingByKey(key)?.label }}</h2>
                <p class="text-xs text-zinc-500 mt-0.5">{{ settingByKey(key)?.description }}</p>
              </div>
              <div class="p-6 space-y-4">
                <div class="flex gap-2">
                  <input
                    v-model="listInput[key]"
                    type="text"
                    :placeholder="key === 'OWN_ASN_LIST' ? 'ex.: 65001' : 'ex.: 100.64.0.0/10'"
                    class="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 placeholder-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 font-mono"
                    :class="failed[key] ? 'border-red-500/60 ring-2 ring-red-500/30' : ''"
                    @keyup.enter="listAdd(key)"
                  />
                  <button
                    class="px-3 py-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 hover:bg-emerald-500/20 transition-colors"
                    @click="listAdd(key)"
                  >
                    <Plus class="w-4 h-4" />
                  </button>
                </div>
                <p v-if="failed[key]" class="text-xs text-red-400">
                  Valor inválido — {{ key === 'OWN_ASN_LIST' ? 'informe um ASN numérico' : 'informe um CIDR (ex.: 10.0.0.0/8)' }}.
                </p>
                <div class="flex flex-wrap gap-2">
                  <span
                    v-for="item in listItems(key)"
                    :key="item"
                    class="inline-flex items-center gap-1.5 pl-3 pr-1.5 py-1 rounded-full bg-zinc-800 border border-zinc-700 text-sm font-mono text-slate-200"
                  >
                    {{ item }}
                    <button
                      class="w-5 h-5 rounded-full flex items-center justify-center text-zinc-500 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                      @click="listRemove(key, item)"
                    >
                      <X class="w-3 h-3" />
                    </button>
                  </span>
                  <span v-if="listItems(key).length === 0" class="text-xs text-zinc-600 italic py-1.5">
                    Nenhum item cadastrado.
                  </span>
                </div>
              </div>
            </section>
          </div>
        </template>

        <!-- ── IA: configuração guiada ── -->
        <template v-if="activeTab === 'IA'">
          <section class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden max-w-4xl">
            <div class="px-6 py-4 border-b border-zinc-800 bg-zinc-950/40 flex items-center justify-between">
              <div>
                <h2 class="text-sm font-semibold text-slate-200">Explicações por IA (LLM local)</h2>
                <p class="text-xs text-zinc-500 mt-0.5">
                  Gera explicações em linguagem natural para alertas e anomalias usando um Ollama local — sem enviar dados para fora.
                </p>
              </div>
              <!-- Toggle -->
              <button
                class="relative w-11 h-6 rounded-full transition-colors flex-shrink-0"
                :class="llmEnabled ? 'bg-emerald-500' : 'bg-zinc-700'"
                @click="toggleLlm"
              >
                <span
                  class="absolute top-0.5 w-5 h-5 rounded-full bg-white transition-all"
                  :class="llmEnabled ? 'left-[22px]' : 'left-0.5'"
                ></span>
              </button>
            </div>

            <div v-if="llmEnabled" class="p-6 space-y-5">
              <!-- Endpoint -->
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Endpoint do Ollama</label>
                <div class="flex gap-2">
                  <input
                    v-model="drafts['LLM_ENDPOINT']"
                    type="text"
                    placeholder="http://localhost:11434"
                    class="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
                    @keyup.enter="saveEndpoint"
                  />
                  <button
                    class="px-4 py-2 rounded-lg bg-zinc-800 border border-zinc-700 text-sm text-zinc-300 hover:text-emerald-400 hover:border-emerald-500/40 transition-colors"
                    :disabled="probing"
                    @click="saveEndpoint"
                  >
                    <Loader2 v-if="probing" class="w-4 h-4 animate-spin" />
                    <span v-else>Conectar</span>
                  </button>
                </div>
                <p class="text-xs text-zinc-500 mt-1">
                  Rodando local: <span class="font-mono">http://localhost:11434</span> · No stack Docker:
                  <span class="font-mono">http://ollama:11434</span> (suba com <span class="font-mono">docker compose --profile llm up -d</span>)
                </p>
                <p v-if="probeError" class="text-xs text-amber-400 mt-1">{{ probeError }}</p>
              </div>

              <!-- Modelos detectados -->
              <div v-if="models.length > 0">
                <label class="block text-sm font-medium text-slate-200 mb-1">Modelo</label>
                <div class="flex flex-wrap gap-2">
                  <button
                    v-for="m in models"
                    :key="m"
                    class="px-3 py-1.5 rounded-lg border text-sm font-mono transition-colors"
                    :class="drafts['LLM_MODEL'] === m
                      ? 'bg-emerald-500/15 text-emerald-400 border-emerald-500/40'
                      : 'bg-zinc-950 text-zinc-400 border-zinc-700 hover:text-zinc-200'"
                    @click="selectModel(m)"
                  >
                    {{ m }}
                  </button>
                </div>
                <p class="text-xs text-zinc-500 mt-1">
                  Detectados no endpoint. Recomendado: <span class="font-mono">qwen2.5:3b</span> (roda bem em CPU).
                </p>
              </div>

              <!-- Idioma -->
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Idioma das explicações</label>
                <div class="flex gap-2">
                  <button
                    v-for="l in LANGUAGES"
                    :key="l.value"
                    class="px-3 py-1.5 rounded-lg border text-sm transition-colors"
                    :class="(drafts[LANGUAGE_KEY] ?? 'pt-BR') === l.value
                      ? 'bg-emerald-500/15 text-emerald-400 border-emerald-500/40'
                      : 'bg-zinc-950 text-zinc-400 border-zinc-700 hover:text-zinc-200'"
                    @click="saveLanguage(l.value)"
                  >
                    {{ l.label }}
                  </button>
                </div>
              </div>

              <!-- Teste -->
              <div class="pt-2 border-t border-zinc-800">
                <button
                  class="flex items-center gap-2 px-4 py-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 hover:bg-emerald-500/20 text-sm font-medium transition-colors"
                  :disabled="testing || !drafts['LLM_MODEL']"
                  @click="testLlm"
                >
                  <Loader2 v-if="testing" class="w-4 h-4 animate-spin" />
                  <FlaskConical v-else class="w-4 h-4" />
                  Testar geração
                </button>
                <div
                  v-if="testResult"
                  class="mt-3 px-4 py-3 rounded-lg text-sm border"
                  :class="testResult.ok
                    ? 'bg-emerald-500/5 border-emerald-500/20 text-emerald-300'
                    : 'bg-red-500/5 border-red-500/20 text-red-400'"
                >
                  {{ testResult.text }}
                </div>
                <p class="text-xs text-zinc-500 mt-2">
                  Mudanças de endpoint/modelo passam a valer para novas explicações após reiniciar o coletor.
                </p>
              </div>
            </div>

            <div v-else class="p-6 text-sm text-zinc-500">
              Desabilitado — nenhuma explicação automática será gerada. Os alertas continuam funcionando normalmente.
            </div>
          </section>
        </template>

        <!-- Demais grupos: listagem genérica -->
        <section
          v-if="genericItems(activeTab).length > 0"
          class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden max-w-4xl"
        >
          <div class="divide-y divide-zinc-800">
            <div
              v-for="setting in genericItems(activeTab)"
              :key="setting.key"
              class="px-6 py-4 flex items-start gap-4"
            >
              <div class="flex-1 min-w-0">
                <label :for="setting.key" class="block text-sm font-medium text-slate-200 mb-0.5">
                  {{ setting.label }}
                </label>
                <p class="text-xs text-zinc-500 leading-relaxed">{{ setting.description }}</p>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0 w-72">
                <input
                  :id="setting.key"
                  v-model="drafts[setting.key]"
                  type="text"
                  :placeholder="setting.value || '—'"
                  class="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 placeholder-zinc-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500/50 transition-all font-mono"
                  @keyup.enter="save(setting.key)"
                />
                <button
                  @click="save(setting.key)"
                  :disabled="saving[setting.key] || !isDirty(setting.key)"
                  :class="[
                    'flex-shrink-0 w-9 h-9 rounded-lg flex items-center justify-center transition-all',
                    saved[setting.key]
                      ? 'bg-emerald-500/20 border border-emerald-500/30 text-emerald-400'
                      : failed[setting.key]
                        ? 'bg-red-500/20 border border-red-500/30 text-red-400'
                        : isDirty(setting.key)
                          ? 'bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 hover:bg-emerald-500/20'
                          : 'bg-zinc-800 border border-zinc-700 text-zinc-600 cursor-not-allowed'
                  ]"
                  :title="saved[setting.key] ? 'Salvo!' : failed[setting.key] ? 'Erro ao salvar' : 'Salvar'"
                >
                  <Loader2 v-if="saving[setting.key]" class="w-4 h-4 animate-spin" />
                  <Check v-else-if="saved[setting.key]" class="w-4 h-4" />
                  <AlertCircle v-else-if="failed[setting.key]" class="w-4 h-4" />
                  <Save v-else class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <div
          v-if="genericItems(activeTab).length === 0 && activeTab !== 'Rede' && activeTab !== 'IA'"
          class="text-sm text-zinc-600 italic py-8 text-center"
        >
          Nenhum parâmetro neste grupo.
        </div>
      </template>
    </div>
  </div>
</template>
