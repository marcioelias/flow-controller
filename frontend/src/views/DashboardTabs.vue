<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router'
import { Activity, TrendingUp, Server } from 'lucide-vue-next'

const route = useRoute()
const router = useRouter()

const tabs = [
  { path: '/dashboard', label: 'Tempo real', icon: Activity, exact: true },
  { path: '/dashboard/history', label: 'Histórico', icon: TrendingUp, exact: false },
  { path: '/dashboard/server', label: 'Servidor', icon: Server, exact: false },
]

function isActive(tab: (typeof tabs)[number]): boolean {
  return tab.exact ? route.path === tab.path : route.path.startsWith(tab.path)
}
</script>

<template>
  <div>
    <!-- Tab bar -->
    <div class="border-b border-zinc-800 bg-zinc-950/60 sticky top-0 z-10 backdrop-blur">
      <nav class="max-w-7xl mx-auto px-8 flex gap-1">
        <button
          v-for="tab in tabs"
          :key="tab.path"
          class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 -mb-px transition-colors"
          :class="isActive(tab)
            ? 'border-emerald-500 text-emerald-400'
            : 'border-transparent text-zinc-400 hover:text-zinc-200 hover:border-zinc-700'"
          @click="router.push(tab.path)"
        >
          <component :is="tab.icon" class="w-4 h-4" />
          {{ tab.label }}
        </button>
      </nav>
    </div>

    <router-view />
  </div>
</template>
