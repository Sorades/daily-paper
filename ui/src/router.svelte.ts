export interface Route {
  page: string
  params: Record<string, string>
}

function parseHash(hash: string): Route {
  const h = hash.replace(/^#\/?/, '')
  const parts = h.split('/').filter(Boolean)

  if (parts.length === 0) {
    return { page: 'dashboard', params: {} }
  }

  if (parts[0] === 'date' && parts[1]) {
    return { page: 'date', params: { date: parts[1] } }
  }

  if (parts[0] === 'run' && parts[1]) {
    return { page: 'run', params: { id: parts[1] } }
  }

  if (parts[0] === 'config') {
    return { page: 'config', params: {} }
  }

  if (parts[0] === 'cache') {
    return { page: 'cache', params: {} }
  }

  if (parts[0] === 'logs') {
    return { page: 'logs', params: {} }
  }

  return { page: 'dashboard', params: {} }
}

// Reactive state using Svelte 5 runes
let current = $state<Route>(parseHash(location.hash))

export function getRoute(): Route {
  return current
}

export function navigate(hash: string): void {
  location.hash = hash
}

window.addEventListener('hashchange', () => {
  current = parseHash(location.hash)
})
