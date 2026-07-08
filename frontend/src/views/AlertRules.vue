<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useAlertsStore } from '../stores/alerts'
import { useAuthStore } from '../stores/auth'
import { ShieldAlert, Plus, Pencil, Trash2, ToggleLeft, ToggleRight } from 'lucide-vue-next'

const store = useAlertsStore()
const auth = useAuthStore()

// Form state
const showForm = ref(false)
const formStep = ref<1 | 2>(1)
const editingId = ref<number | null>(null)
const formError = ref('')

// Step 1: rule type
const ruleType = ref<'upload_inversion' | 'attack_signature'>('upload_inversion')

// Common fields
const formName = ref('')
const formExporterId = ref<number | null>(null)
const formEnabled = ref(true)

// upload_inversion params
const uiShortWindow = ref(10)
const uiHistoryWindow = ref(1440)
const uiInversionRatio = ref(2.0)
const uiMinHistDlRatio = ref(3.0)
const uiMinUploadMbps = ref(5)
const uiCooldownMin = ref(30)

// attack_signature params
const asWindowMin = ref(2)
const asMinPps = ref(1000)
const asMaxAvgPkt = ref(200)
const asMinTotalPkts = ref(10000)
const asAttackPorts = ref('')
const asCooldownMin = ref(5)
const asAutoBgp = ref(false)
const asWithdrawAfterMin = ref(60)
const asBgpCommunityId = ref<number | null>(null)

// Exporters list (for selector)
const exporters = ref<{ id: number; ip_address: string; name: string }[]>([])
const communities = ref<{ id: number; name: string; community: string }[]>([])
const deleteConfirm = ref<number | null>(null)

async function loadExporters() {
  try {
    const resp = await fetch('/api/exporters', {
      headers: { Authorization: `Bearer ${auth.token}` },
    })
    if (resp.ok) exporters.value = await resp.json()
  } catch { /* non-fatal */ }
}

async function loadCommunities() {
  try {
    const resp = await fetch('/api/bgp/communities', {
      headers: { Authorization: `Bearer ${auth.token}` },
    })
    if (resp.ok) communities.value = await resp.json()
  } catch { /* non-fatal */ }
}

function openCreate() {
  editingId.value = null
  formStep.value = 1
  ruleType.value = 'upload_inversion'
  formName.value = ''
  formExporterId.value = null
  formEnabled.value = true
  resetParams()
  formError.value = ''
  showForm.value = true
}

function openEdit(rule: typeof store.rules[0]) {
  editingId.value = rule.id
  formStep.value = 2 // go directly to params for edit
  ruleType.value = rule.rule_type as 'upload_inversion' | 'attack_signature'
  formName.value = rule.name
  formExporterId.value = rule.exporter_id
  formEnabled.value = rule.enabled

  const p = rule.params
  if (rule.rule_type === 'upload_inversion') {
    uiShortWindow.value = Number(p.short_window_min ?? 10)
    uiHistoryWindow.value = Number(p.history_window_min ?? 1440)
    uiInversionRatio.value = Number(p.inversion_ratio ?? 2.0)
    uiMinHistDlRatio.value = Number(p.min_hist_download_ratio ?? 3.0)
    uiMinUploadMbps.value = Number(p.min_upload_mbps ?? 5)
    uiCooldownMin.value = Number(p.cooldown_min ?? 30)
  } else {
    asWindowMin.value = Number(p.window_min ?? 2)
    asMinPps.value = Number(p.min_pps ?? 1000)
    asMaxAvgPkt.value = Number(p.max_avg_pkt_bytes ?? 200)
    asMinTotalPkts.value = Number(p.min_total_packets ?? 10000)
    asAttackPorts.value = (p.attack_ports as number[] | undefined ?? []).join(', ')
    asCooldownMin.value = Number(p.cooldown_min ?? 5)
    asAutoBgp.value = Boolean(p.auto_bgp_announce ?? false)
    asWithdrawAfterMin.value = Number(p.bgp_withdraw_after_min ?? 60)
    asBgpCommunityId.value = (p.bgp_community_id as number | null) ?? null
  }

  formError.value = ''
  showForm.value = true
}

function resetParams() {
  uiShortWindow.value = 10
  uiHistoryWindow.value = 1440
  uiInversionRatio.value = 2.0
  uiMinHistDlRatio.value = 3.0
  uiMinUploadMbps.value = 5
  uiCooldownMin.value = 30
  asWindowMin.value = 2
  asMinPps.value = 1000
  asMaxAvgPkt.value = 200
  asMinTotalPkts.value = 10000
  asAttackPorts.value = ''
  asCooldownMin.value = 5
}

function buildParams() {
  if (ruleType.value === 'upload_inversion') {
    return {
      short_window_min: uiShortWindow.value,
      history_window_min: uiHistoryWindow.value,
      inversion_ratio: uiInversionRatio.value,
      min_hist_download_ratio: uiMinHistDlRatio.value,
      min_upload_mbps: uiMinUploadMbps.value,
      cooldown_min: uiCooldownMin.value,
    }
  } else {
    const ports = asAttackPorts.value
      .split(',')
      .map(s => parseInt(s.trim()))
      .filter(n => !isNaN(n))
    return {
      window_min: asWindowMin.value,
      min_pps: asMinPps.value,
      max_avg_pkt_bytes: asMaxAvgPkt.value,
      min_total_packets: asMinTotalPkts.value,
      attack_ports: ports,
      cooldown_min: asCooldownMin.value,
      auto_bgp_announce: asAutoBgp.value,
      bgp_withdraw_after_min: asWithdrawAfterMin.value,
      bgp_community_id: asBgpCommunityId.value,
    }
  }
}

async function submit() {
  formError.value = ''
  if (!formName.value.trim()) {
    formError.value = 'Nome é obrigatório'
    return
  }
  if (ruleType.value === 'upload_inversion' && uiHistoryWindow.value < uiShortWindow.value) {
    formError.value = 'Janela de histórico deve ser >= janela curta'
    return
  }

  const data = {
    name: formName.value.trim(),
    exporter_id: formExporterId.value,
    rule_type: ruleType.value,
    enabled: formEnabled.value,
    params: buildParams(),
  }

  try {
    if (editingId.value) {
      await store.updateRule(editingId.value, data)
    } else {
      await store.createRule(data)
    }
    showForm.value = false
  } catch (e: unknown) {
    formError.value = (e as Error).message
  }
}

async function doDelete(id: number) {
  await store.deleteRule(id)
  deleteConfirm.value = null
}

function ruleTypeLabel(t: string) {
  return t === 'upload_inversion' ? 'Upload Inversion' : 'Attack Signature'
}

onMounted(async () => {
  await Promise.all([store.loadRules(), loadExporters(), loadCommunities()])
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-5xl mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <ShieldAlert class="w-6 h-6 text-amber-500" />
        <h1 class="text-2xl font-bold text-slate-100">Regras de Alerta</h1>
      </div>
      <button @click="openCreate"
        class="flex items-center gap-2 px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 text-white text-sm font-medium transition-colors">
        <Plus class="w-4 h-4" /> Nova Regra
      </button>
    </div>

    <!-- Rules table -->
    <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl overflow-hidden">
      <div v-if="store.rules.length === 0" class="text-center py-12 text-zinc-500 text-sm">
        Nenhuma regra criada.
        <button @click="openCreate" class="text-amber-400 hover:underline ml-1">Criar primeira regra →</button>
      </div>
      <table v-else class="w-full text-sm">
        <thead>
          <tr class="text-left text-xs text-zinc-500 border-b border-zinc-800 bg-zinc-900/80">
            <th class="px-4 py-3">Nome</th>
            <th class="px-4 py-3">Tipo</th>
            <th class="px-4 py-3">Exporter</th>
            <th class="px-4 py-3">Status</th>
            <th class="px-4 py-3"></th>
          </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50">
          <tr v-for="rule in store.rules" :key="rule.id" class="hover:bg-zinc-800/30 transition-colors">
            <td class="px-4 py-3 text-slate-200 font-medium">{{ rule.name }}</td>
            <td class="px-4 py-3">
              <span class="px-2 py-0.5 rounded text-xs font-mono"
                :class="rule.rule_type === 'upload_inversion'
                  ? 'bg-sky-500/10 text-sky-400'
                  : 'bg-orange-500/10 text-orange-400'">
                {{ ruleTypeLabel(rule.rule_type) }}
              </span>
            </td>
            <td class="px-4 py-3 text-zinc-400 text-xs font-mono">
              {{ rule.exporter_id
                  ? (exporters.find(e => e.id === rule.exporter_id)?.ip_address ?? `#${rule.exporter_id}`)
                  : 'Todos' }}
            </td>
            <td class="px-4 py-3">
              <button @click="store.toggleRule(rule.id)" class="flex items-center gap-1.5 text-sm transition-colors"
                :class="rule.enabled ? 'text-emerald-400' : 'text-zinc-600'">
                <component :is="rule.enabled ? ToggleRight : ToggleLeft" class="w-5 h-5" />
                {{ rule.enabled ? 'Ativa' : 'Inativa' }}
              </button>
            </td>
            <td class="px-4 py-3">
              <div class="flex items-center gap-2">
                <button @click="openEdit(rule)" class="text-zinc-500 hover:text-zinc-200 transition-colors">
                  <Pencil class="w-4 h-4" />
                </button>
                <div v-if="deleteConfirm === rule.id" class="flex items-center gap-1.5">
                  <span class="text-xs text-red-400">Confirmar?</span>
                  <button @click="doDelete(rule.id)" class="text-xs px-1.5 py-0.5 rounded bg-red-500 text-white">Sim</button>
                  <button @click="deleteConfirm = null" class="text-xs text-zinc-500">Não</button>
                </div>
                <button v-else @click="deleteConfirm = rule.id" class="text-zinc-600 hover:text-red-400 transition-colors">
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <!-- Form slide-over -->
  <Teleport to="body">
    <div v-if="showForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      @click.self="showForm = false">
      <div class="bg-zinc-900 border border-zinc-700 rounded-xl shadow-2xl p-6 w-full max-w-lg mx-4 space-y-5">
        <h3 class="font-semibold text-slate-100">
          {{ editingId ? 'Editar Regra' : 'Nova Regra de Alerta' }}
        </h3>

        <!-- Step 1: type selection (create only) -->
        <div v-if="formStep === 1" class="space-y-3">
          <p class="text-xs text-zinc-500">Selecione o tipo de detecção:</p>
          <button @click="ruleType = 'upload_inversion'; formStep = 2"
            class="w-full text-left p-4 rounded-lg border transition-colors"
            :class="ruleType === 'upload_inversion'
              ? 'border-sky-500/50 bg-sky-500/5'
              : 'border-zinc-700 hover:border-zinc-600'">
            <div class="font-medium text-slate-200 text-sm">Upload Inversion</div>
            <div class="text-xs text-zinc-400 mt-1">Detecta hosts que passam a fazer mais upload do que download — sinal de botnet, servidor não declarado ou loop de roteamento.</div>
          </button>
          <button @click="ruleType = 'attack_signature'; formStep = 2"
            class="w-full text-left p-4 rounded-lg border transition-colors"
            :class="ruleType === 'attack_signature'
              ? 'border-orange-500/50 bg-orange-500/5'
              : 'border-zinc-700 hover:border-zinc-600'">
            <div class="font-medium text-slate-200 text-sm">Attack Signature</div>
            <div class="text-xs text-zinc-400 mt-1">Detecta alto PPS com pacotes pequenos direcionados a portas de ataque conhecidas — SYN flood, amplificação UDP, brute force.</div>
          </button>
        </div>

        <!-- Step 2: params -->
        <div v-else class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div class="col-span-2">
              <label class="text-xs text-zinc-400 mb-1 block">Nome *</label>
              <input v-model="formName" placeholder="Ex: PPPoE clientes BNG1"
                class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 placeholder-zinc-600 focus:outline-none focus:border-amber-500" />
            </div>
            <div>
              <label class="text-xs text-zinc-400 mb-1 block">Exporter</label>
              <select v-model="formExporterId"
                class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500">
                <option :value="null">Todos</option>
                <option v-for="e in exporters" :key="e.id" :value="e.id">{{ e.ip_address }}</option>
              </select>
            </div>
            <div class="flex items-end">
              <button @click="formEnabled = !formEnabled"
                class="flex items-center gap-2 px-3 py-2 rounded-lg border text-sm transition-colors w-full justify-center"
                :class="formEnabled
                  ? 'border-emerald-500/30 bg-emerald-500/5 text-emerald-400'
                  : 'border-zinc-700 text-zinc-500'">
                <component :is="formEnabled ? ToggleRight : ToggleLeft" class="w-4 h-4" />
                {{ formEnabled ? 'Ativa' : 'Inativa' }}
              </button>
            </div>
          </div>

          <!-- upload_inversion params -->
          <template v-if="ruleType === 'upload_inversion'">
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Janela curta (min)</label>
                <input v-model.number="uiShortWindow" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Janela histórico (min)</label>
                <input v-model.number="uiHistoryWindow" type="number" min="60"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Ratio inversão (×)</label>
                <input v-model.number="uiInversionRatio" type="number" min="1" step="0.1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Ratio hist. DL/UL mín (×)</label>
                <input v-model.number="uiMinHistDlRatio" type="number" min="1" step="0.1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Upload mín (Mbps)</label>
                <input v-model.number="uiMinUploadMbps" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Cooldown (min)</label>
                <input v-model.number="uiCooldownMin" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
            </div>
          </template>

          <!-- attack_signature params -->
          <template v-else>
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Janela (min)</label>
                <input v-model.number="asWindowMin" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">PPS mínimo</label>
                <input v-model.number="asMinPps" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Pkt máx médio (bytes)</label>
                <input v-model.number="asMaxAvgPkt" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Total pacotes mín</label>
                <input v-model.number="asMinTotalPkts" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
              <div class="col-span-2">
                <label class="text-xs text-zinc-400 mb-1 block">Portas de ataque (vazio = padrão)</label>
                <input v-model="asAttackPorts" placeholder="80, 443, 53, 22, 123..."
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono placeholder-zinc-600 focus:outline-none focus:border-amber-500" />
              </div>
              <div>
                <label class="text-xs text-zinc-400 mb-1 block">Cooldown (min)</label>
                <input v-model.number="asCooldownMin" type="number" min="1"
                  class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
              </div>
            </div>
            <!-- BGP auto-announce -->
            <div class="border-t border-zinc-800 pt-3 space-y-3">
              <div class="flex items-center justify-between">
                <div>
                  <div class="text-sm text-slate-200">Blackhole automático via BGP</div>
                  <div class="text-xs text-zinc-500">Anuncia /32 do IP atacante ao detectar o alerta</div>
                </div>
                <button @click="asAutoBgp = !asAutoBgp"
                  class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm transition-colors"
                  :class="asAutoBgp ? 'border-red-500/30 bg-red-500/5 text-red-400' : 'border-zinc-700 text-zinc-500'">
                  <span class="w-2 h-2 rounded-full" :class="asAutoBgp ? 'bg-red-400' : 'bg-zinc-600'"></span>
                  {{ asAutoBgp ? 'Ativo' : 'Inativo' }}
                </button>
              </div>
              <template v-if="asAutoBgp">
                <div class="grid grid-cols-2 gap-3">
                  <div>
                    <label class="text-xs text-zinc-400 mb-1 block">Retirar após (min, 0=nunca)</label>
                    <input v-model.number="asWithdrawAfterMin" type="number" min="0"
                      class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500" />
                  </div>
                  <div>
                    <label class="text-xs text-zinc-400 mb-1 block">Community BGP</label>
                    <select v-model="asBgpCommunityId"
                      class="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-amber-500">
                      <option :value="null">Nenhuma</option>
                      <option v-for="c in communities" :key="c.id" :value="c.id">{{ c.name }} ({{ c.community }})</option>
                    </select>
                  </div>
                </div>
              </template>
            </div>
          </template>

          <div v-if="formError" class="text-xs text-red-400 px-1">{{ formError }}</div>

          <div class="flex gap-3 pt-1">
            <button v-if="!editingId" @click="formStep = 1"
              class="px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">
              ← Voltar
            </button>
            <button @click="showForm = false"
              class="flex-1 px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-sm font-medium transition-colors">
              Cancelar
            </button>
            <button @click="submit" :disabled="store.loading"
              class="flex-1 px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-40 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors">
              {{ editingId ? 'Salvar' : 'Criar' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
