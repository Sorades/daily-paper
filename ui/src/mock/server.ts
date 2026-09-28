// Mock API handlers for preview mode when backend is not running
import type { StatsResponse, DateResponse, CacheResponse, StatusResponse, RunManifest } from '../types'

export function setupMockApi() {
  const originalFetch = window.fetch

  window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
    
    // Check if it's an API request
    if (url.startsWith('/api/')) {
      const jsonResponse = (data: unknown, status = 200) => {
        return new Response(JSON.stringify(data), {
          status,
          headers: { 'Content-Type': 'application/json' },
        })
      }

      // /api/status
      if (url === '/api/status') {
        const data: StatusResponse = { pipeline_running: false }
        return jsonResponse(data)
      }

      // /api/stats/:year/:month
      if (url.match(/\/api\/stats\/\d+\/\d+/)) {
        const parts = url.split('/')
        const year = parseInt(parts[3])
        const month = parseInt(parts[4])
        const data: StatsResponse = {
          year,
          month,
          days: [
            { day: 1, total: 2, success: 2, failed: 0 },
            { day: 3, total: 1, success: 1, failed: 0 },
            { day: 5, total: 3, success: 2, failed: 1 },
            { day: 8, total: 1, success: 0, failed: 1 },
            { day: 12, total: 2, success: 2, failed: 0 },
            { day: 15, total: 4, success: 3, failed: 1 },
            { day: 19, total: 1, success: 1, failed: 0 },
            { day: 22, total: 2, success: 2, failed: 0 },
            { day: 26, total: 1, success: 1, failed: 0 },
            { day: 28, total: 2, success: 2, failed: 0 },
          ],
        }
        return jsonResponse(data)
      }

      // /api/date/:date
      if (url.match(/\/api\/date\/[\d-]+/)) {
        const parts = url.split('/')
        const date = parts[3]
        const data: DateResponse = {
          date,
          runs: [
            {
              run_id: `${date.replace(/-/g, '')}-080000-a1b2c3`,
              status: 'succeeded',
              started_at: `${date}T08:00:00.000Z`,
              finished_at: `${date}T08:04:12.000Z`,
              report_exists: true,
              error: null,
            },
            {
              run_id: `${date.replace(/-/g, '')}-143000-d4e5f6`,
              status: 'failed',
              started_at: `${date}T14:30:00.000Z`,
              finished_at: `${date}T14:31:05.000Z`,
              report_exists: false,
              error: {
                kind: 'RateLimited',
                message: 'OpenAI API rate limit exceeded (TPM tier-1 limit reached). Please retry later.',
              },
            },
          ],
        }
        return jsonResponse(data)
      }

      // /api/run/:id
      if (url.match(/\/api\/run\/[^\/]+$/) && !url.endsWith('/stream')) {
        const parts = url.split('/')
        const runId = parts[3]
        const data: RunManifest = {
          run_id: runId,
          parent_run_id: null,
          status: 'Succeeded',
          started_at: '2026-06-06T08:00:00.000Z',
          finished_at: '2026-06-06T08:04:12.000Z',
          config_hash: '9f8e7d6c5b4a3210ef',
          cli_overrides: [
            { key: 'max_candidates', value: '25' },
            { key: 'dry_run', value: 'true' },
          ],
          date_window: {
            start: '2026-06-05',
            end: '2026-06-06',
            label: '2026-06-06',
          },
          stages: [
            {
              stage: 'ZoteroSync',
              status: 'Succeeded',
              started_at: '2026-06-06T08:00:00.000Z',
              finished_at: '2026-06-06T08:00:15.000Z',
              cache_hit: false,
              input_hash: 'zotero-profile-hash-1',
              output_ref: 'cache/zotero/items.json',
              error: null,
            },
            {
              stage: 'SourceFetch',
              status: 'Succeeded',
              started_at: '2026-06-06T08:00:15.000Z',
              finished_at: '2026-06-06T08:00:45.000Z',
              cache_hit: true,
              input_hash: 'arxiv-cs-ai-query-hash',
              output_ref: 'cache/arxiv/query.json',
              error: null,
            },
            {
              stage: 'Embedding',
              status: 'Succeeded',
              started_at: '2026-06-06T08:00:45.000Z',
              finished_at: '2026-06-06T08:01:20.000Z',
              cache_hit: false,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'Rerank',
              status: 'Succeeded',
              started_at: '2026-06-06T08:01:20.000Z',
              finished_at: '2026-06-06T08:01:35.000Z',
              cache_hit: false,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'PdfFetch',
              status: 'Succeeded',
              started_at: '2026-06-06T08:01:35.000Z',
              finished_at: '2026-06-06T08:02:10.000Z',
              cache_hit: true,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'TextExtract',
              status: 'Succeeded',
              started_at: '2026-06-06T08:02:10.000Z',
              finished_at: '2026-06-06T08:02:30.000Z',
              cache_hit: false,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'MetadataFetch',
              status: 'Succeeded',
              started_at: '2026-06-06T08:02:30.000Z',
              finished_at: '2026-06-06T08:02:40.000Z',
              cache_hit: true,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'DeepRead',
              status: 'Succeeded',
              started_at: '2026-06-06T08:02:40.000Z',
              finished_at: '2026-06-06T08:03:50.000Z',
              cache_hit: false,
              input_hash: null,
              output_ref: null,
              error: null,
            },
            {
              stage: 'Render',
              status: 'Succeeded',
              started_at: '2026-06-06T08:03:50.000Z',
              finished_at: '2026-06-06T08:04:12.000Z',
              cache_hit: false,
              input_hash: null,
              output_ref: 'cache/reports/report.html',
              error: null,
            },
            {
              stage: 'Send',
              status: 'Skipped',
              started_at: null,
              finished_at: null,
              cache_hit: false,
              input_hash: null,
              output_ref: null,
              error: null,
            },
          ],
          warnings: [
            {
              kind: 'PdfWarning',
              message: 'Paper 2406.12345 has non-standard font encoding; fell back to raw stream extract.',
              context: null,
            },
          ],
          error: null,
        }
        return jsonResponse(data)
      }

      // /api/config
      if (url === '/api/config') {
        return jsonResponse({
          path: '/data/dsh/home/daily-paper/config.toml',
          content: `# Daily Paper Configuration\n[schedule]\nenabled = true\nhour = 7\nminute = 30\n\n[source.arxiv]\ncategories = ["cs.AI", "cs.CL", "cs.LG"]\nmax_results = 50\n\n[llm]\nmodel = "gemini-2.5-pro"\ntemperature = 0.2\nmax_tokens = 4096\n\n[email]\nenabled = false\nsmtp_host = "smtp.example.com"\nsmtp_port = 587\nfrom_address = "digest@example.com"\n`,
        })
      }

      // /api/cache
      if (url === '/api/cache') {
        const data: CacheResponse = {
          caches: [
            { kind: 'arxiv', size_bytes: 14200000, size_display: '13.5 MB', file_count: 84 },
            { kind: 'embeddings', size_bytes: 48900000, size_display: '46.6 MB', file_count: 320 },
            { kind: 'models', size_bytes: 134217728, size_display: '128.0 MB', file_count: 4 },
            { kind: 'papers', size_bytes: 382400000, size_display: '364.7 MB', file_count: 92 },
            { kind: 'rerank', size_bytes: 3200000, size_display: '3.1 MB', file_count: 28 },
            { kind: 'zotero', size_bytes: 840000, size_display: '820.3 KB', file_count: 12 },
            { kind: 'deliveries', size_bytes: 42000, size_display: '41.0 KB', file_count: 19 },
            { kind: 'reports', size_bytes: 18500000, size_display: '17.6 MB', file_count: 36 },
            { kind: 'runs', size_bytes: 6200000, size_display: '5.9 MB', file_count: 48 },
          ],
        }
        return jsonResponse(data)
      }

      // Default mock success for DELETE/PUT/POST
      return jsonResponse({ message: 'Success (Mock Mode)' })
    }

    return originalFetch(input, init)
  }
}
