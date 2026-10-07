<script lang="ts">
  import {
    FEATURE_PIN_CHANGE_UI,
    FEATURE_DEVICE_REBOOT,
    FEATURE_TOUCH_CALIBRATION_UI,
    FEATURE_TOUCH_DIAGNOSTICS,
    FEATURE_SECURE_UPDATE,
    getErrorMessage,
    bytesToHex,
    type Request,
    type Response
  } from '@swps/nockster-js';
  import { hardwareSession, perform } from './session';
  import DeviceReset from './DeviceReset.svelte';
  let pin = '';
  let changingPin = false;
  let trust = '';
  let notice = '';
  let diagnostics = false;
  $: busy = Boolean($hardwareSession.busy);
  $: features = $hardwareSession.info?.features ?? 0;
  $: security = $hardwareSession.security;
  function requireOk(response: Response) {
    if (response.type === 'Err') throw new Error(getErrorMessage(response.code));
    if (response.type !== 'Ok') throw new Error('Unexpected device response');
  }
  async function request(request: Request, label: string) {
    await hardwareSession.run(label, async current =>
      requireOk(await current.call(request, 180000))
    );
  }
  async function changePin() {
    const entered = pin;
    pin = '';
    await hardwareSession.run('Choose and confirm your new PIN on the Nockster screen', current =>
      current.changePinOnDevice(entered)
    );
    changingPin = false;
    notice = 'Device PIN changed.';
  }
  async function reboot() {
    await hardwareSession.reboot();
  }
  async function readTrust() {
    await hardwareSession.run('Reading update trust key…', async current => {
      const value = await current.getUpdateTrust();
      trust = value.configured
        ? bytesToHex(value.pubkey_sha256)
        : 'No update signing key configured';
    });
  }
</script>

<section>
  <div class="hw-section-heading">
    <div>
      <h2>Device security</h2>
      <p>Review the settings reported by your Nockster.</p>
    </div>
    <button class="hw-button" disabled={busy} onclick={() => perform(hardwareSession.refreshInfo)}
      >Refresh</button
    >
  </div>
  {#if notice}<p class="hw-notice" role="status">{notice}</p>{/if}
  {#if security}
    <dl class="hw-facts">
      <div>
        <dt>Secure boot</dt>
        <dd>{security.secure_boot ? 'Enabled' : 'Disabled'}</dd>
      </div>
      <div>
        <dt>Flash encryption</dt>
        <dd>{security.flash_encryption ? 'Enabled' : 'Disabled'}</dd>
      </div>
      <div>
        <dt>Hardware JTAG</dt>
        <dd>{security.pad_jtag_disabled ? 'Disabled' : 'Enabled'}</dd>
      </div>
      <div>
        <dt>USB JTAG</dt>
        <dd>{security.usb_jtag_disabled ? 'Disabled' : 'Enabled'}</dd>
      </div>
      <div>
        <dt>Download mode</dt>
        <dd>{security.download_mode_disabled ? 'Disabled' : 'Enabled'}</dd>
      </div>
      <div>
        <dt>HMAC key slots</dt>
        <dd>{security.hmac_key_slots}</dd>
      </div>
      <div>
        <dt>Device storage</dt>
        <dd>{security.nvs_initialized ? 'Initialized' : 'Not initialized'}</dd>
      </div>
    </dl>
    <details class="hw-details">
      <summary>Hardware details</summary>
      <pre>{JSON.stringify(
          security,
          (_key, value) => (value instanceof Uint8Array ? Array.from(value) : value),
          2
        )}</pre>
    </details>
  {:else}<p class="hw-notice">This device does not report chip security settings.</p>{/if}
  <div class="hw-section">
    <h3>PIN & screen</h3>
    <p>Your new PIN is entered on the device.</p>
    <div class="hw-actions">
      {#if features & FEATURE_PIN_CHANGE_UI}<button
          class="hw-button"
          disabled={busy}
          onclick={() => (changingPin = !changingPin)}>Change device PIN</button
        >{/if}
      {#if features & FEATURE_TOUCH_CALIBRATION_UI}<button
          class="hw-button"
          disabled={busy}
          onclick={() =>
            perform(() =>
              request(
                { type: 'StartTouchCalibration' },
                'Follow the calibration points on your Nockster'
              )
            )}>Calibrate touchscreen</button
        >{/if}
      {#if features & FEATURE_TOUCH_DIAGNOSTICS}<button
          class="hw-button"
          disabled={busy}
          onclick={() =>
            perform(async () => {
              await request(
                { type: 'ShowTouchDiagnostics', enabled: !diagnostics },
                'Updating touchscreen diagnostics…'
              );
              diagnostics = !diagnostics;
            })}>{diagnostics ? 'Stop touchscreen test' : 'Test touchscreen'}</button
        >{/if}
    </div>
    {#if changingPin}<form
        class="hw-form"
        onsubmit={event => {
          event.preventDefault();
          void perform(changePin);
        }}
      >
        <label for="current-device-pin">Current device PIN</label><input
          id="current-device-pin"
          type="password"
          autocomplete="off"
          bind:value={pin}
          disabled={busy}
        />
        <div class="hw-actions">
          <button class="hw-button hw-primary" disabled={busy || !pin}>Continue on device</button
          ><button
            type="button"
            class="hw-link"
            disabled={busy}
            onclick={() => {
              pin = '';
              changingPin = false;
            }}>Cancel</button
          >
        </div>
      </form>{/if}
  </div>
  <div class="hw-section">
    <h3>Build information</h3>
    <dl class="hw-facts">
      <div>
        <dt>Commit</dt>
        <dd><code>{$hardwareSession.build?.git_commit || 'Not reported'}</code></dd>
      </div>
      <div>
        <dt>Protocol</dt>
        <dd>{$hardwareSession.info?.proto_v}</dd>
      </div>
      <div>
        <dt>Transaction types</dt>
        <dd><code>{$hardwareSession.build?.tx_types_rev || 'Not reported'}</code></dd>
      </div>
    </dl>
    {#if features & FEATURE_SECURE_UPDATE}<button
        class="hw-link"
        disabled={busy}
        onclick={() => perform(readTrust)}>Show update signing key fingerprint</button
      >{/if}
    {#if trust}<code class="hw-address hw-result">{trust}</code>{/if}
  </div>
  <div class="hw-section">
    <h3>Device controls</h3>
    <div class="hw-actions">
      <button
        class="hw-button"
        disabled={busy}
        onclick={() =>
          perform(async () => {
            await hardwareSession.run('Checking device connection…', current => current.ping());
            notice = 'Your Nockster responded.';
          })}>Test connection</button
      >
      {#if features & FEATURE_DEVICE_REBOOT}<button
          class="hw-button"
          disabled={busy}
          onclick={() => perform(reboot)}>Restart device</button
        >{/if}
    </div>
    <DeviceReset />
  </div>
</section>
