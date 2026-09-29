<script lang="ts">
  import { getDateRuns, deleteRun, deleteDate, bulkDeleteRuns } from '../api'
  import { navigate } from '../router.svelte'
  import type { DateResponse, RunSummary } from '../types'
  import StatusBadge from '../components/StatusBadge.svelte'
  import Spinner from '../components/Spinner.svelte'
  import Modal from '../components/Modal.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Icon from '../components/Icon.svelte'

  interface Props {
    date: string
  }

  let { date }: Props = $props()

  let data = $state<DateResponse | null>(null)
  let loading = $state(true)
  let actionLoading = $state(false)
  let error = $state<string | null>(null)
  let filterStatus = $state<'all' | 'succeeded' | 'failed' | 'running'>('all')

  // Selected runs for bulk deletion
  let selectedIds = $state<string[]>([])

  // Modal state
  let modalOpen = $state(false)
  let modalTitle = $state('')
  let modalMessage = $state('')
  let modalDanger = $state(false)
  let pendingAction = $state<(() => Promise<void>) | null>(null)

  let filteredRuns = $derived.by(() => {
    if (!data) return []
    if (filterStatus === 'all') return data.runs
    return data.runs.filter((r) => r.status.toLowerCase() === filterStatus)
  })

  let allSelected = $derived(
    filteredRuns.length > 0 && filteredRuns.every((r) => selectedIds.includes(r.run_id)),
  )

  function toggleSelectAll() {
    if (allSelected) {
      selectedIds = []
    } else {
      selectedIds = filteredRuns.map((r) => r.run_id)
    }
  }

  function toggleSelect(id: string) {
    if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((item) => item !== id)
    } else {
      selectedIds = [...selectedIds, id]
    }
  }

  async function loadData() {
    loading = true
    error = null
    selectedIds = []
    try {
      data = await getDateRuns(date)
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
      actionLoading = true
      try {
        await pendingAction()
        await loadData()
      } catch (e: unknown) {
        error = e instanceof Error ? e.message : String(e)
      } finally {
        actionLoading = false
      }
    }
    modalOpen = false
    pendingAction = null
  }

  function handleCancel() {
    if (actionLoading) return
    modalOpen = false
    pendingAction = null
  }

  function formatTime(iso: string | null): string {
    if (!iso) return '—'
    const d = new Date(iso)
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
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

  function handleDeleteRun(runId: string) {
    confirmAction(
      'Delete Run',
      `Are you sure you want to delete run ${runId}? This will remove the execution manifest and cached artifacts.`,
      () => deleteRun(runId).then(() => {}),
      true,
    )
  }

  function handleDeleteSelected() {
    if (selectedIds.length === 0) return
    confirmAction(
      'Delete Selected Runs',
      `Delete ${selectedIds.length} selected run(s)? This action cannot be undone.`,
      () => bulkDeleteRuns(selectedIds).then(() => {}),
      true,
    )
  }

  function handleDeleteAll() {
    if (!data) return
    confirmAction(
      'Delete All Runs',
      `Delete ALL ${data.runs.length} runs for ${date}? This will clear all execution logs and reports for this day.`,
      () => deleteDate(date).then(() => {}),
      true,
    )
  }

  $effect(() => {
    void date
    loadData()
  })
</script>

<div class="date-detail-page">
  <!-- Header Breadcrumb & Actions -->
  <div class="page-header">
    <div class="header-left">
      <button class="ghost sm back-btn" onclick={() => navigate('#/')}>
        <Icon name="arrow-left" size={16} />
        <span>Back to Calendar</span>
      </button>

      <div class="date-heading">
        <h2>{date}</h2>
        {#if data}
          <span class="count-badge">{data.runs.length} {data.runs.length === 1 ? 'run' : 'runs'}</span>
        {/if}
      </div>
    </div>

    <div class="header-right">
      <button class="primary sm" onclick={() => navigate('#/run/new')}>
        <Icon name="play" size={13} />
        <span>New Run for {date}</span>
      </button>

      {#if data && data.runs.length > 0}
        <button class="danger sm" onclick={handleDeleteAll}>
          <Icon name="trash" size={13} />
          <span>Delete All</span>
        </button>
      {/if}
    </div>
  </div>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if loading}
    <div class="loading-state">
      <Spinner size="large" />
      <p>Loading runs for {date}...</p>
    </div>
  {:else if data}
    {#if data.runs.length === 0}
      <div class="empty-state">
        <div class="empty-icon-box">
          <Icon name="calendar" size={32} />
        </div>
        <h3>No Runs Recorded</h3>
        <p>There are no pipeline runs recorded on {date}. You can start a new run right now.</p>
        <button class="primary" onclick={() => navigate('#/run/new')}>
          <Icon name="play" size={14} />
          <span>Run Pipeline</span>
        </button>
      </div>
    {:else}
      <!-- Filter and Bulk Action Bar -->
      <div class="filter-bar">
        <div class="filter-tabs">
          <button
            class="filter-tab"
            class:active={filterStatus === 'all'}
            onclick={() => (filterStatus = 'all')}
          >
            All ({data.runs.length})
          </button>
          <button
            class="filter-tab"
            class:active={filterStatus === 'succeeded'}
            onclick={() => (filterStatus = 'succeeded')}
          >
            Succeeded ({data.runs.filter((r) => r.status.toLowerCase() === 'succeeded').length})
          </button>
          <button
            class="filter-tab"
            class:active={filterStatus === 'failed'}
            onclick={() => (filterStatus = 'failed')}
          >
            Failed ({data.runs.filter((r) => r.status.toLowerCase() === 'failed').length})
          </button>
        </div>

        {#if selectedIds.length > 0}
          <div class="bulk-actions">
            <span class="selected-count">{selectedIds.length} selected</span>
            <button class="danger sm" onclick={handleDeleteSelected}>
              <Icon name="trash" size={13} />
              <span>Delete Selected</span>
            </button>
          </div>
        {/if}
      </div>

      <!-- Runs List Table/Cards -->
      <div class="runs-container">
        <div class="table-header-row">
          <div class="col-check">
            <input
              type="checkbox"
              checked={allSelected}
              onchange={toggleSelectAll}
              aria-label="Select all runs"
            />
          </div>
          <div class="col-status">Status</div>
          <div class="col-id">Run ID</div>
          <div class="col-time">Timeline</div>
          <div class="col-duration">Duration</div>
          <div class="col-report">Report</div>
          <div class="col-actions">Actions</div>
        </div>

        {#each filteredRuns as run (run.run_id)}
          {@const isChecked = selectedIds.includes(run.run_id)}
          <div class="run-card" class:checked={isChecked}>
            <div class="col-check">
              <input
                type="checkbox"
                checked={isChecked}
                onchange={() => toggleSelect(run.run_id)}
                aria-label="Select run"
              />
            </div>

            <div class="col-status">
              <StatusBadge status={run.status} size="sm" />
            </div>

            <div class="col-id">
              <a
                href="#/run/{run.run_id}"
                class="run-id-link"
                onclick={(e) => {
                  e.preventDefault()
                  navigate(`#/run/${run.run_id}`)
                }}
              >
                {run.run_id}
              </a>
            </div>

            <div class="col-time">
              <span class="time-main">{formatTime(run.started_at)}</span>
              {#if run.finished_at}
                <span class="time-sub">&rarr; {formatTime(run.finished_at)}</span>
              {/if}
            </div>

            <div class="col-duration">
              <span class="duration-text">{formatDuration(run.started_at, run.finished_at)}</span>
            </div>

            <div class="col-report">
              {#if run.report_exists}
                <a
                  class="report-chip"
                  href="/report/{run.run_id}/report.html"
                  target="_blank"
                  rel="noopener"
                  title="View HTML Digest"
                >
                  <Icon name="file-text" size={13} />
                  <span>View</span>
                  <Icon name="external" size={11} />
                </a>
              {:else}
                <span class="no-report">—</span>
              {/if}
            </div>

            <div class="col-actions">
              <button
                class="secondary sm"
                onclick={() => navigate(`#/run/${run.run_id}`)}
                title="View Run Details"
              >
                Details
              </button>
              <button
                class="ghost sm trash-action-btn"
                onclick={() => handleDeleteRun(run.run_id)}
                title="Delete Run"
              >
                <Icon name="trash" size={14} />
              </button>
            </div>

            {#if run.error}
              <div class="error-strip">
                <Icon name="alert" size={14} />
                <span class="error-kind-pill">{run.error.kind}</span>
                <span class="error-msg">{run.error.message}</span>
              </div>
            {/if}
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
  loading={actionLoading}
  onConfirm={handleConfirm}
  onCancel={handleCancel}
>
  <p>{modalMessage}</p>
</Modal>

<style>
  .date-detail-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
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

  .date-heading {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .count-badge {
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    padding: 2px 8px;
    border-radius: var(--radius-full);
    font-size: 12px;
    font-weight: 600;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 10px;
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

  /* Empty State */
  .empty-state {
    background: var(--bg-surface);
    border: 1px dashed var(--border);
    border-radius: var(--radius-xl);
    padding: 60px 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 12px;
  }

  .empty-icon-box {
    width: 60px;
    height: 60px;
    border-radius: var(--radius-full);
    background: var(--bg-secondary);
    color: var(--text-tertiary);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 4px;
  }

  .empty-state h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .empty-state p {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
    max-width: 360px;
  }

  /* Filter bar */
  .filter-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .filter-tabs {
    display: flex;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 3px;
    gap: 2px;
  }

  .filter-tab {
    background: transparent;
    border: none;
    box-shadow: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .filter-tab:hover {
    color: var(--text-primary);
  }

  .filter-tab.active {
    background: var(--bg-secondary);
    color: var(--primary);
    font-weight: 600;
  }

  .bulk-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .selected-count {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  /* Runs Table Container */
  .runs-container {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .table-header-row {
    display: grid;
    grid-template-columns: 40px 110px 1.4fr 1.2fr 90px 100px 120px;
    align-items: center;
    padding: 12px 18px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .run-card {
    display: grid;
    grid-template-columns: 40px 110px 1.4fr 1.2fr 90px 100px 120px;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 13px;
    transition: background var(--transition-fast);
    position: relative;
  }

  .run-card:last-child {
    border-bottom: none;
  }

  .run-card:hover {
    background: var(--bg-hover);
  }

  .run-card.checked {
    background: var(--primary-light);
  }

  .col-check input {
    cursor: pointer;
    width: 15px;
    height: 15px;
  }

  .run-id-link {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 13px;
    color: var(--primary);
  }

  .run-id-link:hover {
    text-decoration: underline;
  }

  .time-main {
    font-weight: 500;
    color: var(--text-primary);
  }

  .time-sub {
    font-size: 12px;
    color: var(--text-tertiary);
    margin-left: 4px;
  }

  .duration-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
  }

  .report-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: var(--radius-full);
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 500;
    text-decoration: none;
  }

  .report-chip:hover {
    border-color: var(--primary);
    color: var(--primary);
    background: var(--bg-surface);
  }

  .no-report {
    color: var(--text-tertiary);
  }

  .col-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
  }

  .trash-action-btn {
    padding: 5px;
    color: var(--text-tertiary);
  }

  .trash-action-btn:hover {
    color: var(--danger);
    background: var(--danger-light);
  }

  .error-strip {
    grid-column: 1 / -1;
    margin-top: 10px;
    padding: 8px 12px;
    border-radius: var(--radius-md);
    background: var(--danger-light);
    color: var(--danger-text);
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .error-kind-pill {
    font-weight: 600;
    background: rgba(239, 68, 68, 0.2);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
  }

  .error-msg {
    word-break: break-all;
  }

  @media (max-width: 900px) {
    .table-header-row {
      display: none;
    }
    .run-card {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      align-items: center;
      padding: 14px 16px;
    }
    .col-id {
      flex: 1;
    }
    .col-actions {
      margin-left: auto;
    }
  }
</style>
