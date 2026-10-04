import { createApp } from 'vue'
import { createPinia } from 'pinia'
import './assets/index.css'
import App from './App.vue'
import router from './router'
import { installSessionGuard } from './lib/sessionGuard'

const pinia = createPinia()
const app = createApp(App)

app.use(pinia)
app.use(router)
installSessionGuard(router)
app.mount('#app')
