<script lang="ts">
  import type { RunStatus, StageStatus } from '../types'

  interface Props {
    status: RunStatus | StageStatus | string
  }

  let { status }: Props = $props()

  const colorMap: Record<string, string> = {
    Running: 'running',
    Succeeded: 'success',
    Failed: 'failed',
    Blocked: 'warning',
    Cancelled: 'warning',
    Pending: 'pending',
    Skipped: 'pending',
  }

  let cls = $derived(colorMap[status] ?? 'pending')
</script>

<span class="badge {cls}">{status}</span>

<style>
  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 12px;
    font-weight: 500;
    line-height: 1.4;
    white-space: nowrap;
  }

  .running {
    background: #dbeafe;
    color: #1e40af;
  }

  .success {
    background: #dcfce7;
    color: #166534;
  }

  .failed {
    background: #fee2e2;
    color: #991b1b;
  }

  .warning {
    background: #fef3c7;
    color: #92400e;
  }

  .pending {
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }
</style>
