<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Settings as SettingsIcon, Save, Check, AlertCircle, Loader2 } from 'lucide-vue-next'
import { useSettingsStore } from '../stores/settings'

const store = useSettingsStore()

// Per-key state
const drafts   = ref<Record<string, string>>({})
const saving   = ref<Record<string, boolean>>({})
const saved    = ref<Record<string, boolean>>({})
const failed   = ref<Record<string, boolean>>({})

onMounted(async () => {
  await store.loadSettings()
  // Seed drafts from loaded values
  for (const s of store.settings) {
    drafts.value[s.key] = s.value
  }
})

const groups = computed(() => {
  const map = new Map<string, typeof store.settings>()
  for (const s of store.settings) {
    if (!map.has(s.group_name)) map.set(s.group_name, [])
    map.get(s.group_name)!.push(s)
  }
  return map
})

const GROUP_ORDER = ['Rede', 'Organização', 'Análise', 'Alertas', 'Sessão', 'Coletor', 'IA']
const orderedGroups = computed(() =>
  GROUP_ORDER
    .filter(g => groups.value.has(g))
    .map(g => ({ name: g, items: groups.value.get(g)! }))
)

async function save(key: string) {
  saving.value[key] = true
  saved.value[key]  = false
  failed.value[key] = false

  const ok = await store.updateSetting(key, drafts.value[key] ?? '')
  saving.value[key] = false
  if (ok) {
    saved.value[key] = true
    setTimeout(() => { saved.value[key] = false }, 2500)
  } else {
    failed.value[key] = true
    setTimeout(() => { failed.value[key] = false }, 3000)
  }
}

function isDirty(key: string) {
  const original = store.settings.find(s => s.key === key)?.value ?? ''
  return drafts.value[key] !== original
}
</script>

<template>
  <div class="p-8">
    <div class="max-w-3xl mx-auto space-y-8">

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

      <!-- Loading -->
      <div v-if="store.loading" class="flex items-center justify-center py-16 text-zinc-500 gap-2">
        <Loader2 class="w-5 h-5 animate-spin" />
        <span class="text-sm">Carregando...</span>
      </div>

      <!-- Error -->
      <div v-else-if="store.error" class="flex items-center gap-2 p-4 bg-red-500/10 border border-red-500/20 rounded-xl text-red-400 text-sm">
        <AlertCircle class="w-4 h-4 flex-shrink-0" />
        {{ store.error }}
      </div>

      <!-- Groups -->
      <template v-else>
        <section
          v-for="group in orderedGroups"
          :key="group.name"
          class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden"
        >
          <div class="px-6 py-3 border-b border-zinc-800 bg-zinc-950/40">
            <h2 class="text-xs font-semibold text-zinc-400 uppercase tracking-wider">{{ group.name }}</h2>
          </div>

          <div class="divide-y divide-zinc-800">
            <div
              v-for="setting in group.items"
              :key="setting.key"
              class="px-6 py-4 flex items-start gap-4"
            >
              <!-- Label + description -->
              <div class="flex-1 min-w-0">
                <label :for="setting.key" class="block text-sm font-medium text-slate-200 mb-0.5">
                  {{ setting.label }}
                </label>
                <p class="text-xs text-zinc-500 leading-relaxed">{{ setting.description }}</p>
              </div>

              <!-- Input + Save button -->
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
                  <Check  v-else-if="saved[setting.key]"  class="w-4 h-4" />
                  <AlertCircle v-else-if="failed[setting.key]" class="w-4 h-4" />
                  <Save   v-else class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </section>
      </template>

    </div>
  </div>
</template>
