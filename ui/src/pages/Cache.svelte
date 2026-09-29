<script lang="ts">
  import { getCache, cleanCache } from '../api'
  import type { CacheResponse, CacheInfo } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import Modal from '../components/Modal.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Icon from '../components/Icon.svelte'

  let data = $state<CacheResponse | null>(null)
  let loading = $state(true)
  let actionLoading = $state(false)
  let error = $state<string | null>(null)
  let successMsg = $state<string | null>(null)

  // Modal state
  let modalOpen = $state(false)
  let modalTitle = $state('')
  let modalMessage = $state('')
  let pendingKind = $state<string | null>(null)

  const KIND_DESCRIPTIONS: Record<string, string> = {
    arxiv: 'Raw API responses and RSS search results from arXiv',
    embeddings: 'Vector embeddings generated for paper titles and abstracts',
    models: 'Cached local models or fastembed model weights',
    papers: 'Downloaded source paper preprints and metadata',
    rerank: 'Computed reranker scores and similarity caches',
    zotero: 'Cached Zotero library collections and item attachments',
    deliveries: 'Receipt records and delivery history proofs',
    reports: 'Generated self-contained HTML daily briefing files',
    runs: 'Historical run execution logs and manifest records',
  }

  async function loadData() {
    loading = true
    error = null
    try {
      data = await getCache()
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
    }
  }

  function confirmClean(kind: string) {
    if (kind === 'all') {
      modalTitle = 'Clean All Caches'
      modalMessage =
        'This will purge ALL cached data, downloaded PDFs, embeddings, runs, and generated reports! This action cannot be reversed.'
    } else if (kind === 'runs' || kind === 'reports') {
      modalTitle = `Clean ${kind.toUpperCase()} Cache`
      modalMessage = `Purging '${kind}' will permanently remove history displayed on your Dashboard and links to HTML reports. Proceed with caution!`
    } else {
      modalTitle = `Clean ${kind} cache`
      modalMessage = `Delete all cached files for '${kind}'? Next run will re-fetch or re-compute this data.`
    }
    pendingKind = kind
    modalOpen = true
  }

  async function handleConfirm() {
    if (pendingKind) {
      actionLoading = true
      error = null
      successMsg = null
      try {
        const res = await cleanCache(pendingKind)
        successMsg = `Cleaned ${pendingKind} cache: freed ${formatBytes(res.freed_bytes)}.`
        await loadData()
      } catch (e: unknown) {
        error = e instanceof Error ? e.message : String(e)
      } finally {
        actionLoading = false
      }
    }
    modalOpen = false
    pendingKind = null
  }

  function handleCancel() {
    if (actionLoading) return
    modalOpen = false
    pendingKind = null
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B'
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
  }

  let totalSizeBytes = $derived(
    data?.caches.reduce((sum, c) => sum + c.size_bytes, 0) ?? 0,
  )

  let totalFiles = $derived(
    data?.caches.reduce((sum, c) => sum + c.file_count, 0) ?? 0,
  )

  $effect(() => {
    loadData()
  })
</script>

<div class="cache-page">
  <div class="page-header">
    <div class="header-left">
      <h2>Storage & Cache Management</h2>
      <p class="header-desc">
        Inspect disk usage of intermediate stages and clear stale cache directories.
      </p>
    </div>

    <div class="header-actions">
      <button class="secondary sm" onclick={loadData} disabled={loading || actionLoading}>
        <Icon name="refresh" size={14} />
        <span>Refresh</span>
      </button>

      {#if data}
        <button
          class="danger sm"
          onclick={() => confirmClean('all')}
          disabled={loading || actionLoading || totalFiles === 0}
        >
          <Icon name="trash" size={13} />
          <span>Purge All Cache</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Overall usage metrics -->
  <div class="usage-summary-card">
    <div class="summary-item">
      <div class="summary-label">Total Disk Usage</div>
      <div class="summary-val">{formatBytes(totalSizeBytes)}</div>
    </div>
    <div class="summary-item">
      <div class="summary-label">Total Cached Files</div>
      <div class="summary-val font-mono">{totalFiles}</div>
    </div>
    <div class="summary-item">
      <div class="summary-label">Cache Categories</div>
      <div class="summary-val font-mono">{data?.caches.length ?? 0}</div>
    </div>
  </div>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if successMsg}
    <div class="success-banner">
      <Icon name="check" size={16} />
      <span>{successMsg}</span>
      <button class="dismiss-btn" onclick={() => (successMsg = null)}>
        <Icon name="x" size={14} />
      </button>
    </div>
  {/if}

  {#if loading}
    <div class="loading-state">
      <Spinner size="large" />
      <p>Calculating cache sizes...</p>
    </div>
  {:else if data}
    <div class="cache-cards-grid">
      {#each data.caches as c}
        {@const isCritical = c.kind === 'runs' || c.kind === 'reports'}
        {@const isEmpty = c.file_count === 0}
        <div class="cache-card" class:is-empty={isEmpty}>
          <div class="cache-card-header">
            <div class="kind-wrap">
              <span class="kind-name">{c.kind}</span>
              {#if isCritical}
                <span class="critical-pill" title="Contains execution history">Critical</span>
              {/if}
            </div>

            <button
              class="danger sm clean-btn"
              disabled={isEmpty || actionLoading}
              onclick={() => confirmClean(c.kind)}
            >
              <Icon name="trash" size={12} />
              <span>Clean</span>
            </button>
          </div>

          <p class="kind-desc">
            {KIND_DESCRIPTIONS[c.kind] || 'Cached artifacts for ' + c.kind}
          </p>

          <div class="cache-stats-row">
            <div class="stat-col">
              <span class="stat-lbl">Size</span>
              <span class="stat-val font-mono">{c.size_display}</span>
            </div>
            <div class="stat-col">
              <span class="stat-lbl">Files</span>
              <span class="stat-val font-mono">{c.file_count}</span>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<Modal
  open={modalOpen}
  title={modalTitle}
  confirmText="Purge"
  danger={true}
  loading={actionLoading}
  onConfirm={handleConfirm}
  onCancel={handleCancel}
>
  <p>{modalMessage}</p>
</Modal>

<style>
  .cache-page {
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
    gap: 4px;
  }

  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .header-desc {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* Usage summary */
  .usage-summary-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 18px 24px;
    box-shadow: var(--shadow-sm);
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .summary-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .summary-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-tertiary);
  }

  .summary-val {
    font-size: 24px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .font-mono {
    font-family: var(--font-mono);
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

  /* Cache Cards Grid */
  .cache-cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px;
  }

  .cache-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 18px 20px;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .cache-card:hover {
    box-shadow: var(--shadow-md);
  }

  .cache-card.is-empty {
    opacity: 0.7;
  }

  .cache-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .kind-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .kind-name {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .critical-pill {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: var(--radius-full);
    background: var(--warning-light);
    color: var(--warning-text);
  }

  .clean-btn {
    gap: 4px;
    padding: 4px 8px;
    font-size: 11px;
  }

  .kind-desc {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.4;
    min-height: 34px;
  }

  .cache-stats-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }

  .stat-col {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-lbl {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-tertiary);
  }

  .stat-val {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }
</style>
