<script lang="ts">
  import { connect, disconnect, clearLines, getLogState } from '../stores/logs.svelte'
  import Icon from '../components/Icon.svelte'

  let logState = getLogState()
  let paused = $state(false)
  let filterLevel = $state<'ALL' | 'INFO' | 'WARN' | 'ERROR' | 'DEBUG'>('ALL')
  let searchQuery = $state('')
  let autoScroll = $state(true)
  let container = $state<HTMLDivElement | null>(null)

  $effect(() => {
    connect()
    return () => disconnect()
  })

  // Format log lines with color detection
  interface FormattedLine {
    raw: string
    timestamp?: string
    level?: string
    message: string
  }

  function parseLine(line: string): FormattedLine {
    // Check for JSON or standard tracing fmt format
    // Example: 2026-06-06T12:00:00.123456Z  INFO daily_paper::commands::serve: starting web server
    const match = line.match(/^([0-9-T:.Z]+)?\s*(INFO|WARN|ERROR|DEBUG|TRACE)?\s*(.*)$/)
    if (match && (match[2] || match[1])) {
      return {
        raw: line,
        timestamp: match[1],
        level: match[2] || 'INFO',
        message: match[3] || line,
      }
    }
    // Fallback: search for keywords
    let level = 'INFO'
    if (line.includes('ERROR') || line.includes('error:')) level = 'ERROR'
    else if (line.includes('WARN') || line.includes('warn:')) level = 'WARN'
    else if (line.includes('DEBUG')) level = 'DEBUG'

    return { raw: line, level, message: line }
  }

  let filteredLines = $derived.by(() => {
    return logState.lines
      .map(parseLine)
      .filter((item) => {
        if (filterLevel !== 'ALL' && item.level !== filterLevel) {
          return false
        }
        if (searchQuery.trim()) {
          const q = searchQuery.toLowerCase()
          return item.raw.toLowerCase().includes(q)
        }
        return true
      })
  })

  // Auto-scroll on new lines
  $effect(() => {
    const _ = logState.lines.length
    if (!paused && autoScroll && container) {
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

  function copyAll() {
    navigator.clipboard.writeText(logState.lines.join('\n'))
  }
</script>

<div class="logs-page">
  <div class="page-header">
    <div class="header-left">
      <div class="title-with-status">
        <h2>Live Server Logs</h2>
        <span class="status-indicator" class:connected={logState.connected}>
          <span class="status-dot"></span>
          <span>{logState.connected ? 'Stream Connected' : 'Disconnected'}</span>
        </span>
      </div>
      <p class="header-desc">
        Real-time tracing events, pipeline stage transitions, and backend HTTP server output.
      </p>
    </div>

    <div class="header-actions">
      <button class="secondary sm" onclick={copyAll} title="Copy logs to clipboard">
        <Icon name="copy" size={13} />
        <span>Copy All</span>
      </button>

      <button class="secondary sm" onclick={handleClear} title="Clear log window">
        <Icon name="trash" size={13} />
        <span>Clear</span>
      </button>

      <button
        class={paused ? 'primary sm' : 'secondary sm'}
        onclick={togglePause}
        title="Pause stream updates"
      >
        <Icon name={paused ? 'play' : 'refresh'} size={13} />
        <span>{paused ? 'Resume Stream' : 'Pause Stream'}</span>
      </button>
    </div>
  </div>

  <!-- Filters & Search Bar -->
  <div class="log-control-bar">
    <div class="level-filters">
      {#each ['ALL', 'INFO', 'WARN', 'ERROR', 'DEBUG'] as lvl}
        <button
          class="level-tab"
          class:active={filterLevel === lvl}
          onclick={() => (filterLevel = lvl as typeof filterLevel)}
        >
          {lvl}
        </button>
      {/each}
    </div>

    <div class="search-input-wrap">
      <Icon name="filter" size={13} />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="Filter logs by keyword..."
        class="search-input"
      />
      {#if searchQuery}
        <button class="clear-search-btn" onclick={() => (searchQuery = '')}>
          <Icon name="x" size={12} />
        </button>
      {/if}
    </div>

    <label class="autoscroll-toggle">
      <input type="checkbox" bind:checked={autoScroll} />
      <span>Auto-scroll</span>
    </label>

    <div class="lines-count font-mono">
      {filteredLines.length} / {logState.lines.length} lines
    </div>
  </div>

  <!-- Terminal Window -->
  <div class="terminal-card">
    <div class="terminal-bar">
      <div class="mac-buttons">
        <span class="mac-btn red"></span>
        <span class="mac-btn yellow"></span>
        <span class="mac-btn green"></span>
      </div>
      <span class="terminal-title">daily-paper-serve :: stdout/stderr</span>
    </div>

    <div class="terminal-body" bind:this={container}>
      {#if filteredLines.length === 0}
        <div class="empty-terminal">
          <p>
            {#if logState.lines.length === 0}
              Waiting for live logs from server stream...
            {:else}
              No log lines match current filter ({filterLevel}, "{searchQuery}").
            {/if}
          </p>
        </div>
      {:else}
        {#each filteredLines as line, i (i)}
          <div class="log-entry">
            {#if line.timestamp}
              <span class="log-time">{line.timestamp.slice(11, 23)}</span>
            {/if}
            {#if line.level}
              <span class="log-level-badge level-{line.level.toLowerCase()}">
                {line.level}
              </span>
            {/if}
            <span class="log-msg level-text-{line.level?.toLowerCase() || 'info'}">
              {line.message}
            </span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .logs-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: calc(100vh - 120px);
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

  .title-with-status {
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

  .status-indicator {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: var(--radius-full);
    background: var(--danger-light);
    color: var(--danger-text);
  }

  .status-indicator.connected {
    background: var(--success-light);
    color: var(--success-text);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
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

  /* Control bar */
  .log-control-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 8px 14px;
    box-shadow: var(--shadow-xs);
  }

  .level-filters {
    display: flex;
    gap: 2px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 2px;
  }

  .level-tab {
    background: transparent;
    border: none;
    box-shadow: none;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .level-tab.active {
    background: var(--bg-surface);
    color: var(--primary);
    box-shadow: var(--shadow-xs);
  }

  .search-input-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 200px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 0 10px;
    color: var(--text-tertiary);
  }

  .search-input {
    border: none;
    background: transparent;
    padding: 6px 0;
    box-shadow: none;
    font-size: 12px;
    width: 100%;
  }

  .search-input:focus {
    box-shadow: none;
    border: none;
    outline: none;
  }

  .clear-search-btn {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 2px;
    cursor: pointer;
    color: var(--text-tertiary);
  }

  .autoscroll-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .lines-count {
    font-size: 11px;
    color: var(--text-tertiary);
  }

  .font-mono {
    font-family: var(--font-mono);
  }

  /* Terminal Window */
  .terminal-card {
    flex: 1;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: var(--radius-xl);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-lg);
  }

  .terminal-bar {
    display: flex;
    align-items: center;
    padding: 10px 16px;
    background: #161b22;
    border-bottom: 1px solid #30363d;
    position: relative;
  }

  .mac-buttons {
    display: flex;
    gap: 6px;
  }

  .mac-btn {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .mac-btn.red {
    background: #ff5f56;
  }
  .mac-btn.yellow {
    background: #ffbd2e;
  }
  .mac-btn.green {
    background: #27c93f;
  }

  .terminal-title {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    font-family: var(--font-mono);
    font-size: 11px;
    color: #8b949e;
  }

  .terminal-body {
    flex: 1;
    overflow-y: auto;
    padding: 14px 18px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.6;
    color: #c9d1d9;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .empty-terminal {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #484f58;
    font-style: italic;
  }

  .log-entry {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    word-break: break-all;
    white-space: pre-wrap;
  }

  .log-time {
    color: #484f58;
    flex-shrink: 0;
    font-size: 11px;
  }

  .log-level-badge {
    padding: 0 5px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }

  .level-info {
    background: rgba(56, 189, 248, 0.15);
    color: #38bdf8;
  }

  .level-warn {
    background: rgba(245, 158, 11, 0.15);
    color: #fbbf24;
  }

  .level-error {
    background: rgba(239, 68, 68, 0.2);
    color: #f87171;
  }

  .level-debug {
    background: rgba(148, 163, 184, 0.15);
    color: #94a3b8;
  }

  .log-msg {
    flex: 1;
  }

  .level-text-error {
    color: #fca5a5;
  }

  .level-text-warn {
    color: #fde68a;
  }

  .level-text-info {
    color: #e6edf3;
  }

  .level-text-debug {
    color: #8b949e;
  }
</style>
