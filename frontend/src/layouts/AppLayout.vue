<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import {
  LayoutDashboard, Users, Network, LogOut, ChevronDown, ChevronRight,
  User as UserIcon, Key, TrendingUp, Globe, Plug, UserCog, KeyRound, Info,
  SlidersHorizontal, BugPlay, Server, DatabaseBackup, Activity, Tag,
  ListFilter, History, Bell, ShieldAlert, MessageCircle, Brain,
} from 'lucide-vue-next'
import DebugConsole from '../views/DebugConsole.vue'
import FlowVisionLogo from '../components/FlowVisionLogo.vue'

const router = useRouter()
const route  = useRoute()
const authStore = useAuthStore()

const isUserMenuOpen = ref(false)
const debugOpen      = ref(false)
const appVersion = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0'
const appCommit  = typeof __APP_COMMIT__  !== 'undefined' ? __APP_COMMIT__  : 'dev'

// ── Collapsible sections ──────────────────────────────────────────────────
// Each key matches a section; value = expanded
const expanded = ref<Record<string, boolean>>({
  analise: false,
  alertas: false,
  ia:      false,
  bgp:     false,
  admin:   false,
})

// Auto-expand the section that contains the current route
function sectionForPath(p: string): string | null {
  if (['/top-talkers', '/asn-traffic', '/ports'].some(r => p.startsWith(r))) return 'analise'
  if (p.startsWith('/alerts'))                                                 return 'alertas'
  if (p.startsWith('/ai'))                                                     return 'ia'
  if (p.startsWith('/bgp'))                                                    return 'bgp'
  if (['/exporters', '/users', '/backup', '/settings', '/license', '/about']
      .some(r => p.startsWith(r)))                                             return 'admin'
  return null
}

function syncExpanded(p: string) {
  const sec = sectionForPath(p)
  if (sec) expanded.value[sec] = true
}

syncExpanded(route.path)
watch(() => route.path, syncExpanded)

function toggle(sec: string) {
  expanded.value[sec] = !expanded.value[sec]
}

function isActive(paths: string[]) {
  return paths.some(p => route.path.startsWith(p))
}

// ─────────────────────────────────────────────────────────────────────────

function handleLogout() {
  authStore.logout()
  router.push('/login')
}

function closeUserMenu(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (!target.closest('.user-menu-container')) isUserMenuOpen.value = false
}

onMounted(()  => document.addEventListener('click', closeUserMenu))
onUnmounted(() => document.removeEventListener('click', closeUserMenu))
</script>

<template>
  <div class="flex h-screen w-full bg-zinc-950 text-slate-100 font-sans overflow-hidden">
    
    <!-- Sidebar -->
    <aside class="w-64 flex-shrink-0 border-r border-zinc-800 bg-zinc-950/50 flex flex-col z-20">
      <!-- Logo/Brand -->
      <div class="h-16 flex items-center px-6 border-b border-zinc-800">
        <h2 class="text-xl font-bold tracking-tight flex items-center gap-2">
          <FlowVisionLogo class="w-6 h-6 text-emerald-500" />
          <span class="text-slate-100">Flow</span><span class="text-emerald-500">Vision</span>
        </h2>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 overflow-y-auto py-4 px-3 space-y-0.5">

        <!-- ── Dashboards (always visible, never collapsible) ── -->
        <router-link to="/dashboard"
          class="nav-link" :class="route.path.startsWith('/dashboard') ? 'nav-active' : 'nav-idle'">
          <LayoutDashboard class="w-5 h-5" /> Dashboard
        </router-link>

        <router-link to="/history"
          class="nav-link" :class="route.path.startsWith('/history') ? 'nav-active' : 'nav-idle'">
          <TrendingUp class="w-5 h-5" /> Histórico
        </router-link>

        <router-link to="/server"
          class="nav-link" :class="route.path.startsWith('/server') ? 'nav-active' : 'nav-idle'">
          <Server class="w-5 h-5" /> Servidor
        </router-link>

        <!-- ── Análise ── -->
        <div class="pt-2">
          <button @click="toggle('analise')"
            class="nav-section-btn w-full"
            :class="isActive(['/top-talkers','/asn-traffic','/ports']) ? 'text-emerald-400' : 'text-zinc-500'">
            <span class="flex items-center gap-2">
              <Globe class="w-4 h-4" /> Análise
            </span>
            <ChevronRight class="w-3.5 h-3.5 transition-transform duration-200"
              :class="expanded.analise ? 'rotate-90' : ''" />
          </button>
          <div v-if="expanded.analise" class="mt-0.5 ml-3 space-y-0.5 border-l border-zinc-800 pl-3">
            <router-link to="/top-talkers" class="nav-link" :class="route.path.startsWith('/top-talkers') ? 'nav-active' : 'nav-idle'">
              <Users class="w-4 h-4" /> Top Talkers
            </router-link>
            <router-link to="/asn-traffic" class="nav-link" :class="route.path.startsWith('/asn-traffic') ? 'nav-active' : 'nav-idle'">
              <Globe class="w-4 h-4" /> ASN Traffic
            </router-link>
            <router-link to="/ports" class="nav-link" :class="route.path.startsWith('/ports') ? 'nav-active' : 'nav-idle'">
              <Plug class="w-4 h-4" /> Aplicações
            </router-link>
          </div>
        </div>

        <!-- ── Alertas ── -->
        <div>
          <button @click="toggle('alertas')"
            class="nav-section-btn w-full"
            :class="isActive(['/alerts']) ? 'text-emerald-400' : 'text-zinc-500'">
            <span class="flex items-center gap-2">
              <Bell class="w-4 h-4" /> Alertas
            </span>
            <ChevronRight class="w-3.5 h-3.5 transition-transform duration-200"
              :class="expanded.alertas ? 'rotate-90' : ''" />
          </button>
          <div v-if="expanded.alertas" class="mt-0.5 ml-3 space-y-0.5 border-l border-zinc-800 pl-3">
            <router-link to="/alerts/events" class="nav-link" :class="route.path === '/alerts/events' ? 'nav-active' : 'nav-idle'">
              <Bell class="w-4 h-4" /> Eventos
            </router-link>
            <template v-if="authStore.isAdmin">
              <router-link to="/alerts/rules" class="nav-link" :class="route.path === '/alerts/rules' ? 'nav-active' : 'nav-idle'">
                <ShieldAlert class="w-4 h-4" /> Regras
              </router-link>
              <router-link to="/alerts/telegram" class="nav-link" :class="route.path === '/alerts/telegram' ? 'nav-active' : 'nav-idle'">
                <MessageCircle class="w-4 h-4" /> Telegram
              </router-link>
            </template>
          </div>
        </div>

        <!-- ── IA ── -->
        <div>
          <button @click="toggle('ia')"
            class="nav-section-btn w-full"
            :class="isActive(['/ai']) ? 'text-emerald-400' : 'text-zinc-500'">
            <span class="flex items-center gap-2">
              <Brain class="w-4 h-4" /> IA
            </span>
            <ChevronRight class="w-3.5 h-3.5 transition-transform duration-200"
              :class="expanded.ia ? 'rotate-90' : ''" />
          </button>
          <div v-if="expanded.ia" class="mt-0.5 ml-3 space-y-0.5 border-l border-zinc-800 pl-3">
            <router-link to="/ai" class="nav-link" :class="route.path.startsWith('/ai') ? 'nav-active' : 'nav-idle'">
              <Brain class="w-4 h-4" /> Insights
            </router-link>
          </div>
        </div>

        <!-- ── BGP (admin) ── -->
        <template v-if="authStore.isAdmin">
          <div>
            <button @click="toggle('bgp')"
              class="nav-section-btn w-full"
              :class="isActive(['/bgp']) ? 'text-emerald-400' : 'text-zinc-500'">
              <span class="flex items-center gap-2">
                <Activity class="w-4 h-4" /> BGP
              </span>
              <ChevronRight class="w-3.5 h-3.5 transition-transform duration-200"
                :class="expanded.bgp ? 'rotate-90' : ''" />
            </button>
            <div v-if="expanded.bgp" class="mt-0.5 ml-3 space-y-0.5 border-l border-zinc-800 pl-3">
              <router-link to="/bgp" class="nav-link" :class="route.path === '/bgp' ? 'nav-active' : 'nav-idle'">
                <Activity class="w-4 h-4" /> Sessões
              </router-link>
              <router-link to="/bgp/peers" class="nav-link" :class="route.path === '/bgp/peers' ? 'nav-active' : 'nav-idle'">
                <Server class="w-4 h-4" /> Peers
              </router-link>
              <router-link to="/bgp/communities" class="nav-link" :class="route.path === '/bgp/communities' ? 'nav-active' : 'nav-idle'">
                <Tag class="w-4 h-4" /> Communities
              </router-link>
              <router-link to="/bgp/prefixes" class="nav-link" :class="route.path === '/bgp/prefixes' ? 'nav-active' : 'nav-idle'">
                <ListFilter class="w-4 h-4" /> Prefixos
              </router-link>
              <router-link to="/bgp/announcements" class="nav-link" :class="route.path === '/bgp/announcements' ? 'nav-active' : 'nav-idle'">
                <History class="w-4 h-4" /> Anúncios
              </router-link>
            </div>
          </div>
        </template>

        <!-- ── Administração (admin) ── -->
        <template v-if="authStore.isAdmin">
          <div>
            <button @click="toggle('admin')"
              class="nav-section-btn w-full"
              :class="isActive(['/exporters','/users','/backup','/settings','/license','/about']) ? 'text-emerald-400' : 'text-zinc-500'">
              <span class="flex items-center gap-2">
                <SlidersHorizontal class="w-4 h-4" /> Administração
              </span>
              <ChevronRight class="w-3.5 h-3.5 transition-transform duration-200"
                :class="expanded.admin ? 'rotate-90' : ''" />
            </button>
            <div v-if="expanded.admin" class="mt-0.5 ml-3 space-y-0.5 border-l border-zinc-800 pl-3">
              <router-link to="/exporters" class="nav-link" :class="route.path.startsWith('/exporters') ? 'nav-active' : 'nav-idle'">
                <Network class="w-4 h-4" /> Exporters
              </router-link>
              <router-link to="/users" class="nav-link" :class="route.path.startsWith('/users') ? 'nav-active' : 'nav-idle'">
                <UserCog class="w-4 h-4" /> Usuários
              </router-link>
              <router-link to="/backup" class="nav-link" :class="route.path.startsWith('/backup') ? 'nav-active' : 'nav-idle'">
                <DatabaseBackup class="w-4 h-4" /> Backup
              </router-link>
              <router-link to="/settings" class="nav-link" :class="route.path.startsWith('/settings') ? 'nav-active' : 'nav-idle'">
                <SlidersHorizontal class="w-4 h-4" /> Configurações
              </router-link>
              <router-link to="/license" class="nav-link" :class="route.path.startsWith('/license') ? 'nav-active' : 'nav-idle'">
                <KeyRound class="w-4 h-4" /> Licença
              </router-link>
              <router-link to="/about" class="nav-link" :class="route.path.startsWith('/about') ? 'nav-active' : 'nav-idle'">
                <Info class="w-4 h-4" /> Sobre
              </router-link>
            </div>
          </div>
        </template>

      </nav>
    </aside>

    <!-- Main Wrapper -->
    <div class="flex-1 flex flex-col overflow-hidden relative">
      
      <!-- Topbar / Header -->
      <header class="h-16 flex-shrink-0 border-b border-zinc-800 bg-zinc-950/80 backdrop-blur-md flex items-center justify-between px-8 z-10">
        <div class="flex items-center">
          <!-- Optional: Espaço para breadcrumbs ou título dinâmico da página se quiser -->
        </div>

        <div class="flex items-center gap-4">
          <!-- Indicador de Status Live -->
          <div class="flex items-center gap-2 px-3 py-1.5 rounded-full bg-emerald-500/10 border border-emerald-500/20">
            <span class="relative flex h-2 w-2">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
            </span>
            <span class="text-xs font-medium text-emerald-400 tracking-wider uppercase">Live</span>
          </div>

          <!-- Debug Console button -->
          <button
            @click="debugOpen = true"
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-zinc-800 border border-zinc-700
                   text-xs font-medium text-zinc-400 hover:text-zinc-200 hover:border-zinc-500
                   transition-all mr-2"
          >
            <BugPlay class="w-3.5 h-3.5" />
            Debug
          </button>

          <!-- User Menu -->
          <div class="relative user-menu-container">
            <button 
              @click="isUserMenuOpen = !isUserMenuOpen"
              class="flex items-center gap-3 pl-3 pr-2 py-1.5 rounded-full border border-zinc-800 bg-zinc-900 hover:bg-zinc-800 hover:border-zinc-700 transition-all focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
            >
              <div class="flex flex-col items-end">
                <span class="text-sm font-medium text-slate-200 leading-tight">{{ authStore.user?.username }}</span>
                <span v-if="authStore.isAdmin" class="text-[10px] text-emerald-500 uppercase tracking-wider font-semibold">Admin</span>
              </div>
              <div class="w-8 h-8 rounded-full bg-zinc-800 flex items-center justify-center text-zinc-400 border border-zinc-700">
                <UserIcon class="w-4 h-4" />
              </div>
              <ChevronDown class="w-4 h-4 text-zinc-500" />
            </button>

            <!-- Dropdown -->
            <transition
              enter-active-class="transition duration-100 ease-out"
              enter-from-class="transform scale-95 opacity-0"
              enter-to-class="transform scale-100 opacity-100"
              leave-active-class="transition duration-75 ease-in"
              leave-from-class="transform scale-100 opacity-100"
              leave-to-class="transform scale-95 opacity-0"
            >
              <div 
                v-if="isUserMenuOpen"
                class="absolute right-0 mt-2 w-48 bg-zinc-900 rounded-lg shadow-xl border border-zinc-800 py-1 overflow-hidden origin-top-right z-50"
              >
                <!-- O menu "Alterar Senha" é simplesmente ir para a edição do próprio usuário -->
                <button
                  v-if="authStore.user"
                  @click="() => { isUserMenuOpen = false; router.push('/profile'); }"
                  class="w-full flex items-center gap-2 px-4 py-2.5 text-sm text-zinc-300 hover:bg-zinc-800 hover:text-slate-100 transition-colors"
                >
                  <Key class="w-4 h-4" />
                  Meu Perfil
                </button>
                <div class="h-px bg-zinc-800 my-1"></div>
                <button
                  @click="handleLogout"
                  class="w-full flex items-center gap-2 px-4 py-2.5 text-sm text-red-400 hover:bg-red-400/10 hover:text-red-300 transition-colors"
                >
                  <LogOut class="w-4 h-4" />
                  Sair
                </button>
              </div>
            </transition>
          </div>
        </div>
      </header>

      <!-- Page Content -->
      <main class="flex-1 overflow-y-auto relative bg-zinc-950">
        <div class="pb-16"> <!-- Padding bottom extra para o footer não tampar o conteúdo -->
          <slot />
        </div>
      </main>

      <!-- Footer -->
      <footer class="absolute bottom-0 left-0 right-0 h-10 border-t border-zinc-800 bg-zinc-950/90 backdrop-blur-sm flex items-center justify-between px-8 text-xs text-zinc-500 pointer-events-none z-10">
        <p>&copy; {{ new Date().getFullYear() }} Hahn Tech Desenvolvimento e Consultoria Ltda</p>
        <p class="font-mono">FlowVision v{{ appVersion }} <span class="text-zinc-600">({{ appCommit }})</span></p>
      </footer>
    </div>

  </div>

  <!-- Debug Console modal -->
  <Teleport to="body">
    <DebugConsole v-if="debugOpen" @close="debugOpen = false" />
  </Teleport>
</template>

<style scoped>
.nav-link {
  @apply flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150;
}
.nav-active {
  @apply bg-emerald-500/10 text-emerald-400;
}
.nav-idle {
  @apply text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200;
}
.nav-section-btn {
  @apply flex items-center justify-between px-3 py-2 rounded-lg text-sm font-medium
         transition-all duration-150 hover:bg-zinc-800/60;
}
</style>
