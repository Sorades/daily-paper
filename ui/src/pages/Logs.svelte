<script lang="ts">
  import { connect, disconnect, clearLines, getLogState } from '../stores/logs.svelte'

  let logState = getLogState()
  let paused = $state(false)
  let container = $state<HTMLDivElement | null>(null)

  $effect(() => {
    connect()
    return () => disconnect()
  })

  // Auto-scroll on new lines
  $effect(() => {
    // Subscribe to lines length
    const _ = logState.lines.length
    if (!paused && container) {
      requestAnimationFrame(() => {
        if (container) {
          container.scrollTop = container.scrollHeight
        }
      })
    }
  })

  function togglePause() {
    paused = !paused
  }

  function handleClear() {
    clearLines()
  }
</script>

<div class="logs-page">
  <div class="header">
    <h2>Logs</h2>
    <span class="status" class:connected={logState.connected}>
      {logState.connected ? 'Connected' : 'Disconnected'}
    </span>
    <span class="count">{logState.lines.length} lines</span>
    <button onclick={handleClear}>Clear</button>
    <button onclick={togglePause}>
      {paused ? 'Resume' : 'Pause'}
    </button>
  </div>

  <div class="log-viewer" bind:this={container}>
    {#each logState.lines as line, i (i)}
      <div class="log-line">{line}</div>
    {/each}
    {#if logState.lines.length === 0}
      <div class="empty">Waiting for logs...</div>
    {/if}
  </div>
</div>

<style>
  .logs-page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: calc(100vh - 120px);
  }

  .header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .status {
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 10px;
    background: #fee2e2;
    color: #991b1b;
  }

  .status.connected {
    background: #dcfce7;
    color: #166534;
  }

  .count {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .log-viewer {
    flex: 1;
    overflow-y: auto;
    background: #1a1b26;
    color: #a9b1d6;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.6;
    padding: 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
  }

  .log-line {
    white-space: pre-wrap;
    word-break: break-all;
  }

  .empty {
    color: #565f89;
    font-style: italic;
  }
</style>
