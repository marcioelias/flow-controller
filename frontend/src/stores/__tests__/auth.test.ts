import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '../auth'

// Mock fetch
global.fetch = vi.fn()

describe('Auth Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.clearAllMocks()
  })

  it('initializes with no authentication', () => {
    const store = useAuthStore()
    expect(store.isAuthenticated).toBe(false)
    expect(store.user).toBe(null)
    expect(store.token).toBe(null)
  })

  it('logs in successfully', async () => {
    const mockResponse = {
      token: 'fake-jwt-token',
      user: { id: 1, username: 'testuser', is_admin: false }
    }

    ;(global.fetch as any).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse
    })

    const store = useAuthStore()
    await store.login('testuser', 'password')

    expect(store.isAuthenticated).toBe(true)
    expect(store.user?.username).toBe('testuser')
    expect(store.token).toBe('fake-jwt-token')
    expect(localStorage.getItem('token')).toBe('fake-jwt-token')
  })

  it('handles login failure', async () => {
    ;(global.fetch as any).mockResolvedValueOnce({
      ok: false
    })

    const store = useAuthStore()

    await expect(store.login('baduser', 'badpass')).rejects.toThrow('Login failed')
    expect(store.isAuthenticated).toBe(false)
  })

  it('logs out successfully', () => {
    const store = useAuthStore()
    store.token = 'fake-token'
    store.user = { id: 1, username: 'testuser', is_admin: false }
    localStorage.setItem('token', 'fake-token')
    localStorage.setItem('user', JSON.stringify(store.user))

    store.logout()

    expect(store.isAuthenticated).toBe(false)
    expect(store.user).toBe(null)
    expect(store.token).toBe(null)
    expect(localStorage.getItem('token')).toBe(null)
  })

  it('returns auth headers when authenticated', () => {
    const store = useAuthStore()
    store.token = 'fake-token'

    const headers = store.getAuthHeaders()
    expect(headers.Authorization).toBe('Bearer fake-token')
  })

  it('returns empty headers when not authenticated', () => {
    const store = useAuthStore()
    const headers = store.getAuthHeaders()
    expect(headers).toEqual({})
  })

  it('identifies admin users correctly', () => {
    const store = useAuthStore()
    store.user = { id: 1, username: 'admin', is_admin: true }

    expect(store.isAdmin).toBe(true)
  })

  it('identifies non-admin users correctly', () => {
    const store = useAuthStore()
    store.user = { id: 1, username: 'user', is_admin: false }

    expect(store.isAdmin).toBe(false)
  })
})
