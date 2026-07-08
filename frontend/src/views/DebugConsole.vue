<script setup lang="ts">
import { ref, watch, onUnmounted, nextTick } from 'vue'
import { X, BugPlay, Wifi, WifiOff, Trash2, ExternalLink } from 'lucide-vue-next'

const emit = defineEmits<{ close: [] }>()

interface DebugFlow {
  timestamp_sec: number
  exporter_ip:   string
  src_ip:        string
  dst_ip:        string
  src_port:      number
  dst_port:      number
  protocol:      number
  bytes:         number
  packets:       number
  src_asn:       number
  dst_asn:       number
  ingress_if:    number
  egress_if:     number
  tcp_flags:     number
  flow_count:    number
}

const MAX_FLOWS = 500
const RATE_LIMIT_MS = 10

const flows        = ref<DebugFlow[]>([])
const selected     = ref<DebugFlow | null>(null)
const selectedIdx  = ref<number>(-1)
const filterIp     = ref('')
const connected    = ref(false)
const flowCount    = ref(0)
const connectedSec = ref(0)
const newSinceSelected = ref(0)
const listEl       = ref<HTMLElement | null>(null)
const autoScroll   = ref(true)

let ws: WebSocket | null = null
let filterTimer: ReturnType<typeof setTimeout> | null = null
let clockTimer: ReturnType<typeof setInterval> | null = null
let lastRecv = 0

// ── Protocol / port / flags helpers ─────────────────────────────────────────

const PROTO: Record<number, string> = {
  1: 'ICMP', 6: 'TCP', 17: 'UDP', 47: 'GRE',
  50: 'ESP', 51: 'AH', 89: 'OSPF', 132: 'SCTP',
}
function protoName(n: number) { return PROTO[n] ?? `Proto ${n}` }

const PORTS: Record<number, string> = {
  20: 'FTP-data', 21: 'FTP', 22: 'SSH', 23: 'Telnet', 25: 'SMTP',
  53: 'DNS', 67: 'DHCP', 80: 'HTTP', 110: 'POP3', 123: 'NTP',
  143: 'IMAP', 161: 'SNMP', 179: 'BGP', 443: 'HTTPS', 465: 'SMTPS',
  514: 'Syslog', 587: 'SMTP-TLS', 993: 'IMAPS', 995: 'POP3S',
  1194: 'OpenVPN', 1723: 'PPTP', 3306: 'MySQL', 3389: 'RDP',
  5432: 'PostgreSQL', 5900: 'VNC', 6379: 'Redis', 8080: 'HTTP-alt',
  8443: 'HTTPS-alt', 27017: 'MongoDB',
}
function portName(p: number) { return PORTS[p] ? `${p} (${PORTS[p]})` : String(p) }

function tcpFlags(f: number): string {
  const names = ['FIN','SYN','RST','PSH','ACK','URG','ECE','CWR']
  const set = names.filter((_, i) => f & (1 << i))
  return set.length ? set.join(' ') : '—'
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`
  if (b < 1048576) return `${(b/1024).toFixed(1)} KB`
  return `${(b/1048576).toFixed(2)} MB`
}

function formatTime(sec: number): string {
  const d = new Date(sec * 1000)
  return d.toLocaleTimeString('pt-BR', { hour12: false })
}

function elapsed(sec: number): string {
  const h = Math.floor(sec / 3600).toString().padStart(2,'0')
  const m = Math.floor((sec % 3600) / 60).toString().padStart(2,'0')
  const s = (sec % 60).toString().padStart(2,'0')
  return `${h}:${m}:${s}`
}

// ── WebSocket ────────────────────────────────────────────────────────────────

function connect() {
  if (ws) { ws.onclose = null; ws.close() }
  flows.value = []
  selected.value = null
  selectedIdx.value = -1
  newSinceSelected.value = 0
  flowCount.value = 0
  connectedSec.value = 0

  const base = location.hostname + ':3000'
  const url  = filterIp.value.trim()
    ? `ws://${base}/ws/debug?src_ip=${encodeURIComponent(filterIp.value.trim())}`
    : `ws://${base}/ws/debug`

  ws = new WebSocket(url)
  ws.onopen  = () => { connected.value = true }
  ws.onclose = () => { connected.value = false }

  let pending: DebugFlow[] = []
  let rafScheduled = false

  function flush() {
    rafScheduled = false
    if (!pending.length) return
    const batch = pending.splice(0)
    flowCount.value += batch.length
    flows.value.push(...batch)
    if (flows.value.length > MAX_FLOWS) {
      const remove = flows.value.length - MAX_FLOWS
      flows.value.splice(0, remove)
      if (selectedIdx.value >= 0) selectedIdx.value -= remove
    }
    // Count new flows from same src_ip as selected
    if (selected.value) {
      newSinceSelected.value += batch.filter(
        f => f.src_ip === selected.value!.src_ip
      ).length
    }
    if (autoScroll.value) {
      nextTick(() => {
        if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight
      })
    }
  }

  ws.onmessage = (e) => {
    const now = Date.now()
    if (now - lastRecv < RATE_LIMIT_MS) return
    lastRecv = now
    try {
      pending.push(JSON.parse(e.data) as DebugFlow)
      if (!rafScheduled) { rafScheduled = true; requestAnimationFrame(flush) }
    } catch { /* ignore malformed */ }
  }

  if (clockTimer) clearInterval(clockTimer)
  clockTimer = setInterval(() => { if (connected.value) connectedSec.value++ }, 1000)
}

function disconnect() {
  if (ws) { ws.onclose = null; ws.close(); ws = null }
  connected.value = false
  if (clockTimer) { clearInterval(clockTimer); clockTimer = null }
}

// Debounced reconnect on filter change
watch(filterIp, () => {
  if (filterTimer) clearTimeout(filterTimer)
  filterTimer = setTimeout(connect, 500)
})

onUnmounted(disconnect)

// Auto-connect on mount
connect()

// ── Interaction ──────────────────────────────────────────────────────────────

function selectFlow(flow: DebugFlow, idx: number) {
  selected.value = flow
  selectedIdx.value = idx
  newSinceSelected.value = 0
  autoScroll.value = false
}

function jumpToLatest() {
  if (!selected.value) return
  const ip = selected.value.src_ip
  // Find last flow with same src_ip
  for (let i = flows.value.length - 1; i >= 0; i--) {
    if (flows.value[i].src_ip === ip) {
      selected.value = flows.value[i]
      selectedIdx.value = i
      newSinceSelected.value = 0
      nextTick(() => {
        const el = listEl.value?.querySelector(`[data-idx="${i}"]`) as HTMLElement
        el?.scrollIntoView({ block: 'center' })
      })
      return
    }
  }
}

function clearSelected() {
  selected.value = null
  selectedIdx.value = -1
  newSinceSelected.value = 0
  autoScroll.value = true
  nextTick(() => {
    if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight
  })
}

function clearFlows() {
  flows.value = []
  flowCount.value = 0
  selected.value = null
  selectedIdx.value = -1
  newSinceSelected.value = 0
}

function protoColor(p: number): string {
  if (p === 6)  return 'text-blue-400'
  if (p === 17) return 'text-emerald-400'
  if (p === 1)  return 'text-yellow-400'
  return 'text-zinc-400'
}
</script>

<template>
  <!-- Backdrop -->
  <div class="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4"
       @click.self="emit('close')">

    <div class="w-full max-w-7xl h-[90vh] bg-zinc-950 border border-zinc-800 rounded-2xl
                flex flex-col overflow-hidden shadow-2xl">

      <!-- Header -->
      <div class="flex items-center gap-3 px-5 py-3 border-b border-zinc-800 flex-shrink-0">
        <BugPlay class="w-5 h-5 text-emerald-400" />
        <span class="text-sm font-semibold text-slate-200">Flow Debug Console</span>

        <!-- Connection status -->
        <div :class="['flex items-center gap-1.5 px-2 py-1 rounded-full text-xs font-medium ml-2',
          connected ? 'bg-emerald-500/10 text-emerald-400' : 'bg-red-500/10 text-red-400']">
          <component :is="connected ? Wifi : WifiOff" class="w-3 h-3" />
          {{ connected ? 'Conectado' : 'Desconectado' }}
        </div>

        <!-- Filter -->
        <div class="flex items-center gap-2 ml-4">
          <span class="text-xs text-zinc-500">Filtro src_ip:</span>
          <input
            v-model="filterIp"
            type="text"
            placeholder="ex: 10.0.0.5 (vazio = todos)"
            class="bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-1.5 text-xs text-slate-200
                   placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500/50
                   font-mono w-52"
          />
        </div>

        <div class="ml-auto flex items-center gap-2">
          <button @click="clearFlows"
            class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs text-zinc-400
                   hover:text-zinc-200 hover:bg-zinc-800 transition-colors">
            <Trash2 class="w-3.5 h-3.5" />
            Limpar
          </button>
          <button @click="emit('close')"
            class="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800 transition-colors">
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Body: list + detail -->
      <div class="flex flex-1 min-h-0">

        <!-- Flow list -->
        <div ref="listEl"
             class="w-[45%] flex-shrink-0 overflow-y-auto border-r border-zinc-800 font-mono text-xs">
          <div v-if="flows.length === 0"
               class="flex flex-col items-center justify-center h-full text-zinc-600 gap-2">
            <BugPlay class="w-8 h-8 opacity-30" />
            <span>Aguardando flows...</span>
          </div>

          <div
            v-for="(flow, idx) in flows"
            :key="idx"
            :data-idx="idx"
            @click="selectFlow(flow, idx)"
            :class="['flex items-center gap-2 px-3 py-1.5 cursor-pointer transition-colors border-l-2',
              selectedIdx === idx
                ? 'bg-emerald-500/10 border-emerald-500 text-slate-200'
                : 'border-transparent hover:bg-zinc-900 text-zinc-400']"
          >
            <!-- Time -->
            <span class="text-zinc-600 flex-shrink-0">{{ formatTime(flow.timestamp_sec) }}</span>
            <!-- src → dst:port -->
            <span class="text-zinc-300 flex-shrink-0">{{ flow.src_ip }}</span>
            <span class="text-zinc-600">→</span>
            <span class="text-zinc-300 flex-shrink-0">{{ flow.dst_ip }}:{{ flow.dst_port }}</span>
            <!-- proto -->
            <span :class="['flex-shrink-0 font-semibold', protoColor(flow.protocol)]">
              {{ protoName(flow.protocol) }}
            </span>
            <!-- size -->
            <span class="ml-auto flex-shrink-0 text-zinc-500">{{ formatBytes(flow.bytes) }}</span>
          </div>
        </div>

        <!-- Detail panel -->
        <div class="flex-1 overflow-y-auto p-5">
          <!-- Nothing selected -->
          <div v-if="!selected"
               class="flex flex-col items-center justify-center h-full text-zinc-600 gap-2">
            <ExternalLink class="w-8 h-8 opacity-30" />
            <span class="text-sm">Clique em um flow para ver os detalhes</span>
          </div>

          <!-- Flow detail -->
          <div v-else class="space-y-5">
            <!-- Close detail -->
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-zinc-400 uppercase tracking-wider">Detalhe do Flow</span>
              <button @click="clearSelected"
                class="p-1 rounded text-zinc-600 hover:text-zinc-300 hover:bg-zinc-800 transition-colors">
                <X class="w-3.5 h-3.5" />
              </button>
            </div>

            <!-- New flows badge -->
            <div v-if="newSinceSelected > 0"
                 class="flex items-center justify-between px-3 py-2 rounded-lg
                        bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-400">
              <span>{{ newSinceSelected }} flow{{ newSinceSelected > 1 ? 's' : '' }} novos deste IP</span>
              <button @click="jumpToLatest"
                class="underline underline-offset-2 hover:text-emerald-300 transition-colors">
                Ver mais recente
              </button>
            </div>

            <!-- Fields grid -->
            <div class="grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-sm">

              <!-- Header fields -->
              <div class="col-span-2 pb-1 mb-1 border-b border-zinc-800 text-xs text-zinc-500 uppercase tracking-wider">
                Cabeçalho
              </div>
              <div class="text-zinc-500">Timestamp</div>
              <div class="text-slate-200">{{ new Date(selected.timestamp_sec * 1000).toLocaleString('pt-BR') }}</div>
              <div class="text-zinc-500">Exporter</div>
              <div class="text-slate-200">{{ selected.exporter_ip }}</div>

              <!-- Source -->
              <div class="col-span-2 pb-1 mt-3 mb-1 border-b border-zinc-800 text-xs text-zinc-500 uppercase tracking-wider">
                Origem
              </div>
              <div class="text-zinc-500">IP</div>
              <div class="text-slate-200">{{ selected.src_ip }}</div>
              <div class="text-zinc-500">Porta</div>
              <div class="text-slate-200">{{ portName(selected.src_port) }}</div>
              <div class="text-zinc-500">ASN</div>
              <div class="text-slate-200">{{ selected.src_asn || '—' }}</div>

              <!-- Destination -->
              <div class="col-span-2 pb-1 mt-3 mb-1 border-b border-zinc-800 text-xs text-zinc-500 uppercase tracking-wider">
                Destino
              </div>
              <div class="text-zinc-500">IP</div>
              <div class="text-slate-200">{{ selected.dst_ip }}</div>
              <div class="text-zinc-500">Porta</div>
              <div class="text-slate-200">{{ portName(selected.dst_port) }}</div>
              <div class="text-zinc-500">ASN</div>
              <div class="text-slate-200">{{ selected.dst_asn || '—' }}</div>

              <!-- Transport -->
              <div class="col-span-2 pb-1 mt-3 mb-1 border-b border-zinc-800 text-xs text-zinc-500 uppercase tracking-wider">
                Transporte
              </div>
              <div class="text-zinc-500">Protocolo</div>
              <div :class="['font-semibold', protoColor(selected.protocol)]">
                {{ protoName(selected.protocol) }} ({{ selected.protocol }})
              </div>
              <div class="text-zinc-500">Bytes</div>
              <div class="text-slate-200">{{ formatBytes(selected.bytes) }} <span class="text-zinc-600">({{ selected.bytes }})</span></div>
              <div class="text-zinc-500">Pacotes</div>
              <div class="text-slate-200">{{ selected.packets }}</div>
              <div class="text-zinc-500">TCP Flags</div>
              <div class="text-slate-200">
                <span v-if="selected.protocol === 6">{{ tcpFlags(selected.tcp_flags) }}</span>
                <span v-else class="text-zinc-600">—</span>
              </div>

              <!-- Interfaces -->
              <div class="col-span-2 pb-1 mt-3 mb-1 border-b border-zinc-800 text-xs text-zinc-500 uppercase tracking-wider">
                Interfaces
              </div>
              <div class="text-zinc-500">Entrada (ifIndex)</div>
              <div class="text-slate-200">{{ selected.ingress_if || '—' }}</div>
              <div class="text-zinc-500">Saída (ifIndex)</div>
              <div class="text-slate-200">{{ selected.egress_if || '—' }}</div>
              <div class="text-zinc-500">Flow Count</div>
              <div class="text-slate-200">{{ selected.flow_count }}</div>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer stats -->
      <div class="flex items-center gap-4 px-5 py-2 border-t border-zinc-800
                  text-xs text-zinc-600 flex-shrink-0 font-mono">
        <span>{{ flowCount.toLocaleString('pt-BR') }} flows recebidos</span>
        <span class="text-zinc-800">·</span>
        <span>{{ flows.length }} em memória</span>
        <span class="text-zinc-800">·</span>
        <span>conectado há {{ elapsed(connectedSec) }}</span>
        <span v-if="!autoScroll" class="ml-auto text-zinc-500 text-xs">
          scroll pausado —
          <button @click="clearSelected" class="underline underline-offset-2 hover:text-zinc-300">retomar</button>
        </span>
      </div>
    </div>
  </div>
</template>
