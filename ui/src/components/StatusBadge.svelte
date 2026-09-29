<script lang="ts">
  import Icon from './Icon.svelte'

  interface Props {
    status: string
    size?: 'sm' | 'md'
    pulse?: boolean
  }

  let { status, size = 'md', pulse = false }: Props = $props()

  const normalized = $derived(status.toLowerCase())

  const config = $derived.by(() => {
    switch (normalized) {
      case 'running':
        return { label: 'Running', cls: 'badge-running', dot: true, icon: null }
      case 'succeeded':
      case 'success':
        return { label: 'Succeeded', cls: 'badge-success', dot: false, icon: 'check' as const }
      case 'failed':
      case 'error':
        return { label: 'Failed', cls: 'badge-failed', dot: false, icon: 'alert' as const }
      case 'blocked':
      case 'cancelled':
        return { label: status, cls: 'badge-warning', dot: false, icon: 'alert' as const }
      case 'skipped':
        return { label: 'Skipped', cls: 'badge-skipped', dot: false, icon: null }
      case 'pending':
      default:
        return { label: status || 'Pending', cls: 'badge-pending', dot: false, icon: null }
    }
  })
</script>

<span class="badge {config.cls} {size}">
  {#if config.dot || pulse || normalized === 'running'}
    <span class="pulse-dot"></span>
  {:else if config.icon}
    <Icon name={config.icon} size={size === 'sm' ? 12 : 13} />
  {/if}
  <span>{config.label}</span>
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 500;
    line-height: 1;
    border-radius: var(--radius-full);
    white-space: nowrap;
    border: 1px solid transparent;
    user-select: none;
    letter-spacing: 0.01em;
  }

  .badge.sm {
    padding: 3px 8px;
    font-size: 11px;
  }

  .badge.md {
    padding: 4px 10px;
    font-size: 12px;
  }

  .badge-running {
    background: var(--primary-light);
    color: var(--primary-text);
    border-color: var(--primary-border);
  }

  .badge-success {
    background: var(--success-light);
    color: var(--success-text);
    border-color: var(--success-border);
  }

  .badge-failed {
    background: var(--danger-light);
    color: var(--danger-text);
    border-color: var(--danger-border);
  }

  .badge-warning {
    background: var(--warning-light);
    color: var(--warning-text);
    border-color: var(--warning-border);
  }

  .badge-skipped {
    background: var(--bg-tertiary);
    color: var(--text-tertiary);
    border-color: var(--border);
  }

  .badge-pending {
    background: var(--bg-secondary);
    color: var(--text-secondary);
    border-color: var(--border);
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.5s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.85);
    }
  }
</style>
