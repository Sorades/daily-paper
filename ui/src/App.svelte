<script lang="ts">
  import { getRoute } from './router.svelte'
  import Nav from './components/Nav.svelte'
  import Dashboard from './pages/Dashboard.svelte'
  import DateDetail from './pages/DateDetail.svelte'
  import RunDetail from './pages/RunDetail.svelte'
  import RunLauncher from './pages/RunLauncher.svelte'
  import Config from './pages/Config.svelte'
  import Cache from './pages/Cache.svelte'
  import Logs from './pages/Logs.svelte'

  let route = $derived(getRoute())
</script>

<Nav />

<main>
  {#if route.page === 'dashboard'}
    <Dashboard />
  {:else if route.page === 'date'}
    <DateDetail date={route.params.date} />
  {:else if route.page === 'run' && route.params.id === 'new'}
    <RunLauncher />
  {:else if route.page === 'run'}
    <RunDetail runId={route.params.id} />
  {:else if route.page === 'config'}
    <Config />
  {:else if route.page === 'cache'}
    <Cache />
  {:else if route.page === 'logs'}
    <Logs />
  {/if}
</main>

<style>
  main {
    max-width: 1200px;
    margin: 0 auto;
    padding: 24px 20px;
    flex: 1;
    width: 100%;
  }
</style>
