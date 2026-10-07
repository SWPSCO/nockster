<script lang="ts">
  import type { LockForm } from './composer';
  export let value: LockForm;
  export let id: string;
</script>

<div class="lock-fields">
  <div class="field-pair">
    <label for="{id}-recipient-kind"
      >Recipient type
      <select id="{id}-recipient-kind" bind:value={value.recipientKind}>
        <option value="address">Address</option><option value="multisig">Multisig</option>
      </select>
    </label>
    <label for="{id}-condition"
      >Spend condition
      <select id="{id}-condition" bind:value={value.condition}>
        <option value="plain">Standard</option><option value="timelock">Timelock</option>
        <option value="hashlock">Hashlock</option><option value="htlc"
          >HTLC · claim or refund</option
        >
        <option value="burn">Burn</option>
      </select>
    </label>
  </div>
  {#if value.recipientKind === 'multisig' && value.condition !== 'burn'}
    <label for="{id}-signers"
      >Signer addresses
      <textarea
        id="{id}-signers"
        rows="3"
        bind:value={value.signers}
        placeholder="One address per line"
      ></textarea>
    </label>
    <label for="{id}-threshold"
      >Required signatures
      <input id="{id}-threshold" type="text" inputmode="numeric" bind:value={value.threshold} />
    </label>
  {/if}
  {#if value.condition === 'timelock'}
    <label for="{id}-unlock"
      >Unlock block height
      <input
        id="{id}-unlock"
        type="text"
        inputmode="numeric"
        bind:value={value.height}
        placeholder="Block height"
      />
    </label>
  {/if}
  {#if value.condition === 'hashlock' || value.condition === 'htlc'}
    <label for="{id}-commitments"
      >Preimage commitments
      <textarea
        id="{id}-commitments"
        rows="2"
        bind:value={value.commitments}
        placeholder="One commitment hash per line"
      ></textarea>
    </label>
    <p class="field-hint">The recipient must reveal the matching preimages to spend this output.</p>
  {/if}
  {#if value.condition === 'htlc'}
    <label for="{id}-refund"
      >Refund address
      <input
        id="{id}-refund"
        bind:value={value.refundAddress}
        placeholder="Address that can reclaim the funds"
      />
    </label>
    <label for="{id}-refund-height"
      >Refund block height
      <input
        id="{id}-refund-height"
        inputmode="numeric"
        bind:value={value.refundHeight}
        placeholder="Refund becomes available at this height"
      />
    </label>
  {/if}
  {#if value.condition === 'burn'}
    <p class="transaction-notice">Burned funds cannot be spent or recovered.</p>
  {/if}
  <label class="check-field">
    <input type="checkbox" bind:checked={value.privateOutput} />
    <span
      >Keep the lock details private<small>Include only the lock root in the output.</small></span
    >
  </label>
</div>
