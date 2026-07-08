import path from "node:path"
import fs from "node:fs"
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { execSync } from 'node:child_process'

// Single source of truth: ../VERSION (major.minor only)
// Build number is derived from git commit count — increments automatically.
const versionBase = fs.readFileSync(
  path.resolve(__dirname, '../VERSION'), 'utf-8'
).trim()

let buildNumber = '0'
let commitHash = 'dev'
try {
  buildNumber = execSync('git rev-list --count HEAD').toString().trim()
  commitHash  = execSync('git rev-parse --short HEAD').toString().trim()
} catch (e) {
  // building outside a git repo (tarball / CI without git)
}

const appVersion = `${versionBase}.${buildNumber}`

// Resolved versions of direct npm deps from package-lock.json
const npmDeps = (() => {
  const want = [
    'vue', 'vue-router', 'pinia', 'chart.js', 'vue-chartjs',
    'lucide-vue-next', 'radix-vue', 'tailwind-merge', 'vite',
    'tailwindcss', 'typescript', 'vitest',
  ]
  try {
    const lock = JSON.parse(fs.readFileSync(
      path.resolve(__dirname, 'package-lock.json'), 'utf-8'
    ))
    const pkgs = lock.packages || {}
    return want
      .map(name => {
        const entry = pkgs[`node_modules/${name}`]
        return entry ? { name, version: entry.version as string } : null
      })
      .filter(Boolean)
  } catch { return [] }
})()

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    __APP_COMMIT__:  JSON.stringify(commitHash),
    __NPM_DEPS__:    JSON.stringify(npmDeps),
  },
  plugins: [vue()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    proxy: {
      '/ch-api': {
        target: 'http://localhost:8123',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/ch-api/, '')
      }
    }
  }
})
