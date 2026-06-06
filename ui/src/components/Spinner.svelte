<script lang="ts">
  interface Props {
    size?: 'small' | 'medium'
  }

  let { size = 'medium' }: Props = $props()
</script>

<div class="spinner {size}"></div>

<style>
  .spinner {
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  .small {
    width: 16px;
    height: 16px;
  }

  .medium {
    width: 24px;
    height: 24px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
