<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    open: boolean
    title: string
    confirmText?: string
    danger?: boolean
    onConfirm: () => void
    onCancel: () => void
    children?: Snippet
  }

  let {
    open = $bindable(),
    title,
    confirmText = 'Confirm',
    danger = false,
    onConfirm,
    onCancel,
    children,
  }: Props = $props()

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onCancel()
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onCancel()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" tabindex="-1" onclick={handleBackdropClick}>
    <div class="modal">
      <h3>{title}</h3>
      <div class="body">
        {#if children}
          {@render children()}
        {/if}
      </div>
      <div class="actions">
        <button onclick={onCancel}>Cancel</button>
        <button class:danger onclick={onConfirm}>
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
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--bg);
    border-radius: 8px;
    padding: 20px;
    min-width: 320px;
    max-width: 480px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }

  h3 {
    margin: 0 0 12px;
    font-size: 16px;
  }

  .body {
    margin-bottom: 20px;
    color: var(--text-secondary);
    font-size: 14px;
    line-height: 1.5;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
