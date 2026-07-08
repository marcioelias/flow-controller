import { defineStore } from 'pinia'
import { ref } from 'vue'
import { useAuthStore } from './auth'

export interface Setting {
  key: string
  value: string
  label: string
  description: string
  group_name: string
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Setting[]>([])
  const loading  = ref(false)
  const error    = ref<string | null>(null)

  async function loadSettings() {
    const auth = useAuthStore()
    loading.value = true
    error.value   = null
    try {
      const res = await fetch('/api/settings', {
        headers: { Authorization: `Bearer ${auth.token}` },
      })
      if (!res.ok) throw new Error('Falha ao carregar configurações')
      settings.value = await res.json()
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function updateSetting(key: string, value: string): Promise<boolean> {
    const auth = useAuthStore()
    try {
      const res = await fetch(`/api/settings/${key}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${auth.token}`,
        },
        body: JSON.stringify({ value }),
      })
      if (!res.ok) throw new Error('Falha ao salvar')
      const updated: Setting = await res.json()
      const idx = settings.value.findIndex(s => s.key === key)
      if (idx >= 0) settings.value[idx] = updated
      return true
    } catch {
      return false
    }
  }

  return { settings, loading, error, loadSettings, updateSetting }
})
