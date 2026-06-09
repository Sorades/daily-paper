// ── Run pipeline ──────────────────────────────────────────────

export type RunStatus = 'Running' | 'Succeeded' | 'Failed' | 'Blocked' | 'Cancelled'

export type StageStatus = 'Pending' | 'Running' | 'Succeeded' | 'Failed' | 'Skipped'

export type StageName =
  | 'ZoteroSync'
  | 'SourceFetch'
  | 'Embedding'
  | 'Rerank'
  | 'PdfFetch'
  | 'TextExtract'
  | 'MetadataFetch'
  | 'DeepRead'
  | 'Render'
  | 'Send'

export interface DateWindow {
  start: string
  end: string
  label: string
}

export interface CliOverride {
  key: string
  value: string
}

export interface ErrorRecord {
  kind: ErrorKind
  message: string
  retryable: boolean
  context: unknown
  occurred_at: string
}

export interface WarningRecord {
  kind: string
  message: string
  context: unknown
}

export interface StageRecord {
  stage: StageName
  status: StageStatus
  started_at: string | null
  finished_at: string | null
  cache_hit: boolean
  input_hash: string | null
  output_ref: string | null
  error: ErrorRecord | null
}

export interface RunManifest {
  run_id: string
  parent_run_id: string | null
  status: RunStatus
  started_at: string
  finished_at: string | null
  config_hash: string
  cli_overrides: CliOverride[]
  date_window: DateWindow
  stages: StageRecord[]
  warnings: WarningRecord[]
  error: ErrorRecord | null
}

// ── Error types ───────────────────────────────────────────────

export type ErrorKind =
  | 'Config'
  | 'Auth'
  | 'RateLimited'
  | 'RetryableNetwork'
  | 'SourceUnavailable'
  | 'BadSourceData'
  | 'Storage'
  | 'PdfDownload'
  | 'PdfExtract'
  | 'Embedding'
  | 'Llm'
  | 'Render'
  | 'Delivery'

// ── SSE events ────────────────────────────────────────────────

export type PipelineEvent =
  | { Started: { run_id: string } }
  | { StageStart: { run_id: string; stage: StageName } }
  | { StageEnd: { run_id: string; stage: StageName; status: StageStatus; cache_hit: boolean; duration_ms: number } }
  | { Progress: { run_id: string; stage: StageName; current: number; total: number; message: string } }
  | { Ended: { run_id: string; status: RunStatus } }

// ── API request/response ──────────────────────────────────────

export interface ApiMessage {
  message: string
}

export interface StatsResponse {
  year: number
  month: number
  days: StatsDay[]
}

export interface StatsDay {
  day: number
  total: number
  success: number
  failed: number
}

export interface DateResponse {
  date: string
  runs: RunSummary[]
}

export interface RunSummary {
  run_id: string
  status: string // lowercase: "running" | "succeeded" | "failed" | "blocked" | "cancelled"
  started_at: string
  finished_at: string | null
  report_exists: boolean
  error: ErrorSummary | null
}

export interface ErrorSummary {
  kind: string
  message: string
}

export interface RunRequest {
  stages?: string[]
  from_run?: string
  date?: string
  dry_run?: boolean
  force_zotero_sync?: boolean
  force_rerank?: boolean
  force_read?: boolean
  force_send?: boolean
  max_candidates?: number
  no_email?: boolean
  send_email?: boolean
}

export interface RunStartResponse {
  run_id: string
  message: string
}

export interface ConfigResponse {
  path: string
  content: string
}

export interface CacheResponse {
  caches: CacheInfo[]
}

export interface CacheInfo {
  kind: string
  size_bytes: number
  size_display: string
  file_count: number
}

export interface CacheCleanResponse {
  message: string
  kind: string
  freed_bytes: number
}

export interface BulkDeleteResponse {
  message: string
  count: number
}

export interface StatusResponse {
  pipeline_running: boolean
}
