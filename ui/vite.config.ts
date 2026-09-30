import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  base: './',
  build: {
    outDir: 'dist',
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    allowedHosts: ['dev-paper.um580d.sorades.com'],
    proxy: {
      '/api': 'http://localhost:8991',
      '/report': 'http://localhost:8991',
    },
  },
  preview: {
    host: '0.0.0.0',
    port: 5173,
    allowedHosts: ['dev-paper.um580d.sorades.com'],
  },
})
