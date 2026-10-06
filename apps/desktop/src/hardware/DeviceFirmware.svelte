<script lang="ts">
  import { tick } from 'svelte';
  import { FEATURE_SECURE_UPDATE, FEATURE_DEVICE_REBOOT, bytesToHex } from '@swps/nockster-js';
  import { hardwareSession, perform } from './session';
  import { firmwareUpdate } from './firmware';
  let bundleFile: File | undefined;
  let imageFile: File | undefined;
  let confirm = false;
  let review: HTMLDivElement;
  async function showReview() {
    await tick();
    review?.scrollIntoView({ block: 'start' });
  }
  $: busy = Boolean($hardwareSession.busy);
  $: supported = Boolean(($hardwareSession.info?.features ?? 0) & FEATURE_SECURE_UPDATE);
  $: manifest = $firmwareUpdate.bundle?.manifest;
  $: progress = $firmwareUpdate.progress;
  $: percentage = progress?.image_size
    ? Math.min(100, Math.floor((progress.bytes_received / progress.image_size) * 100))
    : 0;

  async function load() {
    if (!bundleFile || !imageFile)
      throw new Error('Select both the signed manifest and firmware image');
    confirm = false;
    await firmwareUpdate.load(bundleFile, imageFile);
    await showReview();
  }
  async function reboot() {
    await hardwareSession.reboot();
  }
</script>

<section>
  <div class="hw-section-heading">
    <div>
      <h2>Firmware</h2>
      <p>Install signed updates without removing your wallets.</p>
    </div>
  </div>
  <dl class="hw-facts">
    <div>
      <dt>Installed release</dt>
      <dd>{$hardwareSession.release?.release_version ?? 'Not reported'}</dd>
    </div>
    <div>
      <dt>Firmware version</dt>
      <dd>{$hardwareSession.info?.fw_major}.{$hardwareSession.info?.fw_minor}</dd>
    </div>
    <div>
      <dt>Build profile</dt>
      <dd>{$hardwareSession.build?.build_profile ?? 'Not reported'}</dd>
    </div>
  </dl>
  {#if !supported}
    <p class="hw-notice">
      This device does not support in-app firmware updates. Follow the <a
        href="https://my.nockster.com/manual/">device manual</a
      > for installation instructions.
    </p>
  {:else}
    <div class="hw-actions hw-section-actions">
      <button
        class="hw-button hw-primary"
        disabled={busy}
        onclick={() =>
          perform(async () => {
            confirm = false;
            await firmwareUpdate.latest();
            await showReview();
          })}>Check for updates</button
      >
    </div>
    <details class="hw-details">
      <summary>Install from files</summary>
      <form
        class="hw-form"
        onsubmit={event => {
          event.preventDefault();
          void perform(load);
        }}
      >
        <p>Choose the signed update manifest and its matching firmware image.</p>
        <label for="update-manifest">Update manifest (.bundle.json)</label><input
          id="update-manifest"
          type="file"
          accept=".json"
          disabled={busy}
          onchange={event => (bundleFile = event.currentTarget.files?.[0])}
        />
        <label for="update-image">Firmware image (.bin)</label><input
          id="update-image"
          type="file"
          accept=".bin"
          disabled={busy}
          onchange={event => (imageFile = event.currentTarget.files?.[0])}
        />
        <button class="hw-button" disabled={busy || !bundleFile || !imageFile}>Check files</button>
      </form>
    </details>
    {#if manifest}
      <div class="hw-update-review" bind:this={review}>
        <h3>Release {manifest.release_version}</h3>
        <p>{$firmwareUpdate.name}</p>
        <dl class="hw-facts">
          <div>
            <dt>Image size</dt>
            <dd>{(manifest.image_size / 1024 / 1024).toFixed(2)} MB</dd>
          </div>
          <div>
            <dt>Build profile</dt>
            <dd>{manifest.build_profile}</dd>
          </div>
          <div>
            <dt>SHA-256</dt>
            <dd><code>{bytesToHex(manifest.image_sha256)}</code></dd>
          </div>
        </dl>
        {#if $firmwareUpdate.installed}
          <p class="hw-notice" role="status">
            Firmware written and verified. {$firmwareUpdate.bootChecked
              ? 'Boot settings verified.'
              : 'Reconnect after restarting to check the installed release.'}
          </p>
          {#if ($hardwareSession.info?.features ?? 0) & FEATURE_DEVICE_REBOOT}<button
              class="hw-button hw-primary"
              disabled={busy}
              onclick={() => perform(reboot)}>Restart device</button
            >{:else}<p>Unplug and reconnect your Nockster to start the installed firmware.</p>{/if}
        {:else if confirm}
          <div class="hw-confirm">
            <h3>Install release {manifest.release_version}?</h3>
            <p>
              Keep your Nockster connected and the app open until the transfer finishes. The device
              checks the signed manifest and firmware image.
            </p>
            <div class="hw-actions">
              <button
                class="hw-button hw-primary"
                disabled={busy}
                onclick={() =>
                  perform(async () => {
                    confirm = false;
                    await firmwareUpdate.install();
                  })}>Install update</button
              ><button class="hw-link" disabled={busy} onclick={() => (confirm = false)}
                >Cancel</button
              >
            </div>
          </div>
        {:else}<button class="hw-button hw-primary" disabled={busy} onclick={() => (confirm = true)}
            >Review update</button
          >{/if}
      </div>
    {/if}
    {#if progress}
      <div class="hw-transfer" role="status">
        <div>
          <span>{progress.image_verified ? 'Image verified' : 'Firmware transfer'}</span><span
            >{percentage}%</span
          >
        </div>
        <progress max="100" value={percentage} aria-label="Firmware transfer"></progress><small
          >{progress.bytes_received.toLocaleString()} / {progress.image_size.toLocaleString()} bytes</small
        >
      </div>
    {/if}
  {/if}
</section>
