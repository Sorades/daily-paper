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
  import Icon from '../components/Icon.svelte'

  interface Props {
    runId: string
  }

  let { runId }: Props = $props()

  let manifest = $state<RunManifest | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)
  let sendLoading = $state(false)
  let sendSuccessMsg = $state<string | null>(null)

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

  function confirmAction(
    title: string,
    message: string,
    action: () => Promise<void>,
    danger = false,
  ) {
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
    return new Date(iso).toLocaleString([], {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
  }

  function formatDuration(start: string, end: string | null): string {
    const s = new Date(start).getTime()
    const e = end ? new Date(end).getTime() : Date.now()
    const ms = Math.max(0, e - s)
    if (ms < 1000) return `${ms}ms`
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
    const min = Math.floor(ms / 60_000)
    const sec = Math.round((ms % 60_000) / 1000)
    return `${min}m ${sec}s`
  }

  function handleDelete() {
    confirmAction(
      'Delete Run',
      `Delete run ${runId}? All cached artifacts and report for this run will be permanently deleted.`,
      async () => {
        await deleteRun(runId)
        if (manifest) {
          navigate(`#/date/${manifest.date_window.label}`)
        } else {
          navigate('#/')
        }
      },
      true,
    )
  }

  async function handleSend() {
    sendLoading = true
    error = null
    sendSuccessMsg = null
    try {
      const res = await resendRun(runId)
      sendSuccessMsg = res.message || 'Report sent successfully!'
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

    if (manifest?.status === 'Running') {
      connect(runId)
    }

    return () => {
      disconnect()
    }
  })

  // Watch pipeline SSE events to refresh manifest
  $effect(() => {
    const _ = pipeline.events
    if (pipeline.events.length > 0) {
      const last = pipeline.events[pipeline.events.length - 1]
      if ('StageEnd' in last || 'Ended' in last) {
        loadRun()
      }
    }
  })

  let hasReport = $derived(
    manifest?.stages.some((s) => s.stage === 'Render' && s.status === 'Succeeded'),
  )
</script>

<div class="run-detail-page">
  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if sendSuccessMsg}
    <div class="success-banner">
      <Icon name="check" size={16} />
      <span>{sendSuccessMsg}</span>
      <button class="dismiss-btn" onclick={() => (sendSuccessMsg = null)}>
        <Icon name="x" size={14} />
      </button>
    </div>
  {/if}

  {#if loading}
    <div class="loading-state">
      <Spinner size="large" />
      <p>Loading run manifest...</p>
    </div>
  {:else if manifest}
    {@const dl = manifest.date_window.label}

    <!-- Top Action Bar -->
    <div class="page-header">
      <div class="header-left">
        <button
          class="ghost sm back-btn"
          onclick={() => navigate(`#/date/${dl}`)}
        >
          <Icon name="arrow-left" size={16} />
          <span>Back to {dl}</span>
        </button>

        <div class="title-with-badges">
          <h2>Run Manifest</h2>
          <StatusBadge status={manifest.status} size="md" />
          {#if pipeline.connected}
            <span class="live-pill">
              <span class="pulse-dot"></span>
              <span>LIVE SSE</span>
            </span>
          {/if}
        </div>
      </div>

      <div class="header-actions">
        <button class="secondary sm" onclick={() => loadRun()} title="Refresh Details">
          <Icon name="refresh" size={14} />
          <span>Refresh</span>
        </button>

        {#if hasReport}
          <a
            class="report-btn"
            href="/report/{runId}/report.html"
            target="_blank"
            rel="noopener"
          >
            <Icon name="file-text" size={14} />
            <span>Open Report</span>
            <Icon name="external" size={12} />
          </a>

          <button class="primary sm" onclick={handleSend} disabled={sendLoading}>
            {#if sendLoading}
              <Spinner size="small" color="#ffffff" />
              <span>Sending...</span>
            {:else}
              <Icon name="send" size={13} />
              <span>Send Report</span>
            {/if}
          </button>
        {/if}

        <button class="danger sm" onclick={handleDelete} title="Delete Run">
          <Icon name="trash" size={13} />
          <span>Delete</span>
        </button>
      </div>
    </div>

    <!-- Overview Meta Card -->
    <div class="meta-card">
      <div class="meta-item">
        <span class="meta-label">Run ID</span>
        <div class="meta-value-copy">
          <code>{manifest.run_id}</code>
        </div>
      </div>

      <div class="meta-item">
        <span class="meta-label">Target Date</span>
        <span class="meta-value font-mono">{dl}</span>
      </div>

      <div class="meta-item">
        <span class="meta-label">Started At</span>
        <span class="meta-value">{formatTime(manifest.started_at)}</span>
      </div>

      <div class="meta-item">
        <span class="meta-label">Duration</span>
        <span class="meta-value font-mono">
          {formatDuration(manifest.started_at, manifest.finished_at)}
        </span>
      </div>

      <div class="meta-item">
        <span class="meta-label">Config Hash</span>
        <code class="hash-code">{manifest.config_hash.slice(0, 10)}</code>
      </div>
    </div>

    <!-- Warnings / Errors alert cards -->
    {#if manifest.error}
      <div class="alert-box error">
        <div class="alert-icon">
          <Icon name="alert" size={18} />
        </div>
        <div class="alert-content">
          <div class="alert-title">{manifest.error.kind} Error</div>
          <div class="alert-message">{manifest.error.message}</div>
          {#if manifest.error.retryable}
            <span class="retryable-pill">Retryable condition</span>
          {/if}
        </div>
      </div>
    {/if}

    {#if manifest.warnings.length > 0}
      <div class="alert-box warning">
        <div class="alert-icon">
          <Icon name="alert" size={18} />
        </div>
        <div class="alert-content">
          <div class="alert-title">Warnings ({manifest.warnings.length})</div>
          <div class="warning-list">
            {#each manifest.warnings as w}
              <div class="warning-item">
                <span class="warning-kind">{w.kind}:</span>
                <span class="warning-msg">{w.message}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}

    {#if manifest.cli_overrides.length > 0}
      <div class="overrides-card">
        <span class="overrides-title">Execution CLI Overrides:</span>
        <div class="overrides-tags">
          {#each manifest.cli_overrides as ov}
            <span class="override-tag">
              <code>{ov.key} = {ov.value}</code>
            </span>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Stage Pipeline Timeline Section -->
    <div class="pipeline-section">
      <div class="section-header">
        <div class="section-title-wrap">
          <Icon name="zap" size={16} />
          <h3>Execution Pipeline Stages</h3>
        </div>
        <span class="section-sub">
          {manifest.stages.filter((s) => s.status === 'Succeeded').length} of {manifest.stages.length} completed
        </span>
      </div>

      <StageTimeline
        stages={manifest.stages}
        progress={pipeline.latestProgress
          ? {
              stage: pipeline.latestStage!,
              current: pipeline.latestProgress.current,
              total: pipeline.latestProgress.total,
              message: pipeline.latestProgress.message,
            }
          : null}
      />
    </div>
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
  .run-detail-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 80px 20px;
    gap: 16px;
    color: var(--text-secondary);
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 16px;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .back-btn {
    align-self: flex-start;
    padding: 0;
    color: var(--text-secondary);
  }

  .back-btn:hover {
    color: var(--primary);
  }

  .title-with-badges {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .live-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: var(--radius-full);
    background: var(--success-light);
    color: var(--success-text);
    border: 1px solid var(--success-border);
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success);
    animation: pulse 1.2s infinite;
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.3;
      transform: scale(0.8);
    }
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .report-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    border: 1px solid var(--primary-border);
    color: var(--primary);
    font-size: 13px;
    font-weight: 600;
    box-shadow: var(--shadow-xs);
    transition: all var(--transition-fast);
  }

  .report-btn:hover {
    background: var(--primary-light);
  }

  /* Meta Card */
  .meta-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 18px 24px;
    box-shadow: var(--shadow-sm);
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 16px;
  }

  .meta-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .meta-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .meta-value {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .font-mono {
    font-family: var(--font-mono);
  }

  .hash-code {
    width: fit-content;
  }

  /* Alert Boxes */
  .alert-box {
    display: flex;
    gap: 14px;
    padding: 16px 20px;
    border-radius: var(--radius-lg);
    border: 1px solid transparent;
  }

  .alert-box.error {
    background: var(--danger-light);
    border-color: var(--danger-border);
    color: var(--danger-text);
  }

  .alert-box.warning {
    background: var(--warning-light);
    border-color: var(--warning-border);
    color: var(--warning-text);
  }

  .alert-icon {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .alert-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .alert-title {
    font-weight: 600;
    font-size: 14px;
  }

  .alert-message {
    font-size: 13px;
    line-height: 1.5;
  }

  .retryable-pill {
    display: inline-block;
    margin-top: 4px;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    background: rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-full);
    width: fit-content;
  }

  .warning-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 4px;
  }

  .warning-item {
    font-size: 13px;
    line-height: 1.4;
  }

  .warning-kind {
    font-weight: 600;
  }

  /* Overrides */
  .overrides-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 12px 18px;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 13px;
  }

  .overrides-title {
    font-weight: 500;
    color: var(--text-secondary);
  }

  .overrides-tags {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .success-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 18px;
    border-radius: var(--radius-md);
    background: var(--success-light);
    color: var(--success-text);
    border: 1px solid var(--success-border);
    font-size: 13px;
    font-weight: 500;
  }

  .dismiss-btn {
    margin-left: auto;
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 4px;
    color: inherit;
    cursor: pointer;
  }

  /* Pipeline Timeline Section */
  .pipeline-section {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 24px;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--border);
  }

  .section-title-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--primary);
  }

  .section-title-wrap h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .section-sub {
    font-size: 13px;
    color: var(--text-secondary);
  }
</style>
