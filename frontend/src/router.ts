import { createRouter, createWebHistory } from 'vue-router'
import Login from './views/Login.vue'
import Dashboard from './views/Dashboard.vue'
import Exporters from './views/Exporters.vue'
import ExporterForm from './views/ExporterForm.vue'
import Users from './views/Users.vue'
import UserForm from './views/UserForm.vue'
import { useAuthStore } from './stores/auth'

const routes = [
  {
    path: '/login',
    name: 'Login',
    component: Login,
    meta: { requiresAuth: false },
  },
  {
    path: '/dashboard',
    name: 'Dashboard',
    component: Dashboard,
    meta: { requiresAuth: true },
  },
  {
    path: '/exporters',
    name: 'Exporters',
    component: Exporters,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/exporters/new',
    name: 'NewExporter',
    component: ExporterForm,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/exporters/:id/edit',
    name: 'EditExporter',
    component: ExporterForm,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/users',
    name: 'Users',
    component: Users,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/users/new',
    name: 'NewUser',
    component: UserForm,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/users/:id/edit',
    name: 'EditUser',
    component: UserForm,
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/profile',
    name: 'Profile',
    component: () => import('./views/Profile.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/top-talkers',
    name: 'TopTalkers',
    component: () => import('./views/TopTalkers.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/asn-traffic',
    name: 'AsnTraffic',
    component: () => import('./views/AsnTraffic.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/ports',
    name: 'PortBreakdown',
    component: () => import('./views/PortBreakdown.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/history',
    name: 'TrafficHistory',
    component: () => import('./views/TrafficHistory.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/server',
    name: 'ServerHealth',
    component: () => import('./views/ServerHealth.vue'),
    meta: { requiresAuth: true },
  },
  // BGP routes (admin only)
  {
    path: '/bgp',
    name: 'BgpDashboard',
    component: () => import('./views/BgpDashboard.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/bgp/peers',
    name: 'BgpPeers',
    component: () => import('./views/BgpPeers.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/bgp/communities',
    name: 'BgpCommunities',
    component: () => import('./views/BgpCommunities.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/bgp/prefixes',
    name: 'BgpPrefixes',
    component: () => import('./views/BgpPrefixes.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/bgp/announcements',
    name: 'BgpAnnouncements',
    component: () => import('./views/BgpAnnouncements.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  // Alert routes
  {
    path: '/alerts/events',
    name: 'AlertEvents',
    component: () => import('./views/AlertEvents.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/alerts/rules',
    name: 'AlertRules',
    component: () => import('./views/AlertRules.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/alerts/telegram',
    name: 'TelegramSettings',
    component: () => import('./views/TelegramSettings.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  // AI / ML routes
  {
    path: '/ai',
    name: 'AiInsights',
    component: () => import('./views/AiInsights.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/backup',
    name: 'Backup',
    component: () => import('./views/BackupRestore.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/settings',
    name: 'Settings',
    component: () => import('./views/Settings.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/license',
    name: 'License',
    component: () => import('./views/License.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/about',
    name: 'About',
    component: () => import('./views/About.vue'),
    meta: { requiresAuth: true, requiresAdmin: true },
  },
  {
    path: '/',
    redirect: '/dashboard',
  },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

// Navigation guard
router.beforeEach((to, _from, next) => {
  const authStore = useAuthStore()

  if (to.meta.requiresAuth && !authStore.isAuthenticated) {
    next('/login')
  } else if (to.meta.requiresAdmin && !authStore.isAdmin) {
    next('/dashboard')  // Redirect non-admins to dashboard
  } else if (to.path === '/login' && authStore.isAuthenticated) {
    next('/dashboard')
  } else {
    next()
  }
})

export default router
