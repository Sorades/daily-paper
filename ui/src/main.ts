import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { initTheme } from './stores/theme.svelte'
import { setupMockApi } from './mock/server'

// Fallback to mock data if running standalone without backend
setupMockApi()

// Initialize theme preference
initTheme()

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
