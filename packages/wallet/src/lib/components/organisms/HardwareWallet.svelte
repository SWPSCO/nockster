<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import { walletStore } from '../../stores/wallet';
  import { NocksterDevice, FEATURE_SEED_LABELS } from '@swps/nockster-js';
  import { cheetahPkhB58, ensureVaultReady } from '../../../vault/wasm';
  import {
    NOCKSTER_HID_FILTER,
    isNockster,
    requireDeviceInfo,
    hardwareAddresses,
    type NocksterHIDDevice,
    type HardwareAddress
  } from '../../utils/hardwareDevice.ts';

  export let onBack: () => void = () => {};

  let device: NocksterDevice | null = null;
  let connectedHid: HIDDevice | null = null;
  let state: 'disconnected' | 'connecting' | 'connected' = 'disconnected';
  let error = '';
  let productName = '';
  let firmware = '';
  let locked = true;
  let attemptsRemaining: number | null = null;
  let pin = '';
  let busy = false;
  let hasSeed = false;
  let addresses: HardwareAddress[] = [];
  let labels: Record<number, string> = {};
  let lastSync: number | null = null;
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let destroyed = false;
  let showPopoutHint = false;
  let authorizedDevices: NocksterHIDDevice[] = [];
  const hidSupported = typeof navigator !== 'undefined' && !!navigator.hid;
  const isExtensionView = typeof chrome !== 'undefined' && !!chrome.runtime?.id;

  onMount(() => {
    void refreshAuthorizedDevices();
    navigator.hid?.addEventListener('connect', refreshAuthorizedDevices);
    navigator.hid?.addEventListener('disconnect', onHidDisconnect);
    pollTimer = setInterval(() => {
      if (state === 'connected' && !busy) void run(refreshInfo);
    }, 10000);
  });

  onDestroy(() => {
    destroyed = true;
    clearInterval(pollTimer);
    navigator.hid?.removeEventListener('connect', refreshAuthorizedDevices);
    navigator.hid?.removeEventListener('disconnect', onHidDisconnect);
    void disconnect();
  });

  async function run(action: () => Promise<void>) {
    if (busy || destroyed) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (err) {
      if (!destroyed) error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function refreshAuthorizedDevices() {
    try {
      authorizedDevices = ((await navigator.hid?.getDevices()) ?? []).filter(isNockster);
    } catch {
      authorizedDevices = [];
    }
  }

  function onHidDisconnect(event: Event) {
    if ((event as HIDConnectionEvent).device === connectedHid) void disconnect();
    void refreshAuthorizedDevices();
  }

  async function connect() {
    if (!navigator.hid || state !== 'disconnected') return;
    state = 'connecting';
    showPopoutHint = false;
    try {
      // Open the chooser directly from the click when a device choice is needed.
      const choices =
        authorizedDevices.length === 1
          ? authorizedDevices
          : await navigator.hid.requestDevice({ filters: [NOCKSTER_HID_FILTER] });
      if (destroyed) return;
      const hid = choices[0] as NocksterHIDDevice | undefined;
      if (!hid || !isNockster(hid)) {
        showPopoutHint = true;
        throw new Error('Select your Nockster in the device chooser');
      }
      connectedHid = hid;
      productName = hid.productName || 'Nockster';
      const current = new NocksterDevice({ debug: false });
      device = current;
      await current.connectHidDevice(hid);
      if (destroyed || device !== current) {
        await current.disconnect();
        return;
      }
      state = 'connected';
      await refreshInfo();
      await refreshAuthorizedDevices();
    } catch (err) {
      showPopoutHint = true;
      await disconnect();
      throw err;
    }
  }

  async function disconnect() {
    const current = device;
    device = null;
    connectedHid = null;
    state = 'disconnected';
    addresses = [];
    labels = {};
    locked = true;
    hasSeed = false;
    pin = '';
    firmware = '';
    lastSync = null;
    attemptsRemaining = null;
    await current?.disconnect().catch(() => undefined);
  }

  function saveWallet(entry: HardwareAddress, select = false) {
    walletStore.upsertHardwareWallet(
      entry.address,
      labels[entry.slot] || `Nockster · slot ${entry.slot}`,
      { product: productName, transport: 'hid' },
      select
    );
  }

  async function refreshInfo() {
    const current = device;
    if (!current) return;
    const status = await current.getLockStatus();
    if (destroyed || device !== current) return;
    locked = status.locked;
    attemptsRemaining = status.attempts_remaining;
    if (locked) {
      addresses = [];
      labels = {};
      return;
    }
    const info = requireDeviceInfo(await current.getInfo());
    await ensureVaultReady();
    const nextAddresses = hardwareAddresses(info, cheetahPkhB58);
    const seedLabels = info.features & FEATURE_SEED_LABELS ? await current.getSeedLabels() : [];
    if (destroyed || device !== current) return;
    hasSeed = info.has_seed;
    firmware = `${info.fw_major}.${info.fw_minor}`;
    labels = Object.fromEntries(seedLabels.map(entry => [entry.slot, entry.label]));
    const changed = JSON.stringify(nextAddresses) !== JSON.stringify(addresses);
    addresses = nextAddresses;
    if (changed) for (const entry of addresses) saveWallet(entry);
    lastSync = Date.now();
  }

  async function unlock() {
    const current = device;
    const enteredPin = pin;
    pin = '';
    if (!current || !enteredPin.trim()) throw new Error('Enter your PIN to unlock');
    try {
      await current.unlock(enteredPin);
    } finally {
      if (device === current) await refreshInfo();
    }
  }

  async function lock() {
    await device?.lock();
    await refreshInfo();
  }

  async function showAddress(entry: HardwareAddress) {
    const current = device;
    await refreshInfo();
    const live = addresses.find(pub => pub.address === entry.address);
    if (!current || current !== device || locked || !live)
      throw new Error('Reconnect and unlock this wallet');
    await current.showAddress(live.slot, live.path);
  }

  function useWallet(entry: HardwareAddress) {
    saveWallet(entry, true);
    onBack();
  }

  function formatPath(path: number[]) {
    return (
      'm' + path.map(index => `/${index & 0x7fffffff}${index >= 0x80000000 ? "'" : ''}`).join('')
    );
  }

  function openPopout() {
    if (typeof chrome !== 'undefined' && chrome.runtime?.getURL) {
      chrome.tabs.create({ url: chrome.runtime.getURL('dist/index.html'), active: true });
    }
  }
</script>

<div class="hardware-wallet fixed-screen">
  <Header title="Hardware Wallet" showBack={true} on:click={onBack} />
  <div class="content">
    <div class="intro">
      <h2>Connect your Nockster hardware wallet</h2>
      <p>
        Connect and unlock your device to add its wallets. Verify receiving addresses on the device
        screen.
      </p>
    </div>
    {#if !hidSupported}
      <div class="error-message">WebHID requires a Chromium browser in a secure context.</div>
    {:else}
      <div class="status-card">
        <div class="status-row">
          <span>Status</span><span class="status-pill" class:connected={state === 'connected'}
            >{state}</span
          >
        </div>
        <div class="status-row"><span>Device</span><span>{productName || '—'}</span></div>
        <div class="status-row"><span>Firmware</span><span>{firmware || '—'}</span></div>
        {#if state === 'connected'}
          <div class="status-row">
            <span>Lock</span><span>{locked ? 'Locked' : 'Unlocked'}</span>
          </div>
          {#if !locked}<div class="status-row">
              <span>Seed loaded</span><span>{hasSeed ? 'Yes' : 'No'}</span>
            </div>{/if}
        {/if}
      </div>
      {#if error}<div class="error-message">{error}</div>{/if}
      {#if showPopoutHint && isExtensionView}
        <div class="hint-card">
          <div class="hint-text">
            If the device chooser does not appear, open Nockster in a full tab and connect there.
          </div>
          <Button variant="secondary" fullWidth={true} on:click={openPopout}>Open Wallet Tab</Button
          >
        </div>
      {/if}
      <div class="actions">
        {#if state === 'disconnected'}
          <Button variant="primary" fullWidth={true} disabled={busy} on:click={() => run(connect)}
            >Connect Hardware Wallet</Button
          >
        {:else}
          <Button
            variant="secondary"
            fullWidth={true}
            disabled={busy}
            on:click={() => run(disconnect)}>Disconnect</Button
          >
          {#if !locked && state === 'connected'}
            <Button variant="secondary" fullWidth={true} disabled={busy} on:click={() => run(lock)}
              >Lock Device</Button
            >
          {/if}
        {/if}
      </div>
      {#if state === 'connected' && locked}
        <div class="unlock-card">
          <div class="unlock-header">
            <div class="title">Unlock required</div>
            {#if attemptsRemaining !== null}<div class="meta">
                Attempts remaining: {attemptsRemaining}
              </div>{/if}
          </div>
          <div class="unlock-form">
            <input
              type="password"
              placeholder="Enter PIN"
              autocomplete="off"
              bind:value={pin}
              disabled={busy}
            />
            <Button
              variant="primary"
              fullWidth={true}
              disabled={busy || !pin.trim()}
              on:click={() => run(unlock)}>Unlock Device</Button
            >
          </div>
        </div>
      {/if}
      {#if state === 'connected' && !locked}
        <div class="addresses-card">
          <div class="addresses-header">
            <div class="title">Wallets</div>
            <div class="meta">
              {lastSync ? `Synced ${new Date(lastSync).toLocaleTimeString()}` : ''}
            </div>
          </div>
          {#if !addresses.length}<div class="empty">No wallets on this device.</div>{/if}
          <div class="address-list">
            {#each addresses as entry (entry.address)}
              <div class="address-row">
                <div>{labels[entry.slot] || `Wallet ${entry.slot + 1}`}</div>
                <div class="address-text">{entry.address}</div>
                <div class="address-meta">Slot {entry.slot} · {formatPath(entry.path)}</div>
                <Button
                  variant="secondary"
                  fullWidth={true}
                  disabled={busy}
                  on:click={() => run(() => showAddress(entry))}>Verify on Device</Button
                >
                <Button
                  variant="primary"
                  fullWidth={true}
                  disabled={busy}
                  on:click={() => useWallet(entry)}>Use Wallet</Button
                >
              </div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .hardware-wallet {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .intro h2 {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 8px 0;
  }

  .intro p {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 0;
  }

  .status-card,
  .unlock-card,
  .addresses-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .status-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .status-row span:last-child {
    color: var(--color-text);
    font-weight: 500;
  }

  .status-pill {
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 999px;
    background: rgba(148, 163, 184, 0.2);
    color: var(--color-text-secondary);
  }

  .status-pill.connected {
    background: rgba(34, 197, 94, 0.15);
    color: #16a34a;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .error-message {
    padding: 12px 16px;
    background: rgba(220, 38, 38, 0.1);
    border: 1px solid rgba(220, 38, 38, 0.3);
    border-radius: 8px;
    color: #dc2626;
    font-size: 14px;
  }

  .hint-card {
    background: var(--color-surface);
    border: 1px dashed var(--color-border);
    border-radius: 12px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hint-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .hint-text {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  .unlock-header,
  .addresses-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .unlock-header .title,
  .addresses-header .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .unlock-header .meta,
  .addresses-header .meta {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .unlock-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .unlock-form input {
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-background);
    color: var(--color-text);
    font-size: 14px;
    outline: none;
  }

  .unlock-form input:focus {
    border-color: var(--color-text);
  }

  .address-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .address-row {
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--color-background);
    border: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .address-text {
    font-size: 13px;
    color: var(--color-text);
    font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', monospace;
    word-break: break-all;
  }

  .address-meta {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .empty {
    font-size: 13px;
    color: var(--color-text-secondary);
  }
</style>
