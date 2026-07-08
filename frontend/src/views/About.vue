<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { ShieldCheck, ShieldX, ExternalLink } from 'lucide-vue-next'

const APP_VERSION = __APP_VERSION__
const APP_COMMIT  = __APP_COMMIT__
const npmDeps     = __NPM_DEPS__

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

interface DepEntry {
  name: string
  version: string
}

interface VersionInfo {
  version: string
  rustc: string
  clickhouse: string
  rust_deps: DepEntry[]
}

const license     = ref<LicenseStatus | null>(null)
const versionInfo = ref<VersionInfo | null>(null)
const depsTab     = ref<'rust' | 'npm'>('rust')

onMounted(async () => {
  try {
    const [licRes, verRes] = await Promise.all([
      fetch('/api/license'),
      fetch('/api/version'),
    ])
    if (licRes.ok) license.value = await licRes.json()
    if (verRes.ok) versionInfo.value = await verRes.json()
  } catch { /* about page must always render */ }
})

const expiresLabel = computed(() => {
  if (!license.value?.expires_at) return 'Perpétua'
  return new Date(license.value.expires_at).toLocaleDateString('pt-BR')
})

const licenseIcon  = computed(() => license.value?.valid ? ShieldCheck : ShieldX)
const licenseColor = computed(() => license.value?.valid ? 'text-emerald-400' : 'text-red-400')
const licenseBg    = computed(() => license.value?.valid
  ? 'bg-emerald-500/10 border-emerald-500/20'
  : 'bg-red-500/10 border-red-500/20')
</script>

<template>
  <div class="p-8">
    <div class="max-w-3xl mx-auto space-y-8">

      <!-- Header -->
      <header class="text-center space-y-3 py-6">
        <div class="flex items-center justify-center gap-3 mb-4">
          <div class="w-14 h-14 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center">
            <svg class="w-8 h-8 text-emerald-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round"
                d="M3 13.5A9 9 0 1 1 12 21v-3m0 0a6 6 0 0 0 6-6H6a6 6 0 0 0 6 6Z" />
            </svg>
          </div>
        </div>
        <h1 class="text-4xl font-bold tracking-tight text-slate-100">FlowVision</h1>
        <p class="text-zinc-400 text-lg">Coletor e analisador de tráfego NetFlow / IPFIX</p>
        <div class="flex items-center justify-center gap-2 mt-2">
          <span class="px-3 py-1 rounded-full bg-zinc-800 border border-zinc-700 text-xs font-mono text-zinc-300">
            v{{ APP_VERSION }}
          </span>
          <span class="px-3 py-1 rounded-full bg-zinc-800 border border-zinc-700 text-xs font-mono text-zinc-500">
            {{ APP_COMMIT }}
          </span>
        </div>
      </header>

      <!-- License card -->
      <section class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-4">
        <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Licença</h2>

        <div v-if="license" class="space-y-3">
          <!-- Status badge -->
          <div :class="['inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm font-medium', licenseBg]">
            <component :is="licenseIcon" class="w-4 h-4" :class="licenseColor" />
            <span :class="licenseColor">{{ license.valid ? 'Licença válida' : 'Licença inválida' }}</span>
          </div>

          <div class="grid grid-cols-2 gap-x-8 gap-y-3 pt-1">
            <div>
              <p class="text-xs text-zinc-500 mb-0.5">Licenciado para</p>
              <p class="text-sm text-slate-200 font-medium">{{ license.licensee || 'Free Tier ISP' }}</p>
            </div>
            <div>
              <p class="text-xs text-zinc-500 mb-0.5">Tier</p>
              <p class="text-sm text-slate-200 font-medium">{{ license.tier_label }}</p>
            </div>
            <div>
              <p class="text-xs text-zinc-500 mb-0.5">Validade</p>
              <p class="text-sm text-slate-200 font-medium">{{ expiresLabel }}</p>
            </div>
            <div v-if="!license.valid && license.message">
              <p class="text-xs text-zinc-500 mb-0.5">Motivo</p>
              <p class="text-sm text-red-400">{{ license.message }}</p>
            </div>
          </div>

          <div class="pt-1 border-t border-zinc-800">
            <p class="text-xs text-zinc-500 mb-1">Fingerprint da máquina</p>
            <p class="text-xs font-mono text-zinc-400 break-all">{{ license.fingerprint }}</p>
          </div>
        </div>

        <div v-else class="text-sm text-zinc-500 animate-pulse">Carregando informações de licença...</div>
      </section>

      <!-- About the product -->
      <section class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-4">
        <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Sobre o sistema</h2>
        <p class="text-sm text-zinc-300 leading-relaxed">
          O <strong class="text-slate-100">FlowVision</strong> é uma plataforma de coleta e análise de fluxos de rede
          NetFlow v9 e IPFIX (v10), desenvolvida para provedores de internet e operadoras de
          telecomunicações. Combina alta performance de ingestão com visibilidade em tempo real
          do tráfego, detecção de anomalias e análise histórica de top talkers, ASNs e aplicações.
        </p>
        <ul class="text-sm text-zinc-400 space-y-1.5 list-none">
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Recepção UDP de NetFlow v9 e IPFIX v10</li>
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Agregação em janelas de 1 segundo com múltiplos workers</li>
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Armazenamento ClickHouse com TTL configurável</li>
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Dashboard em tempo real via WebSocket</li>
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Análise de Top Talkers, ASN, portas e histórico</li>
          <li class="flex items-center gap-2"><span class="text-emerald-500">▸</span> Métricas Prometheus em <code class="text-xs bg-zinc-800 px-1 py-0.5 rounded">/metrics</code></li>
        </ul>
      </section>

      <!-- Component versions -->
      <section class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-4">
        <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Componentes</h2>
        <div class="grid grid-cols-2 gap-3">
          <div class="flex items-center justify-between p-3 rounded-lg bg-zinc-950/60 border border-zinc-800">
            <span class="text-xs text-zinc-400">FlowVision</span>
            <span class="text-xs font-mono text-emerald-400">v{{ APP_VERSION }}</span>
          </div>
          <div class="flex items-center justify-between p-3 rounded-lg bg-zinc-950/60 border border-zinc-800">
            <span class="text-xs text-zinc-400">Commit</span>
            <span class="text-xs font-mono text-zinc-300">{{ APP_COMMIT }}</span>
          </div>
          <div class="flex items-center justify-between p-3 rounded-lg bg-zinc-950/60 border border-zinc-800">
            <span class="text-xs text-zinc-400">Rust</span>
            <span class="text-xs font-mono text-zinc-300 truncate ml-2 max-w-[160px]">{{ versionInfo?.rustc ?? '...' }}</span>
          </div>
          <div class="flex items-center justify-between p-3 rounded-lg bg-zinc-950/60 border border-zinc-800">
            <span class="text-xs text-zinc-400">ClickHouse</span>
            <span class="text-xs font-mono text-zinc-300">{{ versionInfo?.clickhouse ?? '...' }}</span>
          </div>
        </div>
      </section>

      <!-- Dependencies -->
      <section class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-4">
        <div class="flex items-center justify-between">
          <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Dependências</h2>
          <div class="flex gap-1">
            <button
              @click="depsTab = 'rust'"
              :class="['px-3 py-1 rounded text-xs font-medium transition-colors',
                depsTab === 'rust'
                  ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
                  : 'text-zinc-500 hover:text-zinc-300']"
            >Rust</button>
            <button
              @click="depsTab = 'npm'"
              :class="['px-3 py-1 rounded text-xs font-medium transition-colors',
                depsTab === 'npm'
                  ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
                  : 'text-zinc-500 hover:text-zinc-300']"
            >npm</button>
          </div>
        </div>

        <!-- Rust deps -->
        <div v-if="depsTab === 'rust'" class="overflow-y-auto max-h-56 pr-1 space-y-1">
          <div
            v-for="dep in versionInfo?.rust_deps ?? []"
            :key="dep.name"
            class="flex items-center justify-between px-3 py-2 rounded-lg bg-zinc-950/60 border border-zinc-800/60"
          >
            <span class="text-xs text-zinc-300 font-mono">{{ dep.name }}</span>
            <span class="text-xs font-mono text-zinc-500">{{ dep.version }}</span>
          </div>
          <div v-if="!versionInfo" class="text-xs text-zinc-600 animate-pulse py-2 text-center">
            Carregando...
          </div>
        </div>

        <!-- npm deps -->
        <div v-if="depsTab === 'npm'" class="overflow-y-auto max-h-56 pr-1 space-y-1">
          <div
            v-for="dep in npmDeps"
            :key="dep.name"
            class="flex items-center justify-between px-3 py-2 rounded-lg bg-zinc-950/60 border border-zinc-800/60"
          >
            <span class="text-xs text-zinc-300 font-mono">{{ dep.name }}</span>
            <span class="text-xs font-mono text-zinc-500">{{ dep.version }}</span>
          </div>
        </div>
      </section>

      <!-- Vendor -->
      <section class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 space-y-2">
        <h2 class="text-sm font-semibold text-zinc-400 uppercase tracking-wider">Desenvolvedor</h2>
        <p class="text-base font-semibold text-slate-100">Hahn Tech Desenvolvimento e Consultoria Ltda</p>
        <p class="text-sm text-zinc-400">Soluções de infraestrutura e monitoramento de redes</p>
        <div class="pt-2 flex items-center gap-4 text-sm">
          <a
            href="mailto:contato@hahntech.com.br"
            class="flex items-center gap-1.5 text-emerald-400 hover:text-emerald-300 transition-colors"
          >
            <ExternalLink class="w-3.5 h-3.5" />
            contato@hahntech.com.br
          </a>
        </div>
      </section>

      <!-- Footer note -->
      <p class="text-center text-xs text-zinc-600 pb-4">
        FlowVision v{{ APP_VERSION }} &mdash; &copy; {{ new Date().getFullYear() }} Hahn Tech Desenvolvimento e Consultoria Ltda
      </p>

    </div>
  </div>
</template>
