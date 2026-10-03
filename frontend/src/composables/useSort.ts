import { computed, ref, type Ref } from 'vue'

export type SortDir = 'asc' | 'desc'
type Getter<T> = (row: T) => unknown

export interface SortState {
  key: Ref<string>
  dir: Ref<SortDir>
  toggle: (key: string) => void
}

// Ordenação por coluna das tabelas (task 17.3): números desc no 1º clique,
// texto asc; nulos sempre por último
export function useSort<T>(
  rows: () => T[],
  initialKey: string,
  initialDir: SortDir = 'desc',
  getters: Record<string, Getter<T>> = {},
) {
  const key = ref(initialKey)
  const dir = ref<SortDir>(initialDir)

  const valueOf = (row: T, k: string): unknown =>
    getters[k] ? getters[k](row) : (row as Record<string, unknown>)[k]

  const sorted = computed(() => {
    const k = key.value
    const sign = dir.value === 'asc' ? 1 : -1
    return [...rows()].sort((a, b) => {
      const va = valueOf(a, k)
      const vb = valueOf(b, k)
      if (va == null && vb == null) return 0
      if (va == null) return 1
      if (vb == null) return -1
      if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sign
      if (typeof va === 'boolean' && typeof vb === 'boolean') return (Number(va) - Number(vb)) * sign
      return String(va).localeCompare(String(vb), 'pt-BR', { numeric: true }) * sign
    })
  })

  function toggle(k: string) {
    if (key.value === k) {
      dir.value = dir.value === 'asc' ? 'desc' : 'asc'
      return
    }
    key.value = k
    const sample = rows().map((r) => valueOf(r, k)).find((v) => v != null)
    const isDate = typeof sample === 'string' && /^\d{4}-\d{2}-\d{2}/.test(sample)
    dir.value = typeof sample === 'number' || isDate ? 'desc' : 'asc'
  }

  const state: SortState = { key, dir, toggle }
  return { sorted, sort: state }
}
