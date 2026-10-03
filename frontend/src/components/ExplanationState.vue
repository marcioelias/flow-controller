<script setup lang="ts">
import { ref } from 'vue'
import { Loader2, RotateCcw } from 'lucide-vue-next'
import { useAlertsStore } from '../stores/alerts'

// Estado da explicação IA de um evento (task 17.5 R-04/R-07)
const props = defineProps<{
  eventId: number
  explanation: string | null
  status: 'done' | 'failed' | 'pending' | 'disabled' | undefined
  error?: string | null
  compact?: boolean
}>()
const emit = defineEmits<{ retried: [] }>()

const alerts = useAlertsStore()
const retrying = ref(false)

async function retry() {
  retrying.value = true
  try {
    await alerts.retryExplanation(props.eventId)
    emit('retried')
  } finally {
    retrying.value = false
  }
}
</script>

<template>
  <p
    v-if="explanation"
    class="text-zinc-300"
    :class="compact ? 'line-clamp-2' : 'leading-relaxed'"
    :title="compact ? explanation : undefined"
  >{{ explanation }}</p>

  <div v-else-if="status === 'failed'" class="space-y-1">
    <p class="text-red-400/90 italic" :class="compact ? 'line-clamp-1' : ''" :title="error ?? undefined">
      falhou{{ error ? `: ${error}` : '' }}
    </p>
    <button
      class="inline-flex items-center gap-1 text-xs text-emerald-400 hover:text-emerald-300 disabled:opacity-50"
      :disabled="retrying"
      @click.stop="retry"
    >
      <RotateCcw class="w-3 h-3" :class="{ 'animate-spin': retrying }" /> Tentar de novo
    </button>
  </div>

  <p v-else-if="status === 'disabled'" class="text-zinc-600 italic">
    IA desativada —
    <router-link to="/settings" class="text-emerald-400/80 hover:text-emerald-300 not-italic" @click.stop>
      configurar
    </router-link>
  </p>

  <p v-else class="text-zinc-600 italic flex items-center gap-1.5">
    <Loader2 class="w-3 h-3 animate-spin" /> gerando…
  </p>
</template>
