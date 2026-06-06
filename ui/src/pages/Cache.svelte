<script lang="ts">
  import { getCache, cleanCache } from '../api'
  import type { CacheResponse, CacheInfo } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import Modal from '../components/Modal.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'

  let data = $state<CacheResponse | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)

  // Modal state
  let modalOpen = $state(false)
  let modalTitle = $state('')
  let modalMessage = $state('')
  let pendingKind = $state<string | null>(null)

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
      modalMessage = 'This will delete ALL cached data including runs and reports. This cannot be undone.'
    } else {
      modalTitle = `Clean ${kind}`
      modalMessage = `Delete all cached ${kind} data? This cannot be undone.`
    }
    pendingKind = kind
    modalOpen = true
  }

  async function handleConfirm() {
    if (pendingKind) {
      try {
        await cleanCache(pendingKind)
        await loadData()
      } catch (e: unknown) {
        error = e instanceof Error ? e.message : String(e)
      }
    }
    modalOpen = false
    pendingKind = null
  }

  function handleCancel() {
    modalOpen = false
    pendingKind = null
  }

  function totalSize(caches: CacheInfo[]): string {
    const bytes = caches.reduce((sum, c) => sum + c.size_bytes, 0)
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  }

  $effect(() => {
    loadData()
  })
</script>

<div class="cache-page">
  <div class="header">
    <h2>Cache</h2>
    {#if data}
      <span class="total">Total: {totalSize(data.caches)}</span>
      <button class="danger" onclick={() => confirmClean('all')}>Clean All</button>
    {/if}
  </div>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if loading}
    <div class="loading"><Spinner /></div>
  {:else if data}
    <table>
      <thead>
        <tr>
          <th>Kind</th>
          <th>Size</th>
          <th>Files</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each data.caches as c}
          <tr>
            <td class="kind">{c.kind}</td>
            <td>{c.size_display}</td>
            <td>{c.file_count}</td>
            <td class="action">
              <button
                class="danger"
                disabled={c.file_count === 0}
                onclick={() => confirmClean(c.kind)}
              >
                Clean
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<Modal
  open={modalOpen}
  title={modalTitle}
  confirmText="Delete"
  danger={true}
  onConfirm={handleConfirm}
  onCancel={handleCancel}
>
  <p>{modalMessage}</p>
</Modal>

<style>
  .cache-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .total {
    margin-left: auto;
    font-size: 13px;
    color: var(--text-secondary);
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: 48px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th, td {
    text-align: left;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }

  th {
    font-weight: 500;
    color: var(--text-secondary);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .kind {
    font-family: var(--mono);
  }

  .action {
    text-align: right;
  }
</style>
