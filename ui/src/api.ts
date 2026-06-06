import type {
  ApiMessage,
  BulkDeleteResponse,
  CacheCleanResponse,
  CacheResponse,
  ConfigResponse,
  DateResponse,
  RunManifest,
  RunRequest,
  RunStartResponse,
  StatsResponse,
  StatusResponse,
} from './types'

class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init)
  if (!res.ok) {
    let message = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body.message) message = body.message
    } catch {
      // no JSON body
    }
    throw new ApiError(res.status, message)
  }
  return res.json() as Promise<T>
}

// ── Stats ─────────────────────────────────────────────────────

export function getStats(year: number, month: number): Promise<StatsResponse> {
  return fetchJson(`/api/stats/${year}/${month}`)
}

// ── Date ──────────────────────────────────────────────────────

export function getDateRuns(date: string): Promise<DateResponse> {
  return fetchJson(`/api/date/${date}`)
}

export function deleteDate(date: string): Promise<BulkDeleteResponse> {
  return fetchJson(`/api/date/${date}`, { method: 'DELETE' })
}

// ── Run ───────────────────────────────────────────────────────

export function getRun(runId: string): Promise<RunManifest> {
  return fetchJson(`/api/run/${runId}`)
}

export function deleteRun(runId: string): Promise<ApiMessage> {
  return fetchJson(`/api/run/${runId}`, { method: 'DELETE' })
}

export function triggerRun(req: RunRequest): Promise<RunStartResponse> {
  return fetchJson('/api/run', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
}

export function resendRun(runId: string): Promise<ApiMessage> {
  return fetchJson(`/api/run/${runId}/send`, { method: 'POST' })
}

export function bulkDeleteRuns(runIds: string[]): Promise<BulkDeleteResponse> {
  return fetchJson('/api/runs/bulk', {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ run_ids: runIds }),
  })
}

// ── Config ────────────────────────────────────────────────────

export function getConfig(): Promise<ConfigResponse> {
  return fetchJson('/api/config')
}

export function saveConfig(content: string): Promise<ApiMessage> {
  return fetchJson('/api/config', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  })
}

export function reloadConfig(): Promise<ApiMessage> {
  return fetchJson('/api/config/reload', { method: 'POST' })
}

// ── Cache ─────────────────────────────────────────────────────

export function getCache(): Promise<CacheResponse> {
  return fetchJson('/api/cache')
}

export function cleanCache(kind: string): Promise<CacheCleanResponse> {
  return fetchJson(`/api/cache/${kind}`, { method: 'DELETE' })
}

// ── Status ────────────────────────────────────────────────────

export function getStatus(): Promise<StatusResponse> {
  return fetchJson('/api/status')
}
