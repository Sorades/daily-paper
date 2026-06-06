<script lang="ts">
  interface Props {
    message: string
    onDismiss?: () => void
  }

  let { message, onDismiss }: Props = $props()
</script>

<div class="banner">
  <span>{message}</span>
  {#if onDismiss}
    <button class="dismiss" onclick={onDismiss}>&times;</button>
  {/if}
</div>

<style>
  .banner {
    background: #fee2e2;
    color: #991b1b;
    padding: 10px 16px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
  }

  .dismiss {
    background: none;
    border: none;
    color: inherit;
    font-size: 18px;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0.7;
  }
  .dismiss:hover {
    opacity: 1;
    background: none;
  }
</style>
