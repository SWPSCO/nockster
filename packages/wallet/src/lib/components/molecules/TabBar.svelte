<script lang="ts">
  import Icon from '../atoms/Icon.svelte';

  type TabItem = {
    id: string;
    label: string;
    icon: string;
    path: string;
  };

  export let tabs: TabItem[] = [
    { id: 'wallet', label: 'Wallet', icon: 'wallet', path: '/dashboard' },
    { id: 'send', label: 'Send', icon: 'send', path: '/send' },
    { id: 'receive', label: 'Receive', icon: 'receive', path: '/receive' },
    { id: 'history', label: 'History', icon: 'history', path: '/history' },
    { id: 'settings', label: 'Settings', icon: 'settings', path: '/settings' }
  ];

  export let activeTab: string = 'wallet';
  export let onTabChange: (tab: TabItem) => void = () => {};

  function handleTabClick(tab: TabItem) {
    activeTab = tab.id;
    onTabChange(tab);
  }
</script>

<nav class="tab-bar">
  {#each tabs as tab}
    <button
      class="tab-item"
      class:active={activeTab === tab.id}
      on:click={() => handleTabClick(tab)}
      type="button"
      aria-current={activeTab === tab.id ? 'page' : undefined}
    >
      <Icon name={tab.icon} size="medium" color={activeTab === tab.id ? '#000' : '#999'} />
      <span class="tab-label">{tab.label}</span>
    </button>
  {/each}
</nav>

<style>
  .tab-bar {
    display: flex;
    align-items: center;
    justify-content: space-around;
    height: 60px;
    background: var(--color-background);
    border-top: 1px solid var(--color-border) 156;
    position: sticky;
    bottom: 0;
    z-index: 20;
  }

  .tab-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 100%;
    padding: 8px;
    background: transparent;
    border: none;
    cursor: pointer;
    transition: all 150ms ease;
    position: relative;
  }

  .tab-item::before {
    content: '';
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    width: 0;
    height: 2px;
    background: var(--color-text);
    transition: width 150ms ease;
  }

  .tab-item.active::before {
    width: 100%;
  }

  .tab-item:hover {
    background: var(--color-surface);
  }

  .tab-item:active {
    background: var(--color-surface);
  }

  .tab-label {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-tertiary);
    transition: color 150ms ease;
  }

  .tab-item.active .tab-label {
    color: var(--color-text);
  }

  .tab-item:focus-visible {
    outline: none;
    background: var(--color-surface);
  }

  @media (max-width: 360px) {
    .tab-label {
      display: none;
    }

    .tab-item {
      gap: 0;
    }
  }
</style>
