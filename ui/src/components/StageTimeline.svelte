<script lang="ts">
  import type { StageRecord, StageName } from '../types'
  import StatusBadge from './StatusBadge.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    stages: StageRecord[]
    progress?: { stage: StageName; current: number; total: number; message: string } | null
  }

  let { stages, progress = null }: Props = $props()

  // Standard Pipeline order
  const STAGE_ORDER: StageName[] = [
    'ZoteroSync',
    'SourceFetch',
    'Embedding',
    'Rerank',
    'PdfFetch',
    'TextExtract',
    'MetadataFetch',
    'DeepRead',
    'Render',
    'Send',
  ]

  const STAGE_DESCRIPTIONS: Record<string, string> = {
    ZoteroSync: 'Synchronize paper collections and interest tags from Zotero',
    SourceFetch: 'Fetch recent RSS / API preprints from arXiv categories',
    Embedding: 'Compute vector embeddings for newly fetched candidate papers',
    Rerank: 'Cosine similarity scoring & Top-K ranking based on user profile',
    PdfFetch: 'Download full-text PDF documents for Top-ranked papers',
    TextExtract: 'Extract text and section titles from downloaded PDFs',
    MetadataFetch: 'Retrieve supplemental publication metadata and author info',
    DeepRead: 'Execute LLM deep analysis and structured digest extraction',
    Render: 'Generate self-contained HTML daily briefing document',
    Send: 'Deliver digest report via SMTP email or configured push targets',
  }

  let sortedStages = $derived(
    [...stages].sort((a, b) => STAGE_ORDER.indexOf(a.stage) - STAGE_ORDER.indexOf(b.stage)),
  )

  function formatDuration(start: string | null, end: string | null): string {
    if (!start) return ''
    const s = new Date(start).getTime()
    const e = end ? new Date(end).getTime() : Date.now()
    const ms = Math.max(0, e - s)
    if (ms < 1000) return `${ms}ms`
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
    const min = Math.floor(ms / 60_000)
    const sec = Math.round((ms % 60_000) / 1000)
    return `${min}m ${sec}s`
  }

  function getStepIcon(status: string) {
    switch (status.toLowerCase()) {
      case 'succeeded':
        return 'check'
      case 'running':
        return 'refresh'
      case 'failed':
        return 'alert'
      default:
        return 'clock'
    }
  }
</script>

<div class="timeline-container">
  <div class="timeline-list">
    {#each sortedStages as stage, index}
      {@const isRunning = stage.status === 'Running'}
      {@const isDone = stage.status === 'Succeeded'}
      {@const isFailed = stage.status === 'Failed'}
      {@const showProgress = isRunning && progress && progress.stage === stage.stage}
      {@const isLast = index === sortedStages.length - 1}

      <div
        class="timeline-item"
        class:is-running={isRunning}
        class:is-done={isDone}
        class:is-failed={isFailed}
      >
        <!-- Connector Line & Node -->
        <div class="node-track">
          <div class="node-circle" class:pulse={isRunning}>
            <Icon name={getStepIcon(stage.status)} size={13} />
          </div>
          {#if !isLast}
            <div class="node-connector" class:done={isDone}></div>
          {/if}
        </div>

        <!-- Stage Card Content -->
        <div class="stage-card">
          <div class="stage-header">
            <div class="stage-meta">
              <span class="stage-index">#{index + 1}</span>
              <h4 class="stage-name">{stage.stage}</h4>
              <StatusBadge status={stage.status} size="sm" />
              {#if stage.cache_hit}
                <span class="cache-pill" title="Loaded from state cache">
                  <Icon name="database" size={11} />
                  <span>Cached</span>
                </span>
              {/if}
            </div>

            <div class="stage-time">
              {#if stage.started_at}
                <span class="duration-badge">
                  <Icon name="clock" size={12} />
                  <span>{formatDuration(stage.started_at, stage.finished_at)}</span>
                </span>
              {/if}
            </div>
          </div>

          <div class="stage-desc">
            {STAGE_DESCRIPTIONS[stage.stage] || 'Pipeline processing stage'}
          </div>

          {#if showProgress && progress}
            <div class="progress-box">
              <div class="progress-info">
                <span class="progress-message">
                  {progress.message || 'Processing items...'}
                </span>
                <span class="progress-count">
                  {progress.current} / {progress.total}
                </span>
              </div>
              <div class="progress-bar-bg">
                <div
                  class="progress-bar-fill"
                  style:width="{progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%"
                ></div>
              </div>
            </div>
          {/if}

          {#if stage.error}
            <div class="stage-error-box">
              <Icon name="alert" size={15} />
              <div class="stage-error-content">
                <span class="error-kind">{stage.error.kind}</span>
                <p class="error-message">{stage.error.message}</p>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .timeline-container {
    width: 100%;
  }

  .timeline-list {
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .timeline-item {
    display: flex;
    gap: 16px;
    position: relative;
    padding-bottom: 20px;
  }

  .timeline-item:last-child {
    padding-bottom: 0;
  }

  /* Node Track */
  .node-track {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 32px;
    flex-shrink: 0;
  }

  .node-circle {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--bg-surface);
    border: 2px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    z-index: 2;
    transition: all var(--transition-fast);
  }

  .timeline-item.is-done .node-circle {
    border-color: var(--success);
    background: var(--success-light);
    color: var(--success);
  }

  .timeline-item.is-running .node-circle {
    border-color: var(--primary);
    background: var(--primary-light);
    color: var(--primary);
    box-shadow: 0 0 0 4px rgba(99, 102, 241, 0.15);
  }

  .timeline-item.is-failed .node-circle {
    border-color: var(--danger);
    background: var(--danger-light);
    color: var(--danger);
  }

  .node-connector {
    width: 2px;
    flex: 1;
    background: var(--border);
    margin-top: 4px;
    margin-bottom: 4px;
    min-height: 24px;
    transition: background var(--transition-fast);
  }

  .node-connector.done {
    background: var(--success-border);
  }

  /* Card */
  .stage-card {
    flex: 1;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 14px 18px;
    box-shadow: var(--shadow-xs);
    transition: all var(--transition-fast);
  }

  .timeline-item.is-running .stage-card {
    border-color: var(--primary-border);
    background: var(--primary-light);
    box-shadow: var(--shadow-sm);
  }

  .timeline-item.is-failed .stage-card {
    border-color: var(--danger-border);
  }

  .stage-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 10px;
  }

  .stage-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .stage-index {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
  }

  .stage-name {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .cache-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 500;
    padding: 2px 7px;
    border-radius: var(--radius-full);
    background: var(--info-light);
    color: var(--info-text);
    border: 1px solid var(--info-border);
  }

  .duration-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
  }

  .stage-desc {
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  /* Progress Box */
  .progress-box {
    margin-top: 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .progress-info {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .progress-message {
    font-weight: 500;
    color: var(--text-primary);
  }

  .progress-count {
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--primary);
  }

  .progress-bar-bg {
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .progress-bar-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--primary) 0%, #818cf8 100%);
    border-radius: var(--radius-full);
    transition: width 0.3s ease;
  }

  /* Error Box */
  .stage-error-box {
    margin-top: 10px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--danger-light);
    color: var(--danger-text);
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 12px;
  }

  .stage-error-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .error-kind {
    font-weight: 700;
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.04em;
  }

  .error-message {
    margin: 0;
    line-height: 1.4;
    word-break: break-word;
  }
</style>
