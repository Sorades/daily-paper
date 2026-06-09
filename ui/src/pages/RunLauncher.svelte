<script lang="ts">
  import { triggerRun, getStatus } from '../api'
  import { navigate } from '../router.svelte'
  import type { RunRequest, StageName } from '../types'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Spinner from '../components/Spinner.svelte'

  const ALL_STAGES: { value: string; label: string }[] = [
    { value: 'zotero-sync', label: 'Zotero Sync' },
    { value: 'source-fetch', label: 'Source Fetch' },
    { value: 'embedding', label: 'Embedding' },
    { value: 'rerank', label: 'Rerank' },
    { value: 'deep-read', label: 'Deep Read' },
    { value: 'render', label: 'Render' },
    { value: 'send', label: 'Send' },
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

  let submitting = $state(false)
  let error = $state<string | null>(null)
  let pipelineRunning = $state(false)

  // Set default date to today
  $effect(() => {
    const now = new Date()
    date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  })

  // Check if pipeline is already running
  $effect(() => {
    getStatus()
      .then((s) => { pipelineRunning = s.pipeline_running })
      .catch(() => {})
  })

  function toggleStage(value: string) {
    if (stages.includes(value)) {
      stages = stages.filter((s) => s !== value)
    } else {
      stages = [...stages, value]
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

    if (date) req.date = date
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

<div class="launcher">
  <h2>New Run</h2>

  {#if pipelineRunning}
    <div class="running-banner">
      A pipeline is already running. Please wait for it to finish.
    </div>
  {/if}

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  <form onsubmit={handleSubmit}>
    <div class="form-group">
      <label for="date">Date</label>
      <input id="date" type="text" bind:value={date} placeholder="YYYY-MM-DD" />
    </div>

    <fieldset class="form-group">
      <legend>Stages <span class="hint">(empty = full pipeline)</span></legend>
      <div class="checkbox-grid">
        {#each ALL_STAGES as s}
          <label class="checkbox-label">
            <input
              type="checkbox"
              checked={stages.includes(s.value)}
              onchange={() => toggleStage(s.value)}
            />
            {s.label}
          </label>
        {/each}
      </div>
    </fieldset>

    <div class="form-group">
      <label for="from-run">From Run <span class="hint">(optional)</span></label>
      <input id="from-run" type="text" bind:value={fromRun} placeholder="run_id" />
    </div>

    <fieldset class="form-group">
      <legend>Force Options</legend>
      <div class="checkbox-grid">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={forceZoteroSync} />
          Force Zotero Sync
        </label>
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={forceRerank} />
          Force Rerank
        </label>
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={forceRead} />
          Force Read
        </label>
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={forceSend} />
          Force Send
        </label>
      </div>
    </fieldset>

    <fieldset class="form-group">
      <legend>Email</legend>
      <label class="checkbox-label">
        <input type="checkbox" bind:checked={sendEmail} />
        Send email after completion
      </label>
    </fieldset>

    <div class="form-group">
      <label for="max-candidates">Max Candidates <span class="hint">(optional)</span></label>
      <input id="max-candidates" type="number" bind:value={maxCandidates} placeholder="default" min="1" />
    </div>

    <button type="submit" class="primary" disabled={submitting || pipelineRunning}>
      {#if submitting}
        <Spinner size="small" /> Starting...
      {:else}
        Start Pipeline
      {/if}
    </button>
  </form>
</div>

<style>
  .launcher {
    max-width: 640px;
  }

  h2 {
    margin: 0 0 16px;
    font-size: 18px;
  }

  .running-banner {
    background: #dbeafe;
    color: #1e40af;
    padding: 10px 16px;
    border-radius: 6px;
    font-size: 13px;
    margin-bottom: 16px;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label {
    font-size: 13px;
    font-weight: 500;
  }

  .hint {
    font-weight: 400;
    color: var(--text-secondary);
  }

  .checkbox-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 400;
    font-size: 13px;
    cursor: pointer;
  }

  .checkbox-label input[type='checkbox'] {
    cursor: pointer;
  }
</style>
