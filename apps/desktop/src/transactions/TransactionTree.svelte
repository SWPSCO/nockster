<script lang="ts">
  import type { TransactionTreeNode } from './files';
  export let node: TransactionTreeNode;
</script>

{#if node.children.length}
  <details class="transaction-tree">
    <summary
      >{node.label}{#if node.value}<span>{node.value}</span>{/if}</summary
    >
    <div class="tree-children">
      {#each node.children as child}<svelte:self node={child} />{/each}
    </div>
  </details>
{:else}
  <div class="tree-leaf"><span>{node.label}</span><code>{node.value}</code></div>
{/if}
