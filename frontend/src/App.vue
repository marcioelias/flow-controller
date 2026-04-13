<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Line, Doughnut } from 'vue-chartjs'
import { Chart as ChartJS, CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler } from 'chart.js'
import { Monitor, LayoutDashboard } from 'lucide-vue-next'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler)

// State
const devices = ref<string[]>([])
const selectedDevice = ref<string | null>(null)

// Mocks Visuais caso a Base do clickhouse não responda a tempo no Lab Local
const timeLabels = ref<string[]>(Array.from({length: 10}, (_, i) => `${i}s`))
const trafficData = ref<number[]>(Array.from({length: 10}, () => Math.floor(Math.random() * 500) + 100))
const protoData = ref<number[]>([70, 20, 10])

const lineChartData = ref({
  labels: timeLabels.value,
  datasets: [{
    label: 'Banda Registrada (MB/s)',
    backgroundColor: 'rgba(59, 130, 246, 0.2)', // Tailwaind blue-500 opacity
    borderColor: '#3b82f6',
    borderWidth: 2,
    data: trafficData.value,
    tension: 0.4,
    fill: true,
  }]
})

const donutChartData = ref({
  labels: ['TCP', 'UDP', 'ICMP'],
  datasets: [{
    backgroundColor: ['#10b981', '#f59e0b', '#ef4444'], // emerald, amber, red
    data: protoData.value,
    borderWidth: 0,
  }]
})

const chartOptions = {
  responsive: true,
  maintainAspectRatio: false,
  plugins: { legend: { labels: { color: '#e5e7eb' } } },
  scales: {
    x: { ticks: { color: '#9ca3af' }, grid: { display: false } }, // gray-400
    y: { ticks: { color: '#9ca3af' }, grid: { color: '#374151' } } // gray-700
  }
}

const donutOptions: any = {
  responsive: true,
  maintainAspectRatio: false,
  plugins: { legend: { position: 'bottom', labels: { color: '#e5e7eb' } } },
  borderWidth: 0,
}

const isFetching = ref(false)
let timer: any = null
let deviceTimer: any = null

function selectDevice(ip: string | null) {
  selectedDevice.value = ip
  pollClickHouse() // Reload immediately when switching devices
}

async function pollDevices() {
  try {
    const sqlQuery = "SELECT DISTINCT exporter_ip FROM network_flows_v4 WHERE exporter_ip != '0.0.0.0' ORDER BY exporter_ip FORMAT JSON"
    const res = await fetch(`/ch-api/?user=default&query=${encodeURIComponent(sqlQuery)}`)
    if (res.ok) {
      const data = await res.json()
      if (data.data) {
        devices.value = data.data.map((row: any) => row.exporter_ip)
      }
    }
  } catch (e) {
    // console.warn("Failed to fetch devices", e)
  }
}

async function pollClickHouse() {
  try {
    isFetching.value = true
    const whereClause = selectedDevice.value ? `WHERE exporter_ip = '${selectedDevice.value}'` : ""
    
    // Bandwidth Query
    const sqlQuery = `SELECT toStartOfMinute(timestamp) as t, sum(bytes) as b FROM network_flows_v4 ${whereClause} GROUP BY t ORDER BY t DESC LIMIT 10 FORMAT JSON`
    const res = await fetch(`/ch-api/?user=default&query=${encodeURIComponent(sqlQuery)}`)
    
    if (res.ok) {
      const data = await res.json()
      if (data.data && data.data.length > 0) {
        let newLabels = []
        let newBytes = []
        for (let row of data.data.reverse()) {
           newLabels.push(row.t.split(' ')[1]) // HH:MM:SS
           newBytes.push(row.b / 1024 / 1024) // MB
        }
        lineChartData.value = {
           labels: newLabels,
           datasets: [{
             label: 'Banda Registrada (MB/s)',
             backgroundColor: 'rgba(59, 130, 246, 0.2)',
             borderColor: '#3b82f6',
             borderWidth: 2,
             data: newBytes,
             tension: 0.4,
             fill: true,
           }]
        }
      }
    }
  } catch (e) {
    // console.error("ClickHouse bypass - Retaining mock", e)
  } finally {
    isFetching.value = false
  }
}

onMounted(() => {
  // Força o Dark Mode do Shadcn globalmente
  document.documentElement.classList.add('dark')
  
  // Initial Fetches
  pollDevices()
  pollClickHouse()
  
  // Background Polls
  timer = setInterval(pollClickHouse, 5000)
  deviceTimer = setInterval(pollDevices, 15000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (deviceTimer) clearInterval(deviceTimer)
})
</script>

<template>
  <div class="flex h-screen overflow-hidden bg-zinc-950 text-slate-100 font-sans">
    
    <!-- Sidebar -->
    <aside class="w-72 flex-shrink-0 border-r border-zinc-800 bg-zinc-950/50 flex flex-col">
      <div class="p-6 border-b border-zinc-800">
        <h2 class="text-xl font-bold Tracking-tight flex items-center gap-2">
          <Monitor class="w-5 h-5 text-emerald-500" />
          Network Devices
        </h2>
        <p class="text-xs text-zinc-500 mt-1">Select an exporter to filter traffic</p>
      </div>
      
      <div class="flex-1 overflow-y-auto p-4 space-y-2">
        <button 
          @click="selectDevice(null)"
          :class="[
            'w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200',
            selectedDevice === null 
              ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' 
              : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 border border-transparent'
          ]"
        >
          <LayoutDashboard class="w-4 h-4" />
          All Devices (Global)
        </button>
        
        <div class="pt-4 pb-2">
          <h3 class="px-3 text-xs font-semibold text-zinc-500 uppercase tracking-wider">Detected Exporters</h3>
        </div>

        <button 
          v-for="ip in devices" 
          :key="ip"
          @click="selectDevice(ip)"
          :class="[
            'w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors duration-200',
            selectedDevice === ip 
              ? 'bg-zinc-800 text-slate-100 font-semibold' 
              : 'text-zinc-400 hover:bg-zinc-800/50 hover:text-zinc-300'
          ]"
        >
          <div :class="[
            'w-2 h-2 rounded-full',
            selectedDevice === ip ? 'bg-emerald-500' : 'bg-zinc-700'
          ]"></div>
          {{ ip }}
        </button>
        
        <div v-if="devices.length === 0" class="px-3 py-4 text-center text-zinc-600 text-sm italic">
          No devices detected yet.
        </div>
      </div>
    </aside>

    <!-- Main Content -->
    <main class="flex-1 overflow-y-auto p-8">
      <div class="max-w-7xl mx-auto space-y-8">
        <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
          <div>
            <h1 class="text-3xl font-bold Tracking-tight">Flow Analytics</h1>
            <p class="text-zinc-400 mt-1">
              <span v-if="selectedDevice">Filtering traffic for: <strong class="text-emerald-400">{{ selectedDevice }}</strong></span>
              <span v-else>High-Performance Traffic Insights (Global View)</span>
            </p>
          </div>
          <div class="flex items-center space-x-3">
            <span class="relative flex h-3 w-3">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span class="relative inline-flex rounded-full h-3 w-3 bg-emerald-500"></span>
            </span>
            <span class="text-sm font-medium text-emerald-400">Live Telemetry Active</span>
          </div>
        </header>

        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <!-- Gráfico Linear de Velocidade -->
          <Card class="lg:col-span-2 bg-zinc-900 border-zinc-800 shadow-xl overflow-hidden relative group">
            <div class="absolute inset-0 bg-gradient-to-br from-blue-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500 pointer-events-none"></div>
            <CardHeader class="pb-2">
              <CardTitle class="text-xl">Volume de Carga</CardTitle>
            </CardHeader>
            <CardContent>
              <div class="h-80">
                <Line :data="lineChartData" :options="chartOptions" />
              </div>
            </CardContent>
          </Card>

          <!-- Gráfico de Rosca / Distribuição -->
          <Card class="bg-zinc-900 border-zinc-800 shadow-xl overflow-hidden relative group">
            <div class="absolute inset-0 bg-gradient-to-br from-emerald-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500 pointer-events-none"></div>
            <CardHeader class="pb-2">
              <CardTitle class="text-xl">Distribuição L4</CardTitle>
            </CardHeader>
            <CardContent>
              <div class="h-80 relative flex items-center justify-center">
                <Doughnut :data="donutChartData" :options="donutOptions" />
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </main>
    
  </div>
</template>
