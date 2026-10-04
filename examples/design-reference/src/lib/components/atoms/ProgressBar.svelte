<script lang="ts">
  export let currentStep: number = 1;
  export let totalSteps: number = 4;

  $: progress = (currentStep / totalSteps) * 100;
</script>

<div class="progress-bar">
  <div class="progress-track">
    <div class="progress-fill" style="width: {progress}%"></div>
  </div>
</div>

<style>
  .progress-bar {
    width: 100px;
  }

  .progress-track {
    width: 100%;
    height: 3px;
    background: var(--color-border);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--color-text);
    border-radius: 2px;
    transition: width 0.3s ease;
  }
</style>