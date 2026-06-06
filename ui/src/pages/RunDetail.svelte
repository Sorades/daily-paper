<script lang="ts">
  import { getRun, deleteRun, resendRun } from '../api'
  import { navigate } from '../router.svelte'
  import { connect, disconnect, getPipelineState } from '../stores/pipeline.svelte'
  import type { RunManifest } from '../types'
  import StatusBadge from '../components/StatusBadge.svelte'
  import StageTimeline from '../components/StageTimeline.svelte'
  import Spinner from '../components/Spinner.svelte'
  import Modal from '../components/Modal.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'

  interface Props {
    runId: string
  }

  let { runId }: Props = $props()

  let manifest = $state<RunManifest | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)
  let sendLoading = $state(false)

  let pipeline = getPipelineState()

  // Modal state
  let modalOpen = $state(false)
  let modalTitle = $state('')
  let modalMessage = $state('')
  let modalDanger = $state(false)
  let pendingAction = $state<(() => Promise<void>) | null>(null)

  async function loadRun() {
    try {
      manifest = await getRun(runId)
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
    }
  }

  function confirmAction(title: string, message: string, action: () => Promise<void>, danger = false) {
    modalTitle = title
    modalMessage = message
    modalDanger = danger
    pendingAction = action
    modalOpen = true
  }

  async function handleConfirm() {
    if (pendingAction) {
      try {
        await pendingAction()
      } catch (e: unknown) {
        error = e instanceof Error ? e.message : String(e)
      }
    }
    modalOpen = false
    pendingAction = null
  }

  function handleCancel() {
    modalOpen = false
    pendingAction = null
  }

  function formatTime(iso: string | null): string {
    if (!iso) return '—'
    return new Date(iso).toLocaleString()
  }

  function formatDuration(start: string, end: string | null): string {
    const s = new Date(start).getTime()
    const e = end ? new Date(end).getTime() : Date.now()
    const ms = e - s
    if (ms < 1000) return `${ms}ms`
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
    const min = Math.floor(ms / 60_000)
    const sec = Math.round((ms % 60_000) / 1000)
    return `${min}m ${sec}s`
  }

  function handleDelete() {
    confirmAction(
      'Delete Run',
      `Delete run ${runId}? This cannot be undone.`,
      async () => {
        await deleteRun(runId)
        navigate('#/')
      },
      true,
    )
  }

  async function handleSend() {
    sendLoading = true
    error = null
    try {
      await resendRun(runId)
      await loadRun()
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      sendLoading = false
    }
  }

  $effect(() => {
    void runId
    loadRun()

    // Connect to SSE if run is still running
    if (manifest?.status === 'Running') {
      connect(runId)
    }

    return () => {
      disconnect()
    }
  })

  // Refresh manifest when SSE events arrive
  $effect(() => {
    // Subscribe to pipeline events
    const _ = pipeline.events
    if (pipeline.events.length > 0) {
      const last = pipeline.events[pipeline.events.length - 1]
      if ('StageEnd' in last || 'Ended' in last) {
        loadRun()
      }
    }
  })
</script>

<div class="run-detail">
  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if loading}
    <div class="loading"><Spinner /></div>
  {:else if manifest}
    {@const dl = manifest.date_window.label}
    <div class="header">
      <div class="title-row">
        <a href="#/date/{dl}" onclick={(e) => { e.preventDefault(); navigate(`#/date/${dl}`) }}>
          &larr; {dl}
        </a>
        <h2>Run Detail</h2>
        <StatusBadge status={manifest.status} />
        {#if pipeline.connected}
          <span class="live-dot">● LIVE</span>
        {/if}
      </div>
      <div class="actions">
        <button onclick={() => loadRun()}>Refresh</button>
        {#if manifest.stages.some((s) => s.stage === 'Render' && s.status === 'Succeeded')}
          <a class="report-link" href="/report/{runId}/report.html" target="_blank" rel="noopener">
            Open Report
          </a>
          <button onclick={handleSend} disabled={sendLoading}>
            {sendLoading ? 'Sending...' : 'Send Report'}
          </button>
        {/if}
        <button class="danger" onclick={handleDelete}>Delete</button>
      </div>
    </div>

    <div class="meta">
      <span><strong>Run ID:</strong> <code>{manifest.run_id}</code></span>
      <span><strong>Started:</strong> {formatTime(manifest.started_at)}</span>
      {#if manifest.finished_at}
        <span><strong>Duration:</strong> {formatDuration(manifest.started_at, manifest.finished_at)}</span>
      {/if}
      <span><strong>Config:</strong> <code>{manifest.config_hash.slice(0, 12)}</code></span>
    </div>

    {#if manifest.cli_overrides.length > 0}
      <div class="overrides">
        <strong>CLI Overrides:</strong>
        {#each manifest.cli_overrides as ov}
          <code>{ov.key}={ov.value}</code>
        {/each}
      </div>
    {/if}

    <StageTimeline
      stages={manifest.stages}
      progress={pipeline.latestProgress ? {
        stage: pipeline.latestStage!,
        current: pipeline.latestProgress.current,
        total: pipeline.latestProgress.total,
        message: pipeline.latestProgress.message,
      } : null}
    />

    {#if manifest.warnings.length > 0}
      <div class="warnings">
        <h3>Warnings</h3>
        {#each manifest.warnings as w}
          <div class="warning-item">
            <span class="warning-kind">{w.kind}:</span> {w.message}
          </div>
        {/each}
      </div>
    {/if}

    {#if manifest.error}
      <div class="run-error">
        <h3>Error</h3>
        <span class="error-kind">{manifest.error.kind}:</span>
        {manifest.error.message}
      </div>
    {/if}
  {/if}
</div>

<Modal
  open={modalOpen}
  title={modalTitle}
  confirmText="Delete"
  danger={modalDanger}
  onConfirm={handleConfirm}
  onCancel={handleCancel}
>
  <p>{modalMessage}</p>
</Modal>

<style>
  .run-detail {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: 48px;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .title-row a {
    font-size: 13px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .live-dot {
    font-size: 12px;
    color: var(--success);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .report-link {
    font-size: 13px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
    background: var(--bg);
    transition: background 0.15s;
  }
  .report-link:hover {
    background: var(--bg-secondary);
    text-decoration: none;
  }

  .meta {
    display: flex;
    gap: 20px;
    flex-wrap: wrap;
    font-size: 13px;
    color: var(--text-secondary);
  }

  .overrides {
    font-size: 13px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }

  .warnings {
    background: #fef3c7;
    border: 1px solid #fde68a;
    border-radius: 6px;
    padding: 12px 16px;
  }

  .warnings h3 {
    margin: 0 0 8px;
    font-size: 14px;
    color: #92400e;
  }

  .warning-item {
    font-size: 13px;
    color: #92400e;
  }

  .warning-kind {
    font-weight: 500;
  }

  .run-error {
    background: #fee2e2;
    border: 1px solid #fecaca;
    border-radius: 6px;
    padding: 12px 16px;
    font-size: 13px;
    color: #991b1b;
  }

  .run-error h3 {
    margin: 0 0 8px;
    font-size: 14px;
  }

  .error-kind {
    font-weight: 500;
  }
</style>
