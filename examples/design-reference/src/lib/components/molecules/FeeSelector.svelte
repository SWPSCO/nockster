<script lang="ts">
  import Card from '../atoms/Card.svelte';
  
  export type FeeLevel = 'slow' | 'medium' | 'fast';
  
  export let selected: FeeLevel = 'medium';
  export let customFee: number | null = null;
  export let onSelect: (level: FeeLevel) => void = () => {};
  
  interface FeeOption {
    level: FeeLevel;
    label: string;
    time: string;
    rate: number;
    description: string;
  }
  
  const feeOptions: FeeOption[] = [
    {
      level: 'slow',
      label: 'Slow',
      time: '~60 min',
      rate: 5,
      description: 'Economy'
    },
    {
      level: 'medium',
      label: 'Medium',
      time: '~30 min',
      rate: 15,
      description: 'Standard'
    },
    {
      level: 'fast',
      label: 'Fast',
      time: '~10 min',
      rate: 30,
      description: 'Priority'
    }
  ];
  
  function handleSelect(level: FeeLevel) {
    selected = level;
    onSelect(level);
  }
</script>

<div class="fee-selector">
  <h3 class="fee-title">Transaction Fee</h3>
  
  <div class="fee-options">
    {#each feeOptions as option}
      <Card 
        clickable={true}
        selected={selected === option.level}
        padding="small"
        shadow="small"
        on:click={() => handleSelect(option.level)}
      >
        <div class="fee-option">
          <div class="fee-header">
            <span class="fee-label">{option.label}</span>
            <span class="fee-time">{option.time}</span>
          </div>
          <div class="fee-rate">
            <span class="rate-value">{option.rate}</span>
            <span class="rate-unit">sat/vB</span>
          </div>
          <div class="fee-description">{option.description}</div>
        </div>
      </Card>
    {/each}
  </div>
  
  {#if customFee !== null}
    <div class="custom-fee">
      <span class="custom-fee-label">Custom Fee:</span>
      <span class="custom-fee-value">{customFee} sat/vB</span>
    </div>
  {/if}
</div>

<style>
  .fee-selector {
    width: 100%;
  }
  
  .fee-title {
    font-size: var(--font-base);
    font-weight: var(--font-semibold);
    color: var(--color-text);
    margin: 0 0 var(--spacing-md) 0;
  }
  
  .fee-options {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--spacing-sm);
  }
  
  .fee-option {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
    text-align: center;
  }
  
  .fee-header {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  
  .fee-label {
    font-size: var(--font-sm);
    font-weight: var(--font-semibold);
    color: var(--color-text);
  }
  
  .fee-time {
    font-size: var(--font-xs);
    color: var(--color-text-secondary);
  }
  
  .fee-rate {
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 2px;
    margin: var(--spacing-xs) 0;
  }
  
  .rate-value {
    font-size: var(--font-lg);
    font-weight: var(--font-bold);
    color: var(--color-primary);
  }
  
  .rate-unit {
    font-size: var(--font-xs);
    color: var(--color-text-tertiary);
  }
  
  .fee-description {
    font-size: var(--font-xs);
    color: var(--color-text-tertiary);
  }
  
  .custom-fee {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    margin-top: var(--spacing-md);
    padding: var(--spacing-sm);
    background: var(--color-surface);
    border-radius: var(--radius-md);
  }
  
  .custom-fee-label {
    font-size: var(--font-sm);
    color: var(--color-text-secondary);
  }
  
  .custom-fee-value {
    font-size: var(--font-sm);
    font-weight: var(--font-semibold);
    color: var(--color-text);
  }
</style>