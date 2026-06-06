// Log SSE store

let connected = $state(false)
let lines = $state<string[]>([])

const MAX_LINES = 500

let es: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let reconnectDelay = 1000
let reconnectAttempts = 0
const MAX_RECONNECT_ATTEMPTS = 5

export function getLogState() {
  return {
    get connected() { return connected },
    get lines() { return lines },
  }
}

export function connect(): void {
  disconnect()
  reconnectAttempts = 0
  doConnect()
}

export function disconnect(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  if (es) {
    es.close()
    es = null
  }
  connected = false
  reconnectAttempts = 0
}

export function clearLines(): void {
  lines = []
}

function doConnect(): void {
  es = new EventSource('/api/logs/stream')

  es.addEventListener('log', (e) => {
    const line = e.data as string
    lines = [...lines.slice(-(MAX_LINES - 1)), line]
  })

  es.addEventListener('ping', () => {
    // keep-alive, ignore
  })

  es.onopen = () => {
    connected = true
    reconnectAttempts = 0
    reconnectDelay = 1000
  }

  es.onerror = () => {
    connected = false
    es?.close()
    es = null

    reconnectAttempts++
    if (reconnectAttempts <= MAX_RECONNECT_ATTEMPTS) {
      reconnectTimer = setTimeout(doConnect, reconnectDelay)
      reconnectDelay = Math.min(reconnectDelay * 2, 30_000)
    }
  }
}
