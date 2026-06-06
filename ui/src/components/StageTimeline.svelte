<script lang="ts">
  import type { StageRecord, StageName } from '../types'
  import StatusBadge from './StatusBadge.svelte'

  interface Props {
    stages: StageRecord[]
    progress?: { stage: StageName; current: number; total: number; message: string } | null
  }

  let { stages, progress = null }: Props = $props()

  // Pipeline stage order for display
  const STAGE_ORDER: string[] = [
    'ZoteroSync', 'SourceFetch', 'Deduplicate', 'Embedding',
    'Rerank', 'PdfFetch', 'TextExtract', 'MetadataFetch',
    'DeepRead', 'Render', 'Send',
  ]

  let sortedStages = $derived(
    [...stages].sort((a, b) => STAGE_ORDER.indexOf(a.stage) - STAGE_ORDER.indexOf(b.stage)),
  )

  function formatDuration(start: string | null, end: string | null): string {
    if (!start) return ''
    const s = new Date(start).getTime()
    const e = end ? new Date(end).getTime() : Date.now()
    const ms = e - s
    if (ms < 1000) return `${ms}ms`
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
    const min = Math.floor(ms / 60_000)
    const sec = Math.round((ms % 60_000) / 1000)
    return `${min}m ${sec}s`
  }
</script>

<div class="timeline">
  {#each sortedStages as stage}
    {@const isRunning = stage.status === 'Running'}
    {@const showProgress = isRunning && progress && progress.stage === stage.stage}
    <div class="stage" class:running={isRunning} class:done={stage.status === 'Succeeded'}>
      <div class="stage-header">
        <span class="stage-name">{stage.stage}</span>
        <StatusBadge status={stage.status} />
        {#if stage.cache_hit}
          <span class="cache-hit" title="Cache hit">cached</span>
        {/if}
        <span class="duration">{formatDuration(stage.started_at, stage.finished_at)}</span>
      </div>

      {#if showProgress && progress}
        <div class="progress-row">
          <div class="progress-bar">
            <div
              class="progress-fill"
              style="width: {progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%"
            ></div>
          </div>
          <span class="progress-text">
            {progress.current}/{progress.total}
            {#if progress.message}
              — {progress.message}
            {/if}
          </span>
        </div>
      {/if}

      {#if stage.error}
        <div class="stage-error">
          <span class="error-kind">{stage.error.kind}:</span>
          {stage.error.message}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stage {
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    transition: background 0.15s;
  }

  .stage.running {
    border-color: var(--accent);
    background: #eff6ff;
  }

  .stage.done {
    border-color: var(--success);
  }

  .stage-header {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .stage-name {
    font-weight: 500;
    font-size: 14px;
    min-width: 120px;
  }

  .cache-hit {
    font-size: 11px;
    background: #dbeafe;
    color: #1e40af;
    padding: 1px 6px;
    border-radius: 8px;
  }

  .duration {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-secondary);
    font-family: var(--mono);
  }

  .progress-row {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .progress-bar {
    height: 6px;
    background: var(--bg-tertiary);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s ease;
    border-radius: 3px;
  }

  .progress-text {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .stage-error {
    margin-top: 6px;
    font-size: 13px;
    color: var(--danger);
  }

  .error-kind {
    font-weight: 500;
  }
</style>
