<script lang="ts">
  import { getConfig, saveConfig, reloadConfig } from '../api'
  import type { ConfigResponse } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'

  let data = $state<ConfigResponse | null>(null)
  let content = $state('')
  let loading = $state(true)
  let saving = $state(false)
  let reloading = $state(false)
  let error = $state<string | null>(null)
  let successMsg = $state<string | null>(null)
  let dirty = $state(false)

  async function loadConfig() {
    loading = true
    error = null
    try {
      data = await getConfig()
      content = data.content
      dirty = false
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
    }
  }

  async function handleSave() {
    saving = true
    error = null
    successMsg = null
    try {
      await saveConfig(content)
      dirty = false
      successMsg = 'Config saved. Reload to apply changes.'
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
  }

  async function handleReload() {
    reloading = true
    error = null
    successMsg = null
    try {
      await reloadConfig()
      successMsg = 'Config reloaded successfully.'
      await loadConfig()
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      reloading = false
    }
  }

  function handleInput() {
    dirty = true
    successMsg = null
  }

  $effect(() => {
    loadConfig()
  })
</script>

<div class="config-page">
  <h2>Config</h2>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if successMsg}
    <div class="success-banner">
      {successMsg}
      <button onclick={handleReload} disabled={reloading}>
        {reloading ? 'Reloading...' : 'Reload Now'}
      </button>
      <button class="dismiss" onclick={() => (successMsg = null)}>&times;</button>
    </div>
  {/if}

  {#if loading}
    <div class="loading"><Spinner /></div>
  {:else if data}
    <div class="path-info">
      <strong>File:</strong> <code>{data.path}</code>
    </div>

    <textarea
      class="editor"
      value={content}
      oninput={(e) => { content = (e.target as HTMLTextAreaElement).value; handleInput() }}
      spellcheck="false"
    ></textarea>

    <div class="actions">
      <button class="primary" onclick={handleSave} disabled={saving || !dirty}>
        {saving ? 'Saving...' : 'Save'}
      </button>
      <button onclick={handleReload} disabled={reloading}>
        {reloading ? 'Reloading...' : 'Reload'}
      </button>
    </div>
  {/if}
</div>

<style>
  .config-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
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

  .path-info {
    font-size: 13px;
    color: var(--text-secondary);
  }

  .success-banner {
    background: #dcfce7;
    color: #166534;
    padding: 10px 16px;
    border-radius: 6px;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .success-banner .dismiss {
    margin-left: auto;
    background: none;
    border: none;
    color: inherit;
    font-size: 18px;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0.7;
  }
  .success-banner .dismiss:hover {
    opacity: 1;
    background: none;
  }

  .editor {
    width: 100%;
    min-height: 480px;
    font-family: var(--mono);
    font-size: 13px;
    line-height: 1.6;
    tab-size: 2;
    padding: 12px;
  }

  .actions {
    display: flex;
    gap: 8px;
  }
</style>
