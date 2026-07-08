<script setup lang="ts">
import { onMounted } from 'vue'
import { useRoute } from 'vue-router'
import AppLayout from './layouts/AppLayout.vue'

const route = useRoute()

onMounted(() => {
  // Força o Dark Mode do Shadcn globalmente
  document.documentElement.classList.add('dark')
})
</script>

<template>
  <AppLayout v-if="route.meta.requiresAuth">
    <router-view />
  </AppLayout>
  <router-view v-else />
</template>
