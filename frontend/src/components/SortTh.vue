<script setup lang="ts">
import { computed } from 'vue'
import { ChevronUp, ChevronDown, ChevronsUpDown } from 'lucide-vue-next'
import type { SortState } from '../composables/useSort'

const props = defineProps<{
  sort: SortState
  k: string
  align?: 'left' | 'right'
}>()

const active = computed(() => props.sort.key.value === props.k)
const ariaSort = computed(() =>
  active.value ? (props.sort.dir.value === 'asc' ? 'ascending' : 'descending') : 'none',
)
</script>

<template>
  <th :aria-sort="ariaSort" class="select-none" :class="align === 'right' ? 'text-right' : ''">
    <button
      type="button"
      class="group inline-flex items-center gap-1 font-medium hover:text-zinc-200 transition-colors"
      :class="[align === 'right' ? 'flex-row-reverse' : '', active ? 'text-zinc-200' : '']"
      @click="sort.toggle(k)"
    >
      <slot />
      <ChevronUp v-if="active && sort.dir.value === 'asc'" class="w-3.5 h-3.5 text-emerald-400" />
      <ChevronDown v-else-if="active" class="w-3.5 h-3.5 text-emerald-400" />
      <ChevronsUpDown v-else class="w-3.5 h-3.5 opacity-0 group-hover:opacity-60 transition-opacity" />
    </button>
  </th>
</template>
