<script lang="ts">
  import { onMount } from 'svelte';
  import { hardwareSession, perform } from './session';
  import { firmwareUpdate } from './firmware';
  import type { HardwareDeviceDescriptor } from './transport';
  import DeviceWallets from './DeviceWallets.svelte';
  import DeviceFirmware from './DeviceFirmware.svelte';
  import DeviceSecurity from './DeviceSecurity.svelte';
  import DeviceTools from './DeviceTools.svelte';
  import DeviceReset from './DeviceReset.svelte';
  import './hardware.css';

  let tab = 'Wallets';
  let devices: HardwareDeviceDescriptor[] = [];
  let scanning = false;
  let scanned = false;
  let pin = '';
  let offlineTools = false;
  $: connected = $hardwareSession.connection === 'connected';
  $: busy = Boolean($hardwareSession.busy);
  $: if (!connected) pin = '';

  async function scan() {
    scanning = true;
    try {
      devices = await hardwareSession.list();
      scanned = true;
    } finally {
      scanning = false;
    }
  }
  async function unlock() {
    const entered = pin;
    pin = '';
    await hardwareSession.unlock(entered);
  }
  onMount(() => {
    void perform(scan);
  });
</script>

<div class="hardware-workspace">
  <header class="hardware-heading">
    <div>
      <h1>Hardware</h1>
      {#if !connected}<p>Manage your Nockster device.</p>{/if}
    </div>
    {#if connected}
      <button
        class="hw-button"
        disabled={busy || $firmwareUpdate.installing}
        onclick={() => perform(() => hardwareSession.disconnect())}>Disconnect</button
      >
    {/if}
  </header>

  {#if $hardwareSession.error || busy}
    <div class="hw-feedback">
      {#if $hardwareSession.error}
        <div class="hw-notice hw-error" role="alert">
          <span>{$hardwareSession.error}</span>
          <button
            class="hw-link"
            onclick={hardwareSession.clearError}
            aria-label="Dismiss hardware error">Dismiss</button
          >
        </div>
      {/if}
      {#if busy}
        <div class="hw-notice" role="status">
          <span class="hw-busy-mark" aria-hidden="true"></span>{$hardwareSession.busy}
        </div>
      {/if}
    </div>
  {/if}

  {#if !connected}
    <section class="hw-connect">
      <h2>Connect your Nockster</h2>
      <p>Plug in your device with a USB data cable, then select it below.</p>
      <div class="hw-device-list">
        {#each devices as device (device.id)}
          <div class="hw-device-row">
            <div>
              <strong>{device.name}</strong><small
                >{device.serial || `USB device ${device.id.slice(-12)}`}</small
              >
            </div>
            <button
              class="hw-button hw-primary"
              disabled={busy || scanning}
              onclick={() => perform(() => hardwareSession.connect(device))}>Connect</button
            >
          </div>
        {:else}
          <p class="hw-empty">
            {scanning
              ? 'Looking for connected devices…'
              : scanned
                ? 'No Nockster found. Check the cable and close other apps using your device.'
                : 'Check for connected devices to get started.'}
          </p>
        {/each}
      </div>
      <div class="hw-actions">
        <button class="hw-button" disabled={scanning || busy} onclick={() => perform(scan)}
          >{scanning ? 'Checking…' : 'Refresh devices'}</button
        >
        <a class="hw-link" href="https://my.nockster.com/manual/">Device manual</a>
        <button class="hw-link" onclick={() => (offlineTools = !offlineTools)}
          >{offlineTools ? 'Close offline tools' : 'Open offline tools'}</button
        >
      </div>
    </section>
    {#if offlineTools}<div class="hw-section"><DeviceTools offline /></div>{/if}
  {:else}
    <div class="hw-device-summary">
      <div>
        <strong>{$hardwareSession.descriptor?.name || 'Nockster'}</strong><span
          >{$hardwareSession.descriptor?.serial || 'USB connected'}</span
        >
      </div>
      <span class="hw-lock-state">{$hardwareSession.locked ? 'Locked' : 'Unlocked'}</span>
      {#if !$hardwareSession.locked}<button
          class="hw-link"
          disabled={busy}
          onclick={() => perform(hardwareSession.lock)}>Lock device</button
        >{/if}
    </div>
    {#if $hardwareSession.locked}
      <section class="hw-unlock">
        <h2>Unlock your device</h2>
        <p>
          Enter the PIN for your Nockster.{$hardwareSession.attempts !== null
            ? ` ${$hardwareSession.attempts} attempts remaining.`
            : ''}
        </p>
        <form
          onsubmit={event => {
            event.preventDefault();
            void perform(unlock);
          }}
        >
          <label for="hardware-pin">Device PIN</label>
          <input
            id="hardware-pin"
            type="password"
            bind:value={pin}
            autocomplete="off"
            disabled={busy}
            maxlength="32"
          />
          <button
            class="hw-button hw-primary"
            disabled={busy || !pin || $hardwareSession.attempts === 0}>Unlock device</button
          >
        </form>
        <details class="hw-details">
          <summary>Forgot your device PIN?</summary>
          <p>Reset the device and restore your wallets from their backups.</p>
          <DeviceReset />
        </details>
      </section>
    {:else}
      <nav class="hw-tabs" aria-label="Hardware sections">
        {#each ['Wallets', 'Firmware', 'Security', 'Tools'] as item}
          <button aria-current={tab === item ? 'page' : undefined} onclick={() => (tab = item)}
            >{item}</button
          >
        {/each}
      </nav>
      <div class="hw-panel">
        {#if tab === 'Wallets'}<DeviceWallets />
        {:else if tab === 'Firmware'}<DeviceFirmware />
        {:else if tab === 'Security'}<DeviceSecurity />
        {:else}<DeviceTools />{/if}
      </div>
    {/if}
  {/if}
</div>
