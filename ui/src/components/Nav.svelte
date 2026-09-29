<script lang="ts">
  import { getRoute, navigate } from '../router.svelte'
  import { getTheme, toggleTheme } from '../stores/theme.svelte'
  import { getStatus } from '../api'
  import Icon from './Icon.svelte'
  import Spinner from './Spinner.svelte'

  const links = [
    { hash: '#/', label: 'Dashboard', page: 'dashboard', icon: 'calendar' as const },
    { hash: '#/config', label: 'Config', page: 'config', icon: 'settings' as const },
    { hash: '#/cache', label: 'Cache', page: 'cache', icon: 'database' as const },
    { hash: '#/logs', label: 'Logs', page: 'logs', icon: 'terminal' as const },
  ]

  let route = $derived(getRoute())
  let currentTheme = $derived(getTheme())
  let pipelineRunning = $state(false)
  let statusPollTimer: ReturnType<typeof setInterval> | null = null

  function isActive(link: { page: string }): boolean {
    if (link.page === 'run-new') return route.page === 'run' && route.params.id === 'new'
    return route.page === link.page
  }

  async function checkStatus() {
    try {
      const res = await getStatus()
      pipelineRunning = res.pipeline_running
    } catch {
      // ignore
    }
  }

  $effect(() => {
    checkStatus()
    statusPollTimer = setInterval(checkStatus, 3000)
    return () => {
      if (statusPollTimer) clearInterval(statusPollTimer)
    }
  })
</script>

<nav>
  <div class="nav-inner">
    <div class="left-group">
      <a href="#/" class="brand" onclick={(e) => { e.preventDefault(); navigate('#/') }}>
        <div class="brand-icon">
          <Icon name="paper" size={18} />
        </div>
        <div class="brand-text">
          <span class="brand-title">Daily Paper</span>
          <span class="brand-subtitle">arXiv Digest</span>
        </div>
      </a>

      <div class="nav-links">
        {#each links as link}
          <a
            href={link.hash}
            class="nav-tab"
            class:active={isActive(link)}
            onclick={(e) => { e.preventDefault(); navigate(link.hash) }}
          >
            <Icon name={link.icon} size={15} />
            <span>{link.label}</span>
          </a>
        {/each}
      </div>
    </div>

    <div class="right-group">
      {#if pipelineRunning}
        <button
          class="running-pill"
          onclick={() => navigate(route.page === 'run' ? `#/run/${route.params.id}` : '#/logs')}
          title="Pipeline is actively processing tasks"
        >
          <span class="pulse-indicator"></span>
          <span>Pipeline Running</span>
        </button>
      {/if}

      <button
        class="primary sm run-btn"
        onclick={() => navigate('#/run/new')}
      >
        <Icon name="play" size={13} />
        <span>New Run</span>
      </button>

      <button
        class="ghost theme-toggle"
        onclick={toggleTheme}
        aria-label="Toggle Dark Mode"
        title="Toggle dark/light mode"
      >
        <Icon name={currentTheme === 'dark' ? 'sun' : 'moon'} size={17} />
      </button>
    </div>
  </div>
</nav>

<style>
  nav {
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    z-index: 100;
    backdrop-filter: blur(8px);
    transition: background-color var(--transition-fast), border-color var(--transition-fast);
  }

  .nav-inner {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 60px;
    gap: 20px;
  }

  .left-group {
    display: flex;
    align-items: center;
    gap: 32px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    text-decoration: none;
    color: inherit;
  }

  .brand-icon {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-lg);
    background: linear-gradient(135deg, var(--primary) 0%, #818cf8 100%);
    color: #ffffff;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 8px rgba(79, 70, 229, 0.35);
  }

  .brand-text {
    display: flex;
    flex-direction: column;
  }

  .brand-title {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .brand-subtitle {
    font-size: 11px;
    color: var(--text-tertiary);
    font-weight: 500;
    line-height: 1;
  }

  .nav-links {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-secondary);
    padding: 3px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-subtle);
  }

  .nav-tab {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-secondary);
    padding: 6px 14px;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
    transition: all var(--transition-fast);
  }

  .nav-tab:hover {
    color: var(--text-primary);
    background: rgba(0, 0, 0, 0.03);
  }

  :root[data-theme='dark'] .nav-tab:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .nav-tab.active {
    color: var(--primary);
    background: var(--bg-surface);
    box-shadow: var(--shadow-xs);
    font-weight: 600;
  }

  .right-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .running-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--primary-light);
    color: var(--primary-text);
    border: 1px solid var(--primary-border);
    padding: 6px 12px;
    border-radius: var(--radius-full);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .pulse-indicator {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--primary);
    box-shadow: 0 0 0 0 rgba(99, 102, 241, 0.7);
    animation: ringPulse 1.8s infinite cubic-bezier(0.66, 0, 0, 1);
  }

  @keyframes ringPulse {
    to {
      box-shadow: 0 0 0 8px rgba(99, 102, 241, 0);
    }
  }

  .run-btn {
    padding: 7px 14px;
    gap: 7px;
  }

  .theme-toggle {
    width: 36px;
    height: 36px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
  }

  .theme-toggle:hover {
    color: var(--text-primary);
  }

  @media (max-width: 768px) {
    .nav-inner {
      padding: 0 16px;
    }
    .brand-subtitle {
      display: none;
    }
    .nav-tab span {
      display: none;
    }
    .nav-tab {
      padding: 6px 10px;
    }
  }
</style>
