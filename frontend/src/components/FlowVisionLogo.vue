<script setup lang="ts">
defineProps<{ class?: string }>()
</script>

<template>
  <svg
    :class="$props.class"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <!-- Eye outline -->
    <path d="M1 12s4.5-8 11-8 11 8 11 8-4.5 8-11 8S1 12 1 12z" />
    <!-- Waveform as iris — flow signal inside the eye -->
    <path d="M8 12 L9.5 9 L11 12 L12.5 15 L14 12 L16 12" />
  </svg>
</template>
