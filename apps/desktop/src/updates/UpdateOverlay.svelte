<script lang="ts">
  import { desktopUpdates } from './session';
</script>

{#if $desktopUpdates.installing}
  <div class="update-overlay" role="status" aria-live="polite">
    <div>
      <strong>Installing update…</strong>
      <p>Your wallet is locked. Nockster will restart when the update is ready.</p>
    </div>
  </div>
{/if}

<style>
  .update-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: grid;
    place-items: center;
    background: var(--color-background);
    color: var(--color-text);
    padding: 32px;
  }
  .update-overlay > div {
    max-width: 360px;
  }
  strong {
    font-size: 20px;
  }
  p {
    margin-top: 12px;
    line-height: 1.5;
  }
</style>
