<script setup lang="ts">
// Seletor único de dispositivo (task 17.9 R-07)
import { ref, computed, onMounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { ROLES, roleLabel, type DeviceValue, type ExporterRole } from '../utils/device'

interface Exporter {
  id: number
  ip_address: string
  name: string
  role?: ExporterRole
}

const props = defineProps<{ modelValue: DeviceValue; exporters?: Exporter[] }>()
const emit = defineEmits<{
  'update:modelValue': [value: DeviceValue]
  change: [value: DeviceValue]
}>()

const authStore = useAuthStore()
const fetched = ref<Exporter[]>([])
const list = computed(() => props.exporters ?? fetched.value)

const groups = computed(() =>
  ROLES.map((role) => ({
    role,
    items: list.value.filter((e) => (e.role ?? 'borda') === role),
  })).filter((g) => g.items.length > 0),
)
const showRole = (role: ExporterRole) =>
  props.modelValue === `role:${role}` || groups.value.some((g) => g.role === role)

function onChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value
  emit('update:modelValue', value)
  emit('change', value)
}

onMounted(async () => {
  if (props.exporters) return
  try {
    const res = await fetch('/api/exporters/enabled', { headers: authStore.getAuthHeaders() })
    if (res.ok) fetched.value = await res.json()
  } catch {}
})
</script>

<template>
  <select
    :value="modelValue"
    @change="onChange"
    class="appearance-none pl-3 pr-8 py-2 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 cursor-pointer"
  >
    <optgroup label="Totais">
      <option value="role:borda">Borda — total do AS</option>
      <option v-if="showRole('bng')" value="role:bng">BNG — todos</option>
      <option v-if="showRole('cgnat')" value="role:cgnat">CGNAT — todos</option>
    </optgroup>
    <optgroup v-for="g in groups" :key="g.role" :label="roleLabel(g.role)">
      <option v-for="exp in g.items" :key="exp.id" :value="exp.ip_address">
        {{ exp.name }} ({{ exp.ip_address }})
      </option>
    </optgroup>
  </select>
</template>
