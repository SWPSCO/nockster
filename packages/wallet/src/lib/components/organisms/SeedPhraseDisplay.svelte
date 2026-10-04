<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';

  export let seedPhrase: string[] = [];
  export const revealed: boolean = true;
  export let onContinue: () => void = () => {};
  export let onBack: () => void = () => {};

  let copied = false;

  async function copySeedPhrase() {
    try {
      const text = seedPhrase.join(' ');

      // Try modern clipboard API first
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        // Fallback for Chrome extensions or older browsers
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      }

      copied = true;
      setTimeout(() => {
        copied = false;
      }, 2000);
    } catch (error) {
      console.error('Failed to copy seed phrase:', error);
      // Try fallback method if modern API fails
      try {
        const textarea = document.createElement('textarea');
        textarea.value = seedPhrase.join(' ');
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);

        copied = true;
        setTimeout(() => {
          copied = false;
        }, 2000);
      } catch (fallbackError) {
        console.error('Fallback copy also failed:', fallbackError);
      }
    }
  }

  function downloadSeedPhrase() {
    const text = seedPhrase.join(' ');
    const element = document.createElement('a');
    element.setAttribute('href', 'data:text/plain;charset=utf-8,' + encodeURIComponent(text));
    element.setAttribute('download', 'nockster-seed-phrase.txt');
    element.style.display = 'none';
    document.body.appendChild(element);
    element.click();
    document.body.removeChild(element);
  }

  // Show error message if no seed phrase is provided
  $: if (seedPhrase.length === 0) {
    console.error('No seed phrase provided to SeedPhraseDisplay component');
  }
</script>

<div class="seed-phrase-display fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />

  <div class="content">
    <h2 class="seed-title">Your Secret Recovery Phrase</h2>
    <p class="seed-subtitle">
      Write down these {seedPhrase.length} words in order. This is the only way to recover your wallet.
    </p>

    <div class="seed-grid" class:large-grid={seedPhrase.length === 24}>
      {#each seedPhrase as word, index}
        <div class="seed-word">
          <span class="seed-word-number">{index + 1}.</span>
          <span class="seed-word-text">{word}</span>
        </div>
      {/each}
    </div>

    <div class="action-bar" class:large-mode={seedPhrase.length === 24}>
      <button
        class="action-button"
        class:active={copied}
        on:click={copySeedPhrase}
        title="Copy to clipboard"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          {#if copied}
            <path
              d="M13.5 4.5L6 12L2.5 8.5"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          {:else}
            <rect
              x="5.5"
              y="5.5"
              width="7"
              height="9"
              rx="0.5"
              stroke="currentColor"
              stroke-width="1.5"
              fill="none"
            />
            <rect
              x="3.5"
              y="1.5"
              width="7"
              height="9"
              rx="0.5"
              stroke="currentColor"
              stroke-width="1.5"
              fill="none"
            />
          {/if}
        </svg>
        <span>{copied ? 'Copied' : 'Copy'}</span>
      </button>

      <div class="action-divider"></div>

      <button class="action-button" on:click={downloadSeedPhrase} title="Save as file">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M8 2V10M8 10L5 7M8 10L11 7"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <path
            d="M2 11V13C2 13.5523 2.44772 14 3 14H13C13.5523 14 14 13.5523 14 13V11"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
        <span>Save</span>
      </button>
    </div>

    <div class="warning-box" class:large-mode={seedPhrase.length === 24}>
      ⚠️ Never share this phrase. Anyone with these words can steal your funds.
    </div>
  </div>

  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={onContinue}>I've Written It Down</Button>
  </div>
</div>

<style>
  .seed-phrase-display {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
    position: relative;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 8px 16px 0;
    overflow: hidden;
  }

  .seed-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 6px;
    margin-top: -4px;
    text-align: center;
  }

  .seed-subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 12px;
    line-height: 1.4;
    text-align: center;
  }

  .seed-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
    margin-bottom: 8px;
  }

  .seed-grid.large-grid {
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }

  .seed-word {
    background: var(--color-surface);
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .large-grid .seed-word {
    padding: 6px 8px;
  }

  .seed-word-number {
    font-size: 13px;
    color: var(--color-text-tertiary);
    min-width: 18px;
  }

  .large-grid .seed-word-number {
    font-size: 12px;
  }

  .seed-word-text {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .large-grid .seed-word-text {
    font-size: 13px;
  }

  .action-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0;
    margin: 12px auto 12px;
    background: var(--color-surface);
    border: none;
    border-radius: 24px;
    padding: 4px;
    width: fit-content;
  }

  .action-bar.large-mode {
    margin-top: 0;
    margin-bottom: 6px;
  }

  .action-button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    background: transparent;
    border: none;
    border-radius: 20px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-button:hover {
    color: var(--color-text);
  }

  .action-button.active {
    color: var(--color-success);
  }

  .action-button svg {
    width: 16px;
    height: 16px;
  }

  .action-divider {
    width: 1px;
    height: 18px;
    background: var(--color-border);
    opacity: 0.5;
  }

  .warning-box {
    background: #fff5f5;
    border: none;
    border-radius: 8px;
    padding: 10px 12px;
    margin: 12px 0 16px;
    font-size: 13px;
    color: var(--color-error);
    line-height: 1.4;
    text-align: center;
    font-weight: 500;
  }

  .warning-box.large-mode {
    margin-top: 0;
  }

  .content :global(.button--primary) {
    margin-top: auto;
    margin-bottom: 0;
  }
</style>
