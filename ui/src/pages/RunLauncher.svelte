<script lang="ts">
  import { triggerRun, getStatus } from '../api'
  import { navigate } from '../router.svelte'
  import type { RunRequest } from '../types'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Spinner from '../components/Spinner.svelte'
  import Icon from '../components/Icon.svelte'

  interface StageOption {
    value: string
    label: string
    desc: string
    icon: 'paper' | 'refresh' | 'database' | 'zap' | 'file-text' | 'send'
  }

  const ALL_STAGES: StageOption[] = [
    { value: 'zotero-sync', label: 'Zotero Sync', desc: 'Sync collections and tags from Zotero profile', icon: 'refresh' },
    { value: 'source-fetch', label: 'Source Fetch', desc: 'Fetch latest arXiv preprints', icon: 'paper' },
    { value: 'embedding', label: 'Embedding', desc: 'Compute embeddings for candidates', icon: 'database' },
    { value: 'rerank', label: 'Rerank', desc: 'Semantic profile match & Top-K scoring', icon: 'zap' },
    { value: 'deep-read', label: 'Deep Read', desc: 'LLM paper extraction & structured insights', icon: 'file-text' },
    { value: 'render', label: 'Render', desc: 'Compile self-contained HTML report', icon: 'file-text' },
    { value: 'send', label: 'Send', desc: 'Deliver via email/push channels', icon: 'send' },
  ]

  // Form state
  let date = $state('')
  let stages = $state<string[]>([])
  let fromRun = $state('')
  let forceZoteroSync = $state(false)
  let forceRerank = $state(false)
  let forceRead = $state(false)
  let forceSend = $state(false)
  let sendEmail = $state(false)
  let maxCandidates = $state('')

  let showAdvanced = $state(false)
  let submitting = $state(false)
  let error = $state<string | null>(null)
  let pipelineRunning = $state(false)

  // Default to today
  $effect(() => {
    const now = new Date()
    date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  })

  // Check running status
  $effect(() => {
    getStatus()
      .then((s) => {
        pipelineRunning = s.pipeline_running
      })
      .catch(() => {})
  })

  function toggleStage(value: string) {
    if (stages.includes(value)) {
      stages = stages.filter((s) => s !== value)
    } else {
      stages = [...stages, value]
    }
  }

  function selectPreset(type: 'full' | 'quick' | 'rerank-only') {
    if (type === 'full') {
      stages = []
    } else if (type === 'quick') {
      stages = ['rerank', 'deep-read', 'render']
    } else if (type === 'rerank-only') {
      stages = ['rerank', 'render']
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault()
    submitting = true
    error = null

    const req: RunRequest = {
      dry_run: !sendEmail,
      no_email: !sendEmail,
      send_email: sendEmail,
    }

    if (date.trim()) req.date = date.trim()
    if (stages.length > 0) req.stages = stages
    if (fromRun.trim()) req.from_run = fromRun.trim()
    if (forceZoteroSync) req.force_zotero_sync = true
    if (forceRerank) req.force_rerank = true
    if (forceRead) req.force_read = true
    if (forceSend) req.force_send = true
    if (maxCandidates.trim()) req.max_candidates = parseInt(maxCandidates, 10)

    try {
      const res = await triggerRun(req)
      navigate(`#/run/${res.run_id}`)
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      submitting = false
    }
  }
</script>

<div class="launcher-page">
  <div class="launcher-header">
    <button class="ghost sm back-btn" onclick={() => navigate('#/')}>
      <Icon name="arrow-left" size={16} />
      <span>Back to Dashboard</span>
    </button>
    <h2>Launch New Pipeline Run</h2>
    <p class="launcher-sub">
      Trigger paper scraping, LLM deep analysis, and daily briefing report generation.
    </p>
  </div>

  {#if pipelineRunning}
    <div class="running-callout">
      <div class="callout-icon">
        <Icon name="alert" size={18} />
      </div>
      <div class="callout-body">
        <strong>A pipeline is currently executing!</strong>
        <p>Multiple concurrent pipeline runs are prevented. You can track current progress in the live view.</p>
        <button class="secondary sm" onclick={() => navigate('#/logs')}>
          View Active Logs
        </button>
      </div>
    </div>
  {/if}

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  <form onsubmit={handleSubmit} class="launcher-form">
    <!-- Preset Selection Bar -->
    <div class="form-card">
      <div class="card-title-row">
        <Icon name="zap" size={16} />
        <h3>Pipeline Mode & Presets</h3>
      </div>
      <div class="preset-buttons">
        <button
          type="button"
          class="preset-btn"
          class:active={stages.length === 0}
          onclick={() => selectPreset('full')}
        >
          <span class="preset-name">Full Pipeline (Default)</span>
          <span class="preset-detail">Zotero sync &rarr; Fetch &rarr; Embed &rarr; Rerank &rarr; DeepRead &rarr; Render</span>
        </button>

        <button
          type="button"
          class="preset-btn"
          class:active={stages.length === 3 && stages.includes('rerank')}
          onclick={() => selectPreset('quick')}
        >
          <span class="preset-name">Re-Analyze & Render</span>
          <span class="preset-detail">Rerank &rarr; Deep Read &rarr; Render (uses cached sources)</span>
        </button>

        <button
          type="button"
          class="preset-btn"
          class:active={stages.length === 2 && stages.includes('rerank') && stages.includes('render')}
          onclick={() => selectPreset('rerank-only')}
        >
          <span class="preset-name">Re-Rank Only</span>
          <span class="preset-detail">Fast re-scoring & overview briefing</span>
        </button>
      </div>

      <!-- Stage Checklist Grid -->
      <div class="stages-container">
        <div class="stages-header">
          <span class="label-text">Or Customize Selected Stages:</span>
          {#if stages.length > 0}
            <button
              type="button"
              class="ghost sm reset-stages-btn"
              onclick={() => (stages = [])}
            >
              Reset to Full Pipeline
            </button>
          {/if}
        </div>

        <div class="stage-cards-grid">
          {#each ALL_STAGES as st}
            {@const isSelected = stages.length === 0 || stages.includes(st.value)}
            <div
              class="stage-select-card"
              class:selected={isSelected}
              role="button"
              tabindex="0"
              onclick={() => toggleStage(st.value)}
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') toggleStage(st.value) }}
            >
              <div class="stage-check-wrap">
                <input
                  type="checkbox"
                  checked={isSelected}
                  onchange={() => toggleStage(st.value)}
                  tabindex="-1"
                />
              </div>
              <div class="stage-card-body">
                <div class="stage-card-title">
                  <Icon name={st.icon} size={14} />
                  <span>{st.label}</span>
                </div>
                <span class="stage-card-desc">{st.desc}</span>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- Basic Execution Settings -->
    <div class="form-card">
      <div class="card-title-row">
        <Icon name="calendar" size={16} />
        <h3>Target Date & Source</h3>
      </div>

      <div class="input-grid">
        <div class="input-group">
          <label for="date-input">Target Date (YYYY-MM-DD)</label>
          <input
            id="date-input"
            type="text"
            bind:value={date}
            placeholder="2026-06-06"
            class="font-mono"
            required
          />
          <span class="input-hint">The published date window of papers to digest</span>
        </div>

        <div class="input-group">
          <label for="from-run-input">From Source Run ID (Optional)</label>
          <input
            id="from-run-input"
            type="text"
            bind:value={fromRun}
            placeholder="e.g. 20260606-120000-abcdef"
            class="font-mono"
          />
          <span class="input-hint">Leave blank to automatically select latest compatible run</span>
        </div>
      </div>
    </div>

    <!-- Delivery & Candidate Limits -->
    <div class="form-card">
      <div class="card-title-row">
        <Icon name="send" size={16} />
        <h3>Notification & Delivery</h3>
      </div>

      <div class="checkbox-option-box">
        <label class="custom-checkbox-row">
          <input type="checkbox" bind:checked={sendEmail} />
          <div class="checkbox-label-text">
            <strong>Send email delivery on completion</strong>
            <span>Deliver the generated briefing to configured recipient addresses</span>
          </div>
        </label>
      </div>

      <div class="input-group max-candidates-group">
        <label for="max-candidates-input">Candidate Papers Cap (Optional)</label>
        <input
          id="max-candidates-input"
          type="number"
          bind:value={maxCandidates}
          placeholder="Default from config"
          min="1"
          max="500"
        />
        <span class="input-hint">Override maximum number of preprints fetched for this run</span>
      </div>
    </div>

    <!-- Advanced Cache & Force Overrides (Collapsible) -->
    <div class="form-card">
      <button
        type="button"
        class="ghost advanced-toggle-btn"
        onclick={() => (showAdvanced = !showAdvanced)}
      >
        <div class="card-title-row">
          <Icon name="settings" size={16} />
          <h3>Advanced Cache Bypass & Force Flags</h3>
        </div>
        <Icon name={showAdvanced ? 'x' : 'chevron-right'} size={16} />
      </button>

      {#if showAdvanced}
        <div class="advanced-content">
          <p class="advanced-desc">
            Check these options if you want to bypass cache hits and force re-execution of specific stages.
          </p>
          <div class="force-grid">
            <label class="custom-checkbox-row">
              <input type="checkbox" bind:checked={forceZoteroSync} />
              <div class="checkbox-label-text">
                <strong>Force Zotero Sync</strong>
                <span>Ignore collection cache and re-download all items</span>
              </div>
            </label>

            <label class="custom-checkbox-row">
              <input type="checkbox" bind:checked={forceRerank} />
              <div class="checkbox-label-text">
                <strong>Force Rerank</strong>
                <span>Recalculate similarity matrix and ranking</span>
              </div>
            </label>

            <label class="custom-checkbox-row">
              <input type="checkbox" bind:checked={forceRead} />
              <div class="checkbox-label-text">
                <strong>Force Deep Read</strong>
                <span>Re-query LLM for all paper analyses</span>
              </div>
            </label>

            <label class="custom-checkbox-row">
              <input type="checkbox" bind:checked={forceSend} />
              <div class="checkbox-label-text">
                <strong>Force Send</strong>
                <span>Bypass delivery receipt check even if already sent</span>
              </div>
            </label>
          </div>
        </div>
      {/if}
    </div>

    <!-- Bottom Submit Bar -->
    <div class="submit-bar">
      <button
        type="button"
        class="secondary lg"
        onclick={() => navigate('#/')}
      >
        Cancel
      </button>

      <button
        type="submit"
        class="primary lg submit-btn"
        disabled={submitting || pipelineRunning}
      >
        {#if submitting}
          <Spinner size="small" color="#ffffff" />
          <span>Starting Pipeline...</span>
        {:else}
          <Icon name="play" size={16} />
          <span>Start Execution</span>
        {/if}
      </button>
    </div>
  </form>
</div>

<style>
  .launcher-page {
    max-width: 800px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .launcher-header {
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

  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .launcher-sub {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
  }

  /* Running Callout */
  .running-callout {
    display: flex;
    gap: 14px;
    padding: 16px 20px;
    border-radius: var(--radius-lg);
    background: var(--warning-light);
    border: 1px solid var(--warning-border);
    color: var(--warning-text);
  }

  .callout-icon {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .callout-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }

  .callout-body p {
    margin: 0;
  }

  /* Form */
  .launcher-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .form-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 20px 24px;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--primary);
  }

  .card-title-row h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  /* Presets */
  .preset-buttons {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
  }

  .preset-btn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    text-align: left;
    transition: all var(--transition-fast);
    cursor: pointer;
    box-shadow: none;
  }

  .preset-btn:hover {
    border-color: var(--primary-border);
    background: var(--bg-hover);
  }

  .preset-btn.active {
    background: var(--primary-light);
    border-color: var(--primary);
  }

  .preset-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .preset-btn.active .preset-name {
    color: var(--primary-text);
  }

  .preset-detail {
    font-size: 11px;
    color: var(--text-tertiary);
    margin-top: 4px;
  }

  /* Stage Select */
  .stages-container {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 4px;
  }

  .stages-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .label-text {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .reset-stages-btn {
    padding: 2px 6px;
    font-size: 11px;
  }

  .stage-cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px;
  }

  .stage-select-card {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-surface);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .stage-select-card:hover {
    background: var(--bg-hover);
  }

  .stage-select-card.selected {
    border-color: var(--primary);
    background: var(--primary-light);
  }

  .stage-check-wrap {
    margin-top: 2px;
  }

  .stage-card-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stage-card-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .stage-card-desc {
    font-size: 11px;
    color: var(--text-secondary);
  }

  /* Inputs */
  .input-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .input-group label {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .input-hint {
    font-size: 11px;
    color: var(--text-tertiary);
  }

  .font-mono {
    font-family: var(--font-mono);
  }

  .checkbox-option-box {
    background: var(--bg-secondary);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    border: 1px solid var(--border);
  }

  .custom-checkbox-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    cursor: pointer;
  }

  .custom-checkbox-row input {
    margin-top: 3px;
    cursor: pointer;
  }

  .checkbox-label-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .checkbox-label-text strong {
    font-size: 13px;
    color: var(--text-primary);
  }

  .checkbox-label-text span {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .max-candidates-group {
    max-width: 280px;
  }

  /* Advanced */
  .advanced-toggle-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0;
    color: inherit;
  }

  .advanced-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .advanced-desc {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0;
  }

  .force-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  /* Submit Bar */
  .submit-bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    padding-top: 8px;
  }

  .submit-btn {
    min-width: 160px;
  }

  @media (max-width: 640px) {
    .input-grid,
    .force-grid {
      grid-template-columns: 1fr;
    }
    .submit-bar {
      flex-direction: column-reverse;
    }
    .submit-bar button {
      width: 100%;
    }
  }
</style>
