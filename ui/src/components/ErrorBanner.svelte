<script lang="ts">
  import Icon from './Icon.svelte'

  interface Props {
    message: string
    title?: string
    type?: 'error' | 'warning' | 'info' | 'success'
    onDismiss?: () => void
  }

  let { message, title, type = 'error', onDismiss }: Props = $props()
</script>

<div class="banner {type}">
  <div class="icon-wrap">
    <Icon name={type === 'success' ? 'check' : 'alert'} size={18} />
  </div>
  <div class="content">
    {#if title}
      <div class="title">{title}</div>
    {/if}
    <div class="message">{message}</div>
  </div>
  {#if onDismiss}
    <button class="dismiss-btn" onclick={onDismiss} aria-label="Dismiss">
      <Icon name="x" size={16} />
    </button>
  {/if}
</div>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px;
    border-radius: var(--radius-md);
    border: 1px solid transparent;
    font-size: 13px;
    line-height: 1.5;
    box-shadow: var(--shadow-xs);
    animation: slideDown 0.2s ease-out;
  }

  @keyframes slideDown {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .icon-wrap {
    flex-shrink: 0;
    margin-top: 1px;
  }

  .content {
    flex: 1;
    min-width: 0;
  }

  .title {
    font-weight: 600;
    margin-bottom: 2px;
  }

  .message {
    word-break: break-word;
  }

  .banner.error {
    background: var(--danger-light);
    color: var(--danger-text);
    border-color: var(--danger-border);
  }

  .banner.warning {
    background: var(--warning-light);
    color: var(--warning-text);
    border-color: var(--warning-border);
  }

  .banner.info {
    background: var(--info-light);
    color: var(--info-text);
    border-color: var(--info-border);
  }

  .banner.success {
    background: var(--success-light);
    color: var(--success-text);
    border-color: var(--success-border);
  }

  .dismiss-btn {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 4px;
    border-radius: var(--radius-sm);
    color: inherit;
    opacity: 0.7;
    margin-left: auto;
    cursor: pointer;
  }

  .dismiss-btn:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.05);
  }
</style>
