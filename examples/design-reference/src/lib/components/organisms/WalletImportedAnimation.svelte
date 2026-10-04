<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, scale } from 'svelte/transition';

  export let onComplete: () => void = () => {};
  export let duration: number = 2800; // Total animation duration

  let visible = true;
  let progress = 0;
  let showSuccess = false;
  let showMessage = false;

  onMount(() => {
    // Animate progress
    const progressInterval = setInterval(() => {
      if (progress < 100) {
        progress += 3;
      } else {
        clearInterval(progressInterval);
        showSuccess = true;
        setTimeout(() => showMessage = true, 200);
      }
    }, 20);

    // Auto-dismiss and navigate after animation
    setTimeout(() => {
      visible = false;
      setTimeout(onComplete, 300); // Wait for fade out
    }, duration);

    return () => clearInterval(progressInterval);
  });
</script>

{#if visible}
  <div class="wallet-imported-animation fixed-screen" transition:fade={{ duration: 300 }}>
    <div class="animation-container">
      <div class="import-icon">
        <div class="icon-glow"></div>
        <svg width="90" height="90" viewBox="0 0 90 90" fill="none">
          <!-- Background circle -->
          <circle cx="45" cy="45" r="42" stroke="#e5e7eb" stroke-width="2" opacity="0.2"/>

          <!-- Progress circle -->
          <circle
            cx="45"
            cy="45"
            r="42"
            stroke="url(#importGradient)"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-dasharray="264"
            stroke-dashoffset={264 - (264 * progress) / 100}
            transform="rotate(-90 45 45)"
            class="progress-circle"
          />

          {#if !showSuccess}
            <!-- Download icon during import -->
            <path
              d="M45 25V50M45 50L35 40M45 50L55 40"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="import-arrow"
            />
            <path
              d="M28 58H62"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
          {:else}
            <!-- Success checkmark after import -->
            <path
              d="M30 45L40 55L60 35"
              stroke="url(#importGradient)"
              stroke-width="3"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="check-draw"
              in:scale={{ duration: 400 }}
            />
          {/if}

          <defs>
            <linearGradient id="importGradient" x1="0" y1="0" x2="90" y2="90">
              <stop offset="0%" stop-color="#3b82f6" />
              <stop offset="100%" stop-color="#2563eb" />
            </linearGradient>
          </defs>
        </svg>
      </div>

      {#if showMessage}
        <div class="message-container" in:fade={{ duration: 400 }}>
          <h1 class="title">Welcome Back!</h1>
          <p class="subtitle">
            Your wallet has been successfully imported
          </p>
        </div>
      {:else}
        <div class="loading-container">
          <p class="loading-text">
            {#if progress < 100}
              Importing your wallet...
            {:else}
              Verifying...
            {/if}
          </p>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .wallet-imported-animation {
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

  .import-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
  }

  .icon-glow {
    position: absolute;
    width: 120px;
    height: 120px;
    background: radial-gradient(circle, rgba(59, 130, 246, 0.12) 0%, transparent 70%);
    border-radius: 50%;
    animation: glow 2s ease-in-out infinite;
  }

  @keyframes glow {
    0%, 100% {
      transform: scale(1);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.15);
      opacity: 0.8;
    }
  }

  .progress-circle {
    transition: stroke-dashoffset 0.05s linear;
  }

  .import-arrow {
    animation: downloadPulse 1.2s ease-in-out infinite;
  }

  @keyframes downloadPulse {
    0%, 100% {
      transform: translateY(0);
      opacity: 0.7;
    }
    50% {
      transform: translateY(3px);
      opacity: 1;
    }
  }

  .check-draw {
    stroke-dasharray: 70;
    stroke-dashoffset: 70;
    animation: drawCheck 0.6s cubic-bezier(0.65, 0, 0.35, 1) forwards;
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

  .loading-container {
    min-height: 60px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .loading-text {
    font-size: 16px;
    color: var(--color-text-tertiary);
    font-weight: 500;
    margin: 0;
  }
</style>