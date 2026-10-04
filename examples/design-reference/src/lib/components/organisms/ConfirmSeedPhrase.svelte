<script lang="ts">
  import { onMount } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import ProgressBar from '../atoms/ProgressBar.svelte';

  export let seedPhrase: string[] = [];
  export let onConfirm: () => void = () => {};
  export let onBack: () => void = () => {};

  // Select a random word position to verify
  let targetIndex: number = 0;
  let wordOptions: string[] = [];
  let selectedWord: string = '';
  let error: string = '';
  let isCorrect: boolean = false;

  onMount(() => {
    initializeQuestion();
  });

  function initializeQuestion() {
    // Pick a random word index (1-12)
    targetIndex = Math.floor(Math.random() * 12) + 1;

    // Get the correct word
    const correctWord = seedPhrase[targetIndex - 1];

    // Generate 3 random wrong words from a word list
    const wordList = [
      'abandon', 'ability', 'absent', 'absorb', 'abstract', 'absurd',
      'abuse', 'access', 'accident', 'account', 'accuse', 'achieve',
      'acid', 'acoustic', 'acquire', 'across', 'actor', 'actress',
      'actual', 'adapt', 'address', 'adjust', 'admit', 'adult'
    ];

    // Filter out the correct word and any words already in seed phrase
    const availableWords = wordList.filter(w =>
      w !== correctWord && !seedPhrase.includes(w)
    );

    // Pick 3 random wrong words
    const wrongWords: string[] = [];
    while (wrongWords.length < 3 && availableWords.length > 0) {
      const randomIndex = Math.floor(Math.random() * availableWords.length);
      const word = availableWords[randomIndex];
      if (!wrongWords.includes(word)) {
        wrongWords.push(word);
      }
    }

    // Combine correct word with wrong words and shuffle
    wordOptions = [correctWord, ...wrongWords].sort(() => Math.random() - 0.5);
    selectedWord = '';
    error = '';
    isCorrect = false;
  }

  function selectWord(word: string) {
    selectedWord = word;
    error = '';

    // Check if the selected word is correct
    const correctWord = seedPhrase[targetIndex - 1];
    if (word === correctWord) {
      isCorrect = true;
    } else {
      error = 'Incorrect word. Please try again.';
      isCorrect = false;
    }
  }

  function handleConfirm() {
    if (!selectedWord) {
      error = 'Please select a word';
      return;
    }

    if (isCorrect) {
      onConfirm();
    }
  }
</script>

<div class="confirm-seed fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  <div class="progress-wrapper">
    <ProgressBar currentStep={2} totalSteps={3} />
  </div>

  <div class="content">
    <h2 class="title">Confirm Your Phrase</h2>
    <p class="subtitle">
      Select the correct word to verify you've saved your seed phrase.
    </p>

    <div class="question-container">
      <label class="question-label">What is word #{targetIndex}?</label>

      <div class="word-options">
        {#each wordOptions as word}
          <button
            class="word-option"
            class:selected={selectedWord === word}
            class:correct={selectedWord === word && isCorrect}
            class:incorrect={selectedWord === word && !isCorrect && error}
            on:click={() => selectWord(word)}
          >
            {word}
          </button>
        {/each}
      </div>

      {#if error}
        <span class="error-message">{error}</span>
      {/if}

      {#if isCorrect}
        <span class="success-message">✓ Correct!</span>
      {/if}
    </div>
  </div>

  <div class="button-footer">
    <Button
      variant="primary"
      fullWidth={true}
      on:click={handleConfirm}
      disabled={!isCorrect}
    >
      Continue
    </Button>
  </div>
</div>

<style>
  .confirm-seed {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
    position: relative;
  }

  .progress-wrapper {
    position: absolute;
    top: 28px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 20px 16px 16px;
  }

  .title {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
    margin-top: 0;
    text-align: center;
  }

  .subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 0;
    line-height: 1.4;
    text-align: center;
    max-width: 280px;
    margin-left: auto;
    margin-right: auto;
  }

  .question-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 20px 0;
  }

  .question-label {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 24px;
    text-align: center;
  }

  .word-options {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    width: 100%;
    max-width: 320px;
    margin: 0 auto 40px;
  }

  .word-option {
    padding: 14px 16px;
    background: var(--color-surface);
    border: 1.5px solid rgba(255, 255, 255, 0.15);
    border-radius: 10px;
    font-size: 15px;
    font-weight: 500;
    color: var(--color-text);
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: center;
    width: 100%;
    min-height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .word-option:hover {
    background: rgba(255, 255, 255, 0.05);
    border-color: rgba(255, 255, 255, 0.25);
  }

  .word-option.selected {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.4);
    border-width: 2px;
    color: var(--color-text);
    font-weight: 600;
    padding: 13px 15px;
  }

  .word-option.correct {
    background: rgba(52, 199, 89, 0.15);
    border-color: var(--color-success);
    color: var(--color-success);
    font-weight: 600;
  }

  .word-option.incorrect {
    background: rgba(255, 59, 48, 0.15);
    border-color: var(--color-error);
    color: var(--color-error);
    font-weight: 600;
  }

  .error-message {
    display: block;
    font-size: 14px;
    color: var(--color-error);
    text-align: center;
    margin-top: -20px;
    margin-bottom: 20px;
    font-weight: 500;
  }

  .success-message {
    display: block;
    font-size: 14px;
    color: var(--color-success);
    text-align: center;
    margin-top: -20px;
    margin-bottom: 20px;
    font-weight: 600;
  }

  .button-footer :global(.button--primary) {
    background: var(--color-text);
    color: var(--color-background);
  }

  .button-footer :global(.button--primary:disabled) {
    background: var(--color-surface);
    color: var(--color-text-tertiary);
    cursor: not-allowed;
    border: none;
  }
</style>