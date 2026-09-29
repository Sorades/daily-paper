<script lang="ts">
  import { getStats, getStatus } from '../api'
  import { navigate } from '../router.svelte'
  import type { StatsDay, StatusResponse } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Icon from '../components/Icon.svelte'

  const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

  let year = $state(new Date().getFullYear())
  let month = $state(new Date().getMonth() + 1)
  let days = $state<StatsDay[]>([])
  let loading = $state(true)
  let error = $state<string | null>(null)
  let status = $state<StatusResponse | null>(null)
  let viewMode = $state<'calendar' | 'list'>('calendar')

  let monthLabel = $derived(`${year} 年 ${String(month).padStart(2, '0')} 月`)
  let isCurrentMonth = $derived.by(() => {
    const now = new Date()
    return year === now.getFullYear() && month === now.getMonth() + 1
  })

  // Summary Metrics
  let totalRuns = $derived(days.reduce((acc, d) => acc + d.total, 0))
  let totalSuccess = $derived(days.reduce((acc, d) => acc + d.success, 0))
  let totalFailed = $derived(days.reduce((acc, d) => acc + d.failed, 0))
  let successRate = $derived(totalRuns > 0 ? Math.round((totalSuccess / totalRuns) * 100) : 100)

  // Calendar grid
  let firstDayOfWeek = $derived(new Date(year, month - 1, 1).getDay())
  let startOffset = $derived(firstDayOfWeek === 0 ? 6 : firstDayOfWeek - 1)
  let daysInMonth = $derived(new Date(year, month, 0).getDate())

  let grid = $derived.by(() => {
    const cells: (number | null)[] = []
    for (let i = 0; i < startOffset; i++) cells.push(null)
    for (let d = 1; d <= daysInMonth; d++) cells.push(d)
    while (cells.length % 7 !== 0) cells.push(null)
    return cells
  })

  // Days with runs sorted descending for list view
  let activeDays = $derived(
    days
      .filter((d) => d.total > 0)
      .sort((a, b) => b.day - a.day),
  )

  function statsFor(day: number): StatsDay | undefined {
    return days.find((d) => d.day === day)
  }

  function isToday(day: number): boolean {
    const now = new Date()
    return isCurrentMonth && day === now.getDate()
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
    void year
    void month
    loadData()
  })
</script>

<div class="dashboard-page">
  <!-- Top bar & Overview stats cards -->
  <div class="overview-grid">
    <div class="stat-card">
      <div class="stat-header">
        <span class="stat-title">Total Runs</span>
        <div class="stat-icon-wrap primary">
          <Icon name="paper" size={16} />
        </div>
      </div>
      <div class="stat-value">{totalRuns}</div>
      <div class="stat-desc">Monthly pipeline executions</div>
    </div>

    <div class="stat-card">
      <div class="stat-header">
        <span class="stat-title">Successful</span>
        <div class="stat-icon-wrap success">
          <Icon name="check" size={16} />
        </div>
      </div>
      <div class="stat-value">{totalSuccess}</div>
      <div class="stat-desc">Completed with full reports</div>
    </div>

    <div class="stat-card">
      <div class="stat-header">
        <span class="stat-title">Failures</span>
        <div class="stat-icon-wrap danger">
          <Icon name="alert" size={16} />
        </div>
      </div>
      <div class="stat-value">{totalFailed}</div>
      <div class="stat-desc">Blocked or failed runs</div>
    </div>

    <div class="stat-card">
      <div class="stat-header">
        <span class="stat-title">Success Rate</span>
        <div class="stat-icon-wrap info">
          <Icon name="zap" size={16} />
        </div>
      </div>
      <div class="stat-value">{totalRuns > 0 ? `${successRate}%` : '—'}</div>
      <div class="stat-desc">Reliability benchmark</div>
    </div>
  </div>

  <!-- Main Calendar / Activity Container -->
  <div class="calendar-card">
    <div class="card-header">
      <div class="month-nav">
        <div class="btn-group">
          <button class="ghost nav-arrow-btn" onclick={prevMonth} title="Previous Month">
            <Icon name="arrow-left" size={16} />
          </button>
          <button class="ghost nav-arrow-btn" onclick={nextMonth} title="Next Month">
            <Icon name="arrow-right" size={16} />
          </button>
        </div>
        <h2 class="month-title">{monthLabel}</h2>
        {#if !isCurrentMonth}
          <button class="sm secondary" onclick={goToday}>Back to Today</button>
        {/if}
      </div>

      <div class="header-controls">
        <div class="view-switch">
          <button
            class="switch-btn"
            class:active={viewMode === 'calendar'}
            onclick={() => (viewMode = 'calendar')}
            title="Calendar View"
          >
            <Icon name="calendar" size={15} />
            <span>Calendar</span>
          </button>
          <button
            class="switch-btn"
            class:active={viewMode === 'list'}
            onclick={() => (viewMode = 'list')}
            title="Monthly Runs List"
          >
            <Icon name="file-text" size={15} />
            <span>List ({activeDays.length})</span>
          </button>
        </div>

        <button class="secondary sm refresh-btn" onclick={loadData} title="Refresh Statistics">
          <Icon name="refresh" size={14} />
          <span>Refresh</span>
        </button>
      </div>
    </div>

    {#if error}
      <div class="banner-wrapper">
        <ErrorBanner message={error} onDismiss={() => (error = null)} />
      </div>
    {/if}

    {#if loading}
      <div class="loading-state">
        <Spinner size="large" />
        <p>Loading month statistics...</p>
      </div>
    {:else if viewMode === 'calendar'}
      <div class="calendar-wrapper">
        <div class="calendar-grid">
          {#each WEEKDAYS as wd}
            <div class="weekday-header">{wd}</div>
          {/each}

          {#each grid as day, i}
            {#if day === null}
              <div class="day-slot empty"></div>
            {:else}
              {@const s = statsFor(day)}
              {@const hasRuns = s && s.total > 0}
              {@const isTodayDate = isToday(day)}
              <div
                class="day-slot active-slot"
                class:has-runs={hasRuns}
                class:is-today={isTodayDate}
                tabindex="0"
                role="button"
                onclick={() => navigate(`#/date/${dateStr(day)}`)}
                onkeydown={(e) => { if (e.key === 'Enter') navigate(`#/date/${dateStr(day)}`) }}
              >
                <div class="day-top">
                  <span class="day-number" class:today-pill={isTodayDate}>
                    {day}
                  </span>
                  {#if hasRuns}
                    <span class="total-tag">{s.total} {s.total === 1 ? 'run' : 'runs'}</span>
                  {/if}
                </div>

                <div class="day-content">
                  {#if hasRuns}
                    <div class="run-pills-row">
                      {#if s.success > 0}
                        <div class="metric-pill success" title="{s.success} runs succeeded">
                          <span class="dot"></span>
                          <span>{s.success}</span>
                        </div>
                      {/if}
                      {#if s.failed > 0}
                        <div class="metric-pill failed" title="{s.failed} runs failed">
                          <span class="dot"></span>
                          <span>{s.failed}</span>
                        </div>
                      {/if}
                    </div>
                  {:else}
                    <div class="no-runs-hint"></div>
                  {/if}
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    {:else}
      <!-- List View for Current Month -->
      <div class="list-wrapper">
        {#if activeDays.length === 0}
          <div class="empty-list-state">
            <Icon name="calendar" size={36} />
            <p>No runs recorded in {monthLabel}</p>
            <button class="primary sm" onclick={() => navigate('#/run/new')}>
              Launch First Run
            </button>
          </div>
        {:else}
          <div class="date-card-list">
            {#each activeDays as item}
              {@const ds = dateStr(item.day)}
              <div
                class="active-date-card"
                role="button"
                tabindex="0"
                onclick={() => navigate(`#/date/${ds}`)}
                onkeydown={(e) => { if (e.key === 'Enter') navigate(`#/date/${ds}`) }}
              >
                <div class="date-card-left">
                  <div class="date-badge-box">
                    <span class="date-box-month">{month}月</span>
                    <span class="date-box-day">{item.day}</span>
                  </div>
                  <div class="date-card-info">
                    <span class="date-str">{ds}</span>
                    <span class="date-sub">Total {item.total} execution(s)</span>
                  </div>
                </div>

                <div class="date-card-right">
                  <div class="pills-group">
                    {#if item.success > 0}
                      <span class="badge-success-tag">{item.success} Succeeded</span>
                    {/if}
                    {#if item.failed > 0}
                      <span class="badge-failed-tag">{item.failed} Failed</span>
                    {/if}
                  </div>
                  <div class="arrow-icon-wrap">
                    <Icon name="chevron-right" size={18} />
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .dashboard-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* Stat cards */
  .overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
  }

  .stat-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 18px 20px;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .stat-card:hover {
    box-shadow: var(--shadow-md);
  }

  .stat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-secondary);
  }

  .stat-title {
    font-size: 13px;
    font-weight: 500;
  }

  .stat-icon-wrap {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .stat-icon-wrap.primary {
    background: var(--primary-light);
    color: var(--primary);
  }

  .stat-icon-wrap.success {
    background: var(--success-light);
    color: var(--success);
  }

  .stat-icon-wrap.danger {
    background: var(--danger-light);
    color: var(--danger);
  }

  .stat-icon-wrap.info {
    background: var(--info-light);
    color: var(--info);
  }

  .stat-value {
    font-size: 26px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
    line-height: 1.2;
    margin-top: 4px;
  }

  .stat-desc {
    font-size: 12px;
    color: var(--text-tertiary);
  }

  /* Calendar Card */
  .calendar-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 24px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
    gap: 16px;
  }

  .month-nav {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .btn-group {
    display: flex;
    align-items: center;
    gap: 2px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 2px;
  }

  .nav-arrow-btn {
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }

  .nav-arrow-btn:hover {
    color: var(--text-primary);
    background: var(--bg-surface);
  }

  .month-title {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
    min-width: 140px;
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .view-switch {
    display: flex;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 2px;
  }

  .switch-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    box-shadow: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .switch-btn.active {
    background: var(--bg-surface);
    color: var(--primary);
    box-shadow: var(--shadow-xs);
    font-weight: 600;
  }

  .refresh-btn {
    gap: 6px;
  }

  .banner-wrapper {
    padding: 16px 24px 0;
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

  /* Calendar Grid */
  .calendar-wrapper {
    padding: 16px 20px 24px;
  }

  .calendar-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 8px;
  }

  .weekday-header {
    text-align: center;
    padding: 10px 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-tertiary);
  }

  .day-slot {
    min-height: 96px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-subtle);
    background: var(--bg-secondary);
    padding: 10px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    transition: all var(--transition-fast);
    position: relative;
  }

  .day-slot.empty {
    opacity: 0.3;
    cursor: default;
    background: transparent;
    border-color: transparent;
  }

  .day-slot.active-slot {
    background: var(--bg-surface);
    border-color: var(--border);
    cursor: pointer;
  }

  .day-slot.active-slot:hover {
    border-color: var(--primary-border);
    box-shadow: var(--shadow-sm);
    transform: translateY(-2px);
  }

  .day-slot.is-today {
    border-color: var(--primary);
    background: var(--primary-light);
  }

  .day-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .day-number {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .today-pill {
    background: var(--primary);
    color: #ffffff;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
  }

  .total-tag {
    font-size: 11px;
    color: var(--text-tertiary);
    font-weight: 500;
  }

  .day-content {
    margin-top: 8px;
  }

  .run-pills-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .metric-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 600;
  }

  .metric-pill.success {
    background: var(--success-light);
    color: var(--success-text);
  }

  .metric-pill.failed {
    background: var(--danger-light);
    color: var(--danger-text);
  }

  .metric-pill .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }

  /* List View */
  .list-wrapper {
    padding: 20px 24px;
  }

  .empty-list-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 60px 20px;
    color: var(--text-tertiary);
  }

  .date-card-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .active-date-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .active-date-card:hover {
    background: var(--bg-hover);
    border-color: var(--primary-border);
    transform: translateX(4px);
  }

  .date-card-left {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .date-badge-box {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-xs);
  }

  .date-box-month {
    font-size: 10px;
    color: var(--text-tertiary);
    font-weight: 600;
  }

  .date-box-day {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
    line-height: 1;
  }

  .date-card-info {
    display: flex;
    flex-direction: column;
  }

  .date-str {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-mono);
  }

  .date-sub {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .date-card-right {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .pills-group {
    display: flex;
    gap: 8px;
  }

  .badge-success-tag {
    font-size: 12px;
    font-weight: 500;
    padding: 3px 9px;
    border-radius: var(--radius-full);
    background: var(--success-light);
    color: var(--success-text);
  }

  .badge-failed-tag {
    font-size: 12px;
    font-weight: 500;
    padding: 3px 9px;
    border-radius: var(--radius-full);
    background: var(--danger-light);
    color: var(--danger-text);
  }

  .arrow-icon-wrap {
    color: var(--text-tertiary);
  }

  @media (max-width: 768px) {
    .calendar-grid {
      gap: 4px;
    }
    .day-slot {
      min-height: 64px;
      padding: 6px;
    }
    .total-tag {
      display: none;
    }
    .metric-pill span:last-child {
      display: none;
    }
    .card-header {
      padding: 14px 16px;
    }
  }
</style>
