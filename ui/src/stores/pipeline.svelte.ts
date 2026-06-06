import type { PipelineEvent, StageName, RunStatus } from '../types'

interface ProgressInfo {
  current: number
  total: number
  message: string
}

// Reactive state
let connected = $state(false)
let currentRunId = $state<string | null>(null)
let events = $state<PipelineEvent[]>([])
let latestStage = $state<StageName | null>(null)
let latestProgress = $state<ProgressInfo | null>(null)
let endedStatus = $state<RunStatus | null>(null)

let es: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let reconnectDelay = 1000
let reconnectAttempts = 0
const MAX_RECONNECT_ATTEMPTS = 3
const MAX_EVENTS = 200

export function getPipelineState() {
  return {
    get connected() { return connected },
    get currentRunId() { return currentRunId },
    get events() { return events },
    get latestStage() { return latestStage },
    get latestProgress() { return latestProgress },
    get endedStatus() { return endedStatus },
  }
}

export function connect(runId?: string): void {
  disconnect()

  currentRunId = runId ?? null
  events = []
  latestStage = null
  latestProgress = null
  endedStatus = null
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

function doConnect(): void {
  const url = currentRunId
    ? `/api/run/stream?run_id=${encodeURIComponent(currentRunId)}`
    : '/api/run/stream'

  es = new EventSource(url)

  es.addEventListener('pipeline', (e) => {
    try {
      const ev = JSON.parse(e.data) as PipelineEvent
      handleEvent(ev)
    } catch {
      // ignore malformed events
    }
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
    } else {
      // Fallback: poll the run status
      startPolling()
    }
  }
}

let pollTimer: ReturnType<typeof setInterval> | null = null

function startPolling(): void {
  if (!currentRunId) return
  pollTimer = setInterval(async () => {
    try {
      const res = await fetch(`/api/run/${currentRunId}`)
      if (res.ok) {
        const manifest = await res.json()
        if (manifest.status !== 'Running') {
          // Run finished, stop polling
          if (pollTimer) clearInterval(pollTimer)
          pollTimer = null
          endedStatus = manifest.status
        }
      }
    } catch {
      // ignore poll errors
    }
  }, 5000)
}

function handleEvent(ev: PipelineEvent): void {
  // Append to events buffer
  events = [...events.slice(-(MAX_EVENTS - 1)), ev]

  if ('Started' in ev) {
    currentRunId = ev.Started.run_id
  } else if ('StageStart' in ev) {
    latestStage = ev.StageStart.stage
    latestProgress = null
  } else if ('Progress' in ev) {
    latestStage = ev.Progress.stage
    latestProgress = {
      current: ev.Progress.current,
      total: ev.Progress.total,
      message: ev.Progress.message,
    }
  } else if ('StageEnd' in ev) {
    latestProgress = null
  } else if ('Ended' in ev) {
    endedStatus = ev.Ended.status
    // Disconnect after a short delay
    setTimeout(() => disconnect(), 2000)
  }
}
