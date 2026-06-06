<script lang="ts">
  import { getRoute, navigate } from '../router.svelte'

  const links = [
    { hash: '#/', label: 'Dashboard', page: 'dashboard' },
    { hash: '#/run/new', label: 'New Run', page: 'run-new' },
    { hash: '#/config', label: 'Config', page: 'config' },
    { hash: '#/cache', label: 'Cache', page: 'cache' },
    { hash: '#/logs', label: 'Logs', page: 'logs' },
  ]

  let route = $derived(getRoute())

  function isActive(link: { page: string }): boolean {
    if (link.page === 'run-new') return route.page === 'run' && route.params.id === 'new'
    return route.page === link.page
  }
</script>

<nav>
  <div class="nav-inner">
    <a href="#/" class="logo" onclick={(e) => { e.preventDefault(); navigate('#/') }}>
      Daily Paper
    </a>
    <div class="nav-links">
      {#each links as link}
        <a
          href={link.hash}
          class:active={isActive(link)}
          onclick={(e) => { e.preventDefault(); navigate(link.hash) }}
        >
          {link.label}
        </a>
      {/each}
    </div>
  </div>
</nav>

<style>
  nav {
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    z-index: 100;
  }

  .nav-inner {
    max-width: 1200px;
    margin: 0 auto;
    padding: 0 20px;
    display: flex;
    align-items: center;
    height: 48px;
    gap: 24px;
  }

  .logo {
    font-weight: 600;
    font-size: 16px;
    color: var(--text);
    text-decoration: none;
    white-space: nowrap;
  }
  .logo:hover {
    text-decoration: none;
    color: var(--accent);
  }

  .nav-links {
    display: flex;
    gap: 4px;
  }

  .nav-links a {
    color: var(--text-secondary);
    padding: 6px 12px;
    border-radius: 4px;
    font-size: 13px;
    transition: color 0.15s, background 0.15s;
  }
  .nav-links a:hover {
    color: var(--text);
    background: var(--bg-tertiary);
    text-decoration: none;
  }
  .nav-links a.active {
    color: var(--accent);
    background: var(--bg);
    font-weight: 500;
  }
</style>
