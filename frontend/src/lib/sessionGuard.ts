import type { Router } from 'vue-router'
import { useAuthStore } from '../stores/auth'

// Sessão expirada → login (task 17.11)

let redirecting = false

/** Internal path to return to after login; anything else falls back to the dashboard */
export function safeRedirect(value: unknown): string {
  return typeof value === 'string' && value.startsWith('/') && !value.startsWith('//')
    ? value
    : '/dashboard'
}

export function isApiUnauthorized(url: string, status: number): boolean {
  return status === 401 && url.includes('/api/') && !url.includes('/api/auth/login')
}

/** Clears the session and sends the user to the login page once (R-01) */
export function expireSession(router: Router) {
  const auth = useAuthStore()
  auth.logout()
  const current = router.currentRoute.value
  if (redirecting || current.path === '/login') return
  redirecting = true
  router
    .replace({ path: '/login', query: { expired: '1', redirect: current.fullPath } })
    .finally(() => {
      redirecting = false
    })
}

/** Every 401 from the API ends the session, whichever view made the call */
export function installSessionGuard(router: Router) {
  const original = window.fetch.bind(window)
  window.fetch = async (input, init) => {
    const response = await original(input, init)
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
    if (isApiUnauthorized(url, response.status)) {
      expireSession(router)
    }
    return response
  }
}
