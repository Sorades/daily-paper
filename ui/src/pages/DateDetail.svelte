<script lang="ts">
  import { getDateRuns, deleteRun, deleteDate } from '../api'
  import { navigate } from '../router.svelte'
  import type { DateResponse, RunSummary } from '../types'
  import StatusBadge from '../components/StatusBadge.svelte'
  import Spinner from '../components/Spinner.svelte'
  import Modal from '../components/Modal.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'

  interface Props {
    date: string
  }

  let { date }: Props = $props()

  let data = $state<DateResponse | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)

  // Modal state
  let modalOpen = $state(false)
  let modalTitle = $state('')
  let modalMessage = $state('')
  let modalDanger = $state(false)
  let pendingAction = $state<(() => Promise<void>) | null>(null)

  async function loadData() {
    loading = true
    error = null
    try {
      data = await getDateRuns(date)
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
        await loadData()
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
    return new Date(iso).toLocaleTimeString()
  }

  function handleDeleteRun(runId: string) {
    confirmAction(
      'Delete Run',
      `Delete run ${runId}? This cannot be undone.`,
      () => deleteRun(runId).then(() => {}),
      true,
    )
  }

  function handleDeleteAll() {
    if (!data) return
    confirmAction(
      'Delete All Runs',
      `Delete all ${data.runs.length} runs for ${date}? This cannot be undone.`,
      () => deleteDate(date).then(() => {}),
      true,
    )
  }

  $effect(() => {
    void date
    loadData()
  })
</script>

<div class="date-detail">
  <div class="header">
    <div class="title-row">
      <a href="#/" onclick={(e) => { e.preventDefault(); navigate('#/') }}>&larr; Back</a>
      <h2>{date}</h2>
    </div>
    {#if data && data.runs.length > 0}
      <button class="danger" onclick={handleDeleteAll}>Delete All</button>
    {/if}
  </div>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if loading}
    <div class="loading"><Spinner /></div>
  {:else if data}
    {#if data.runs.length === 0}
      <p class="empty">No runs for this date.</p>
    {:else}
      <div class="run-list">
        {#each data.runs as run}
          <div class="run-row">
            <div class="run-main">
              <StatusBadge status={run.status} />
              <span class="run-id">{run.run_id}</span>
              <span class="run-time">{formatTime(run.started_at)}</span>
              {#if run.finished_at}
                <span class="run-time">&rarr; {formatTime(run.finished_at)}</span>
              {/if}
              {#if run.report_exists}
                <a
                  class="report-link"
                  href="/report/{run.run_id}/report.html"
                  target="_blank"
                  rel="noopener"
                >
                  Report
                </a>
              {/if}
            </div>
            {#if run.error}
              <div class="run-error">
                <span class="error-kind">{run.error.kind}:</span>
                {run.error.message}
              </div>
            {/if}
            <div class="run-actions">
              <button onclick={() => navigate(`#/run/${run.run_id}`)}>Detail</button>
              <button
                class="danger"
                onclick={() => handleDeleteRun(run.run_id)}
              >
                Delete
              </button>
            </div>
          </div>
        {/each}
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
  .date-detail {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .title-row a {
    font-size: 13px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: 48px;
  }

  .empty {
    color: var(--text-secondary);
    padding: 24px 0;
  }

  .run-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .run-row {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .run-main {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .run-id {
    font-family: var(--mono);
    font-size: 13px;
    color: var(--text-secondary);
  }

  .run-time {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .report-link {
    font-size: 12px;
    background: var(--bg-secondary);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .run-error {
    font-size: 13px;
    color: var(--danger);
    padding-left: 4px;
  }

  .error-kind {
    font-weight: 500;
  }

  .run-actions {
    display: flex;
    gap: 8px;
  }
</style>
