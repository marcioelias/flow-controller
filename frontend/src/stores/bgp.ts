import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useAuthStore } from './auth'

export interface BgpPeer {
  id: number
  name: string
  description: string | null
  neighbor_ip: string
  local_ip: string
  local_as: number
  peer_as: number
  hold_time: number
  has_md5: boolean
  enabled: boolean
  created_at: string
}

export interface BgpCommunity {
  id: number
  name: string
  community: string
  description: string | null
  created_at: string
}

export interface BgpPrefix {
  id: number
  prefix: string
  description: string | null
  created_at: string
}

export interface BgpAnnouncement {
  id: number
  prefix: string
  next_hop: string
  community_name: string | null
  community_value: string | null
  peer_name: string | null
  peer_neighbor_ip: string | null
  origin: string
  origin_detail: string | null
  announced_at: string
  withdrawn_at: string | null
}

export interface BgpSession {
  peer_id: number
  peer_name: string
  neighbor_ip: string
  state: 'up' | 'down' | 'unknown'
  last_up: string | null
  last_down: string | null
  updated_at: string
}

function authHeaders() {
  const auth = useAuthStore()
  return { Authorization: `Bearer ${auth.token}`, 'Content-Type': 'application/json' }
}

async function apiCall<T>(url: string, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(url, { headers: authHeaders(), ...opts })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: `HTTP ${res.status}` }))
    throw new Error(body.error || `HTTP ${res.status}`)
  }
  if (res.status === 204) return undefined as T
  return res.json()
}

export const useBgpStore = defineStore('bgp', () => {
  const sessions     = ref<BgpSession[]>([])
  const peers        = ref<BgpPeer[]>([])
  const communities  = ref<BgpCommunity[]>([])
  const prefixes     = ref<BgpPrefix[]>([])
  const announcements = ref<BgpAnnouncement[]>([])
  const loading      = ref(false)
  const error        = ref('')

  async function withLoading<T>(fn: () => Promise<T>): Promise<T> {
    loading.value = true; error.value = ''
    try { return await fn() }
    catch (e: unknown) { error.value = (e as Error).message; throw e }
    finally { loading.value = false }
  }

  // Sessions
  async function loadSessions() {
    sessions.value = await withLoading(() => apiCall<BgpSession[]>('/api/bgp/sessions'))
  }

  // Peers
  async function loadPeers() {
    peers.value = await withLoading(() => apiCall<BgpPeer[]>('/api/bgp/peers'))
  }
  async function createPeer(data: Omit<BgpPeer, 'id' | 'has_md5' | 'created_at'> & { md5_password?: string }) {
    const peer = await apiCall<BgpPeer>('/api/bgp/peers', { method: 'POST', body: JSON.stringify(data) })
    peers.value.push(peer)
    return peer
  }
  async function updatePeer(id: number, data: Partial<BgpPeer> & { md5_password?: string }) {
    const peer = await apiCall<BgpPeer>(`/api/bgp/peers/${id}`, { method: 'PUT', body: JSON.stringify(data) })
    const idx = peers.value.findIndex(p => p.id === id)
    if (idx >= 0) peers.value[idx] = peer
    return peer
  }
  async function deletePeer(id: number) {
    await apiCall<void>(`/api/bgp/peers/${id}`, { method: 'DELETE' })
    peers.value = peers.value.filter(p => p.id !== id)
  }
  async function applyConfig() {
    return apiCall<{ peers_configured: number; config_written: string; reload_signal: string }>('/api/bgp/apply', { method: 'POST' })
  }

  // Communities
  async function loadCommunities() {
    communities.value = await withLoading(() => apiCall<BgpCommunity[]>('/api/bgp/communities'))
  }
  async function createCommunity(data: { name: string; community: string; description?: string }) {
    const c = await apiCall<BgpCommunity>('/api/bgp/communities', { method: 'POST', body: JSON.stringify(data) })
    communities.value.push(c)
    return c
  }
  async function updateCommunity(id: number, data: Partial<BgpCommunity>) {
    const c = await apiCall<BgpCommunity>(`/api/bgp/communities/${id}`, { method: 'PUT', body: JSON.stringify(data) })
    const idx = communities.value.findIndex(x => x.id === id)
    if (idx >= 0) communities.value[idx] = c
    return c
  }
  async function deleteCommunity(id: number) {
    await apiCall<void>(`/api/bgp/communities/${id}`, { method: 'DELETE' })
    communities.value = communities.value.filter(x => x.id !== id)
  }

  // Prefixes
  async function loadPrefixes() {
    prefixes.value = await withLoading(() => apiCall<BgpPrefix[]>('/api/bgp/prefixes'))
  }
  async function createPrefix(data: { prefix: string; description?: string }) {
    const p = await apiCall<BgpPrefix>('/api/bgp/prefixes', { method: 'POST', body: JSON.stringify(data) })
    prefixes.value.push(p)
    return p
  }
  async function updatePrefix(id: number, data: Partial<BgpPrefix>) {
    const p = await apiCall<BgpPrefix>(`/api/bgp/prefixes/${id}`, { method: 'PUT', body: JSON.stringify(data) })
    const idx = prefixes.value.findIndex(x => x.id === id)
    if (idx >= 0) prefixes.value[idx] = p
    return p
  }
  async function deletePrefix(id: number) {
    await apiCall<void>(`/api/bgp/prefixes/${id}`, { method: 'DELETE' })
    prefixes.value = prefixes.value.filter(x => x.id !== id)
  }

  // Announcements
  async function loadAnnouncements(active = true) {
    announcements.value = await withLoading(() =>
      apiCall<BgpAnnouncement[]>(`/api/bgp/announcements?active=${active}`)
    )
  }
  async function announce(data: { prefix: string; next_hop: string; community_id?: number | null; peer_id?: number | null }) {
    return apiCall<{ id: number; command: string }>('/api/bgp/announcements', { method: 'POST', body: JSON.stringify({ ...data, origin: 'manual' }) })
  }
  async function withdraw(id: number) {
    return apiCall<{ command: string }>(`/api/bgp/announcements/${id}`, { method: 'DELETE' })
  }

  return {
    sessions, peers, communities, prefixes, announcements, loading, error,
    loadSessions, loadPeers, createPeer, updatePeer, deletePeer, applyConfig,
    loadCommunities, createCommunity, updateCommunity, deleteCommunity,
    loadPrefixes, createPrefix, updatePrefix, deletePrefix,
    loadAnnouncements, announce, withdraw,
  }
})
