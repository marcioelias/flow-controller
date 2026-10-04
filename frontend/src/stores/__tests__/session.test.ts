import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '../auth'
import { isApiUnauthorized, safeRedirect } from '../../lib/sessionGuard'

function jwt(exp: number): string {
  const b64 = (o: object) => btoa(JSON.stringify(o)).replace(/=+$/, '')
  return `${b64({ alg: 'HS256' })}.${b64({ sub: 'admin', exp, is_admin: true })}.sig`
}

describe('session expiry (task 17.11)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('detects an expired token from its exp claim', () => {
    const store = useAuthStore()
    store.token = jwt(Math.floor(Date.now() / 1000) - 60)
    expect(store.tokenExpired()).toBe(true)
    store.token = jwt(Math.floor(Date.now() / 1000) + 3600)
    expect(store.tokenExpired()).toBe(false)
  })

  // AC-02
  it('ignores 401 from the login endpoint itself', () => {
    expect(isApiUnauthorized('/api/auth/login', 401)).toBe(false)
    expect(isApiUnauthorized('/api/exporters', 401)).toBe(true)
    expect(isApiUnauthorized('/api/exporters', 500)).toBe(false)
  })

  // AC-03
  it('only redirects to internal paths', () => {
    expect(safeRedirect('/exporters')).toBe('/exporters')
    expect(safeRedirect('//evil.com')).toBe('/dashboard')
    expect(safeRedirect('https://evil.com')).toBe('/dashboard')
    expect(safeRedirect(undefined)).toBe('/dashboard')
  })
})
