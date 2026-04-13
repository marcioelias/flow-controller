<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Line, Doughnut } from 'vue-chartjs'
import { Chart as ChartJS, CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler } from 'chart.js'

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Title, Tooltip, Legend, ArcElement, Filler)

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
    data: protoData.value
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

async function pollClickHouse() {
  try {
    isFetching.value = true
    // Chamada disparada ao Vite Proxy local (roteia -> http://localhost:8123)
    const sqlQuery = "SELECT toStartOfMinute(timestamp) as t, sum(bytes) as b FROM network_flows_v4 GROUP BY t ORDER BY t DESC LIMIT 10 FORMAT JSON"
    const res = await fetch(`/ch-api/?user=default&query=${encodeURIComponent(sqlQuery)}`)
    
    if (res.ok) {
      const data = await res.json()
      if (data.data && data.data.length > 0) {
        let newLabels = []
        let newBytes = []
        for (let row of data.data.reverse()) {
           newLabels.push(row.t.split(' ')[1]) // Pega a parte da hora
           newBytes.push(row.b / 1024 / 1024) // Converte para MB
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
  
  // Ciclo de Polling de 5 Segundos (Síncrono com a janela de Flush do Rust)
  pollClickHouse()
  timer = setInterval(pollClickHouse, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="min-h-screen bg-zinc-950 text-slate-100 p-8 font-sans">
    <div class="max-w-7xl mx-auto space-y-8">
      <header class="flex justify-between items-center pb-4 border-b border-zinc-800">
        <div>
          <h1 class="text-3xl font-bold Tracking-tight">Flow Analytics</h1>
          <p class="text-zinc-400 mt-1">High-Performance Traffic Insights (Powered by Clickhouse & Rust)</p>
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
        <Card class="lg:col-span-2 bg-zinc-900 border-zinc-800">
          <CardHeader>
            <CardTitle class="text-xl">Volume de Carga (Global)</CardTitle>
          </CardHeader>
          <CardContent>
            <div class="h-80">
              <Line :data="lineChartData" :options="chartOptions" />
            </div>
          </CardContent>
        </Card>

        <!-- Gráfico de Rosca / Distribuição -->
        <Card class="bg-zinc-900 border-zinc-800">
          <CardHeader>
            <CardTitle class="text-xl">Distribuição L4</CardTitle>
          </CardHeader>
          <CardContent>
            <div class="h-80">
              <Doughnut :data="donutChartData" :options="donutOptions" />
            </div>
          </CardContent>
        </Card>
      </div>
      
    </div>
  </div>
</template>
