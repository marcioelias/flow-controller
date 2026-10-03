<script setup lang="ts">
// Selo de classe do endereço (task 17.7): internet não leva selo
defineProps<{ cls: 'cgnat' | 'internal' | 'internet' | undefined }>()
</script>

<template>
  <span
    v-if="cls === 'cgnat'"
    class="px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wide bg-amber-500/10 text-amber-400 border border-amber-500/30 font-sans"
    title="Endereço de CGNAT (100.64.0.0/10)"
  >CGNAT</span>
  <span
    v-else-if="cls === 'internal'"
    class="px-1.5 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wide bg-sky-500/10 text-sky-400 border border-sky-500/30 font-sans"
    title="Endereço interno (ASN próprio, prefixo interno ou espaço privado)"
  >Interno</span>
</template>
