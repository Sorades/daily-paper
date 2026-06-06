<script lang="ts">
  import { getStats, getStatus } from '../api'
  import { navigate } from '../router.svelte'
  import type { StatsDay, StatusResponse } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'

  const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

  let year = $state(new Date().getFullYear())
  let month = $state(new Date().getMonth() + 1)
  let days = $state<StatsDay[]>([])
  let loading = $state(true)
  let error = $state<string | null>(null)
  let status = $state<StatusResponse | null>(null)

  let monthLabel = $derived(`${year}-${String(month).padStart(2, '0')}`)

  // Calendar grid: compute first day of month and padding
  let firstDayOfWeek = $derived(new Date(year, month - 1, 1).getDay())
  // Convert Sunday=0 to Monday-based (Mon=0, Sun=6)
  let startOffset = $derived(firstDayOfWeek === 0 ? 6 : firstDayOfWeek - 1)
  let daysInMonth = $derived(new Date(year, month, 0).getDate())

  // 6 rows x 7 cols grid
  let grid = $derived.by(() => {
    const cells: (number | null)[] = []
    for (let i = 0; i < startOffset; i++) cells.push(null)
    for (let d = 1; d <= daysInMonth; d++) cells.push(d)
    while (cells.length % 7 !== 0) cells.push(null)
    return cells
  })

  function statsFor(day: number): StatsDay | undefined {
    return days.find((d) => d.day === day)
  }

  async function loadData() {
    loading = true
    error = null
    try {
      const [stats, st] = await Promise.all([
        getStats(year, month),
        getStatus().catch(() => null),
      ])
      days = stats.days
      status = st
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
    }
  }

  function prevMonth() {
    if (month === 1) {
      year--
      month = 12
    } else {
      month--
    }
  }

  function nextMonth() {
    if (month === 12) {
      year++
      month = 1
    } else {
      month++
    }
  }

  function goToday() {
    const now = new Date()
    year = now.getFullYear()
    month = now.getMonth() + 1
  }

  function dateStr(day: number): string {
    return `${year}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`
  }

  $effect(() => {
    // React to year/month changes
    void year
    void month
    loadData()
  })
</script>

<div class="dashboard">
  <div class="header">
    <div class="nav-row">
      <button onclick={prevMonth}>&larr;</button>
      <h2>{monthLabel}</h2>
      <button onclick={nextMonth}>&rarr;</button>
      <button onclick={goToday}>Today</button>
    </div>
    <button class="primary" onclick={() => navigate('#/run/new')}>New Run</button>
  </div>

  {#if status?.pipeline_running}
    <div class="running-banner">
      Pipeline is running&hellip;
    </div>
  {/if}

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if loading}
    <div class="loading"><Spinner /></div>
  {:else}
    <div class="calendar">
      {#each WEEKDAYS as wd}
        <div class="weekday">{wd}</div>
      {/each}
      {#each grid as day, i}
        {#if day === null}
          <div class="day-cell empty"></div>
        {:else}
          {@const s = statsFor(day)}
          <button
            class="day-cell"
            class:has-runs={s && s.total > 0}
            onclick={() => navigate(`#/date/${dateStr(day)}`)}
          >
            <span class="day-num">{day}</span>
            {#if s && s.total > 0}
              <div class="dots">
                {#if s.success > 0}
                  <span class="dot success" title="{s.success} succeeded"></span>
                {/if}
                {#if s.failed > 0}
                  <span class="dot failed" title="{s.failed} failed"></span>
                {/if}
              </div>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .nav-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .nav-row h2 {
    min-width: 120px;
    text-align: center;
    font-size: 18px;
    margin: 0;
  }

  .running-banner {
    background: #dbeafe;
    color: #1e40af;
    padding: 10px 16px;
    border-radius: 6px;
    font-size: 13px;
  }

  .loading {
    display: flex;
    justify-content: center;
    padding: 48px;
  }

  .calendar {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 1px;
    background: var(--border);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }

  .weekday {
    background: var(--bg-secondary);
    padding: 8px;
    text-align: center;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .day-cell {
    background: var(--bg);
    padding: 8px;
    min-height: 72px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    border: none;
    border-radius: 0;
    font-size: 13px;
    transition: background 0.15s;
  }

  .day-cell.empty {
    background: var(--bg-secondary);
    cursor: default;
  }
  .day-cell.empty:hover {
    background: var(--bg-secondary);
  }

  .day-cell.has-runs {
    cursor: pointer;
  }
  .day-cell.has-runs:hover {
    background: var(--bg-secondary);
  }

  .day-num {
    font-weight: 500;
  }

  .dots {
    display: flex;
    gap: 4px;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .dot.success {
    background: var(--success);
  }

  .dot.failed {
    background: var(--danger);
  }
</style>
