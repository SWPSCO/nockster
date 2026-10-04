<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, scale } from 'svelte/transition';

  export let onContinue: () => void = () => {};
  export let duration: number = 2800; // Total animation duration

  let visible = true;
  let showSuccess = false;
  let showMessage = false;

  onMount(() => {
    // Staged reveal
    setTimeout(() => (showSuccess = true), 200);
    setTimeout(() => (showMessage = true), 800);

    // Auto-dismiss and navigate after animation
    setTimeout(() => {
      visible = false;
      setTimeout(onContinue, 300); // Wait for fade out
    }, duration);
  });
</script>

{#if visible}
  <div class="wallet-created-animation fixed-screen" transition:fade={{ duration: 300 }}>
    <div class="animation-container">
      {#if showSuccess}
        <div class="success-icon" in:scale={{ duration: 600, delay: 0 }}>
          <div class="icon-glow"></div>
          <svg width="90" height="90" viewBox="0 0 90 90" fill="none">
            <circle
              cx="45"
              cy="45"
              r="42"
              stroke="url(#gradient)"
              stroke-width="2.5"
              class="circle-draw"
            />
            <path
              d="M28 45L38 55L62 31"
              stroke="url(#gradient)"
              stroke-width="3"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="check-draw"
            />
            <defs>
              <linearGradient id="gradient" x1="0" y1="0" x2="90" y2="90">
                <stop offset="0%" stop-color="#22c55e" />
                <stop offset="100%" stop-color="#16a34a" />
              </linearGradient>
            </defs>
          </svg>
        </div>
      {/if}

      {#if showMessage}
        <div class="message-container" in:fade={{ duration: 400 }}>
          <h1 class="title">Congratulations!</h1>
          <p class="subtitle">Your new wallet has been created successfully</p>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .wallet-created-animation {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: var(--color-background);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .animation-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 40px;
    gap: 32px;
  }

  .success-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon-glow {
    position: absolute;
    width: 120px;
    height: 120px;
    background: radial-gradient(circle, rgba(34, 197, 94, 0.15) 0%, transparent 70%);
    border-radius: 50%;
    animation: glow 2s ease-in-out infinite;
  }

  @keyframes glow {
    0%,
    100% {
      transform: scale(1);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.2);
      opacity: 0.8;
    }
  }

  .circle-draw {
    stroke-dasharray: 264;
    stroke-dashoffset: 264;
    animation: drawCircle 1s cubic-bezier(0.65, 0, 0.35, 1) forwards;
  }

  .check-draw {
    stroke-dasharray: 70;
    stroke-dashoffset: 70;
    animation: drawCheck 0.6s cubic-bezier(0.65, 0, 0.35, 1) forwards;
    animation-delay: 0.7s;
  }

  @keyframes drawCircle {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes drawCheck {
    to {
      stroke-dashoffset: 0;
    }
  }

  .message-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .title {
    font-size: 32px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
    letter-spacing: -0.5px;
  }

  .subtitle {
    font-size: 16px;
    color: var(--color-text-secondary);
    font-weight: 400;
    line-height: 1.5;
    max-width: 280px;
    margin: 0;
  }
</style>
