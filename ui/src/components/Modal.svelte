<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'

  interface Props {
    open: boolean
    title: string
    confirmText?: string
    cancelText?: string
    danger?: boolean
    loading?: boolean
    onConfirm: () => void
    onCancel: () => void
    children?: Snippet
  }

  let {
    open = $bindable(),
    title,
    confirmText = 'Confirm',
    cancelText = 'Cancel',
    danger = false,
    loading = false,
    onConfirm,
    onCancel,
    children,
  }: Props = $props()

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !loading) onCancel()
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget && !loading) onCancel()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" tabindex="-1" onclick={handleBackdropClick}>
    <div class="modal">
      <div class="modal-header">
        <div class="title-with-icon">
          {#if danger}
            <div class="danger-badge">
              <Icon name="alert" size={16} />
            </div>
          {/if}
          <h3>{title}</h3>
        </div>
        <button class="close-btn" onclick={onCancel} disabled={loading} aria-label="Close">
          <Icon name="x" size={16} />
        </button>
      </div>

      <div class="modal-body">
        {#if children}
          {@render children()}
        {/if}
      </div>

      <div class="modal-actions">
        <button class="secondary" onclick={onCancel} disabled={loading}>
          {cancelText}
        </button>
        <button
          class={danger ? 'danger-solid' : 'primary'}
          onclick={onConfirm}
          disabled={loading}
        >
          {#if loading}
            <span class="spinner-small"></span>
          {/if}
          {confirmText}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(15, 23, 42, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 16px;
    animation: fadeIn 0.15s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .modal {
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    width: 100%;
    max-width: 460px;
    box-shadow: var(--shadow-xl);
    overflow: hidden;
    animation: scaleUp 0.15s ease-out;
    display: flex;
    flex-direction: column;
  }

  @keyframes scaleUp {
    from {
      transform: scale(0.96);
      opacity: 0;
    }
    to {
      transform: scale(1);
      opacity: 1;
    }
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 20px 14px;
  }

  .title-with-icon {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .danger-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-full);
    background: var(--danger-light);
    color: var(--danger-text);
  }

  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .close-btn {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 6px;
    border-radius: var(--radius-md);
    color: var(--text-tertiary);
    cursor: pointer;
  }

  .close-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 0 20px 20px;
    color: var(--text-secondary);
    font-size: 14px;
    line-height: 1.6;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 16px 20px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
  }

  .spinner-small {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
