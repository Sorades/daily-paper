<script lang="ts">
  interface Props {
    size?: 'small' | 'medium' | 'large'
    color?: string
  }

  let { size = 'medium', color }: Props = $props()
</script>

<div
  class="spinner {size}"
  style:border-top-color={color}
></div>

<style>
  .spinner {
    border: 2px solid var(--border);
    border-top-color: var(--primary);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    display: inline-block;
    flex-shrink: 0;
  }

  .small {
    width: 14px;
    height: 14px;
    border-width: 2px;
  }

  .medium {
    width: 24px;
    height: 24px;
    border-width: 2.5px;
  }

  .large {
    width: 36px;
    height: 36px;
    border-width: 3px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
