<script lang="ts">
  import { onDestroy } from 'svelte';
  import { hardwareSession, perform } from './session';
  import { hardwareCrypto } from './crypto';
  import { parseHex, readFile } from './inputs';
  import DeviceContacts from './DeviceContacts.svelte';
  import DevicePreimages from './DevicePreimages.svelte';

  export let offline = false;
  let tool = offline ? 'Sign & verify' : 'Device contacts';
  let mode = 'message';
  let selectedAddress = '';
  let payload = '';
  let publicKey = '';
  let challenge = '';
  let signature = '';
  let verification = '';
  let source = '';
  let threshold = 2;
  let count = 3;
  let shares: string[] = [];
  let restoreInput = '';
  let confirmRestore = false;
  let backupNotice = '';
  let noun = '';
  let nounName = '';
  let destroyed = false;
  $: busy = Boolean($hardwareSession.busy);
  $: connected = $hardwareSession.connection === 'connected' && !$hardwareSession.locked;
  $: roots = $hardwareSession.addresses.filter(entry => !entry.path.length);
  $: if (roots.length && !roots.some(entry => entry.address === selectedAddress))
    selectedAddress = roots[0].address;
  const hex = (limbs: bigint[]) => limbs.map(limb => limb.toString(16).padStart(16, '0')).join('');

  async function verify() {
    verification = '';
    const crypto = await hardwareCrypto();
    const valid =
      mode === 'message'
        ? crypto.verify_message(
            publicKey.trim(),
            new TextEncoder().encode(payload),
            challenge.trim(),
            signature.trim()
          )
        : crypto.verify_hash(publicKey.trim(), payload.trim(), challenge.trim(), signature.trim());
    verification = valid ? 'Valid signature' : 'Invalid signature';
  }
  async function sign() {
    verification = '';
    const address = selectedAddress;
    const input = payload;
    const signingMode = mode;
    await hardwareSession.run(
      'Review and approve the message or hash on your Nockster',
      async current => {
        await hardwareSession.refresh(current);
        const entry = $hardwareSession.addresses.find(
          item => item.address === address && !item.path.length
        );
        const pub = $hardwareSession.info?.cheetah_pubs.find(
          item => item.slot === entry?.slot && !item.path.length
        );
        if (!entry || !pub) throw new Error('Select a wallet on the connected device');
        const crypto = await hardwareCrypto();
        const result =
          signingMode === 'message'
            ? await current.signMessage(entry.slot, entry.path, new TextEncoder().encode(input))
            : await current.signHash(
                entry.slot,
                entry.path,
                Array.from(crypto.hash_b58_to_limbs(input.trim()))
              );
        if (destroyed) return;
        publicKey = crypto.cheetah_point_b58(pub.x.map(String), pub.y.map(String));
        challenge = hex(result.chal);
        signature = hex(result.sig);
        mode = signingMode;
        payload = input;
        await verify();
        if (verification !== 'Valid signature')
          throw new Error('The returned signature failed verification');
      }
    );
  }
  async function loadSignature(file: File) {
    verification = '';
    const crypto = await hardwareCrypto();
    const parts = crypto.sig_file_to_hex(await readFile(file, 65536)) as {
      chal_hex: string;
      sig_hex: string;
    };
    challenge = parts.chal_hex;
    signature = parts.sig_hex;
  }
  async function split() {
    shares = [];
    backupNotice = '';
    if (
      !Number.isInteger(threshold) ||
      !Number.isInteger(count) ||
      threshold < 2 ||
      count < threshold ||
      count > 16
    )
      throw new Error('Choose 2–16 shares and a threshold between 2 and the total');
    const crypto = await hardwareCrypto();
    let coil: Uint8Array | undefined;
    let randomness: Uint8Array | undefined;
    try {
      const input = source.trim();
      if (input.startsWith('zprv')) {
        const info = crypto.extended_key_info(input) as { protocol_version: number };
        if (info.protocol_version !== 1)
          throw new Error('Use a Nockchain protocol v1 extended private key');
        coil = crypto.zprv_to_coil(input);
      } else coil = parseHex(input);
      if (coil.length !== 64) throw new Error('Enter a zprv key or a 64-byte master coil in hex');
      randomness = globalThis.crypto.getRandomValues(new Uint8Array(64 * (threshold - 1)));
      shares = crypto.shamir_split_coil(coil, threshold, count, randomness) as string[];
      backupNotice = `Any ${threshold} of these ${count} shares can restore this wallet. Keep them separately.`;
    } finally {
      coil?.fill(0);
      randomness?.fill(0);
      source = '';
    }
  }
  async function restore() {
    const input = restoreInput.trim().split(/\s+/).filter(Boolean);
    const crypto = await hardwareCrypto();
    let coil: Uint8Array | undefined;
    try {
      coil = crypto.shamir_combine_shares(input);
      await hardwareSession.run(
        'Importing the restored wallet into your Nockster',
        async current => {
          await hardwareSession.refresh(current);
          if (!$hardwareSession.info?.has_seed)
            throw new Error('Set up a device PIN and first wallet before restoring a master coil');
          await current.addCoil(coil!);
          await hardwareSession.refresh(current);
        }
      );
      backupNotice = 'Restored wallet added to your Nockster.';
    } finally {
      coil?.fill(0);
      restoreInput = '';
      confirmRestore = false;
    }
  }
  function clearBackup() {
    shares = [];
    source = '';
    restoreInput = '';
    backupNotice = '';
    confirmRestore = false;
  }
  async function inspect(file: File) {
    noun = '';
    nounName = '';
    const crypto = await hardwareCrypto();
    noun = JSON.stringify(crypto.inspect_noun(await readFile(file)), null, 2);
    nounName = file.name;
  }
  onDestroy(() => {
    destroyed = true;
    clearBackup();
    payload = '';
    noun = '';
  });
</script>

<section>
  <div class="hw-section-heading hw-tool-heading">
    <h2>Tools</h2>
    <select
      id="hardware-tool"
      aria-label="Tool"
      bind:value={tool}
      disabled={busy}
      onchange={() => {
        verification = '';
        clearBackup();
      }}
    >
      <option disabled={!connected}>Device contacts</option><option disabled={!connected}
        >Preimage vault</option
      ><option>Sign & verify</option><option>Split & restore</option><option>Inspect a noun</option>
    </select>
  </div>
  {#if tool === 'Device contacts'}<DeviceContacts />
  {:else if tool === 'Preimage vault'}<DevicePreimages />
  {:else if tool === 'Sign & verify'}
    <div class="hw-form">
      <h3>Sign & verify</h3>
      <p>Signing requires approval on your device. Verification runs locally.</p>
      <label for="signature-kind">Payload</label><select
        id="signature-kind"
        bind:value={mode}
        disabled={busy}
        onchange={() => (verification = '')}
        ><option value="message">Message</option><option value="hash">Tip5 hash (base58)</option
        ></select
      >
      <label for="signature-payload"
        >{mode === 'message' ? 'Message text' : 'Hash to sign or verify'}</label
      ><textarea
        id="signature-payload"
        bind:value={payload}
        rows="3"
        spellcheck="false"
        disabled={busy}
        oninput={() => (verification = '')}
      ></textarea>
      {#if connected}<label for="signing-wallet">Signing wallet</label><select
          id="signing-wallet"
          bind:value={selectedAddress}
          disabled={busy}
          >{#each roots as entry}<option value={entry.address}
              >{$hardwareSession.labels[entry.slot] || `Wallet ${entry.slot + 1}`}</option
            >{/each}</select
        ><button
          class="hw-button hw-primary"
          disabled={busy || !selectedAddress || !payload}
          onclick={() => perform(sign)}>Sign on device</button
        >{/if}
      <div class="hw-section">
        <h3>Verify a signature</h3>
        <label class="hw-file-label"
          >Load a .sig file<input
            type="file"
            accept=".sig"
            disabled={busy}
            onchange={event => {
              const file = event.currentTarget.files?.[0];
              if (file) void perform(() => loadSignature(file));
              event.currentTarget.value = '';
            }}
          /></label
        >
        <label for="signature-public-key">Signer public key (base58)</label><input
          id="signature-public-key"
          bind:value={publicKey}
          spellcheck="false"
          disabled={busy}
          oninput={() => (verification = '')}
        />
        <label for="signature-challenge">Challenge (hex)</label><input
          id="signature-challenge"
          bind:value={challenge}
          spellcheck="false"
          disabled={busy}
          oninput={() => (verification = '')}
        />
        <label for="signature-value">Signature (hex)</label><input
          id="signature-value"
          bind:value={signature}
          spellcheck="false"
          disabled={busy}
          oninput={() => (verification = '')}
        />
        <button
          class="hw-button"
          disabled={busy || !publicKey || !challenge || !signature || !payload}
          onclick={() => perform(verify)}>Verify signature</button
        >
        {#if verification}<p class="hw-notice" role="status">{verification}</p>{/if}
      </div>
    </div>
  {:else if tool === 'Split & restore'}
    <div class="hw-form">
      <h3>Split a wallet backup</h3>
      <p>
        Create Shamir shares from a private key you already hold. The device’s private keys cannot
        be exported here.
      </p>
      <label for="backup-source">zprv private key or master coil (hex)</label><textarea
        id="backup-source"
        bind:value={source}
        rows="3"
        spellcheck="false"
        autocomplete="off"
        disabled={busy}
      ></textarea>
      <div class="hw-columns">
        <div>
          <label for="backup-threshold">Shares needed to restore</label><input
            id="backup-threshold"
            type="number"
            min="2"
            max="16"
            bind:value={threshold}
            disabled={busy}
          />
        </div>
        <div>
          <label for="backup-total">Total shares</label><input
            id="backup-total"
            type="number"
            min="2"
            max="16"
            bind:value={count}
            disabled={busy}
          />
        </div>
      </div>
      <button class="hw-button" disabled={busy || !source.trim()} onclick={() => perform(split)}
        >Create backup shares</button
      >
      {#if backupNotice}<p class="hw-notice" role="status">{backupNotice}</p>{/if}
      {#if shares.length}<div class="hw-shares">
          {#each shares as share, index}<label for={`backup-share-${index}`}
              >Share {index + 1}</label
            ><textarea
              id={`backup-share-${index}`}
              readonly
              value={share}
              rows="3"
              spellcheck="false"
            ></textarea>{/each}<button class="hw-button" onclick={clearBackup}>Clear shares</button>
        </div>{/if}
    </div>
    <div class="hw-form hw-section">
      <h3>Restore from shares</h3>
      <p>Combine shares and import the recovered wallet into your connected Nockster.</p>
      <label for="restore-shares">Backup shares, one per line</label><textarea
        id="restore-shares"
        bind:value={restoreInput}
        rows="4"
        spellcheck="false"
        autocomplete="off"
        disabled={busy}
      ></textarea>
      {#if confirmRestore}<div class="hw-confirm">
          <p>Import this recovered private key as a new device wallet?</p>
          <div class="hw-actions">
            <button
              class="hw-button hw-primary"
              disabled={busy || !connected}
              onclick={() => perform(restore)}>Restore to device</button
            ><button class="hw-link" disabled={busy} onclick={() => (confirmRestore = false)}
              >Cancel</button
            >
          </div>
        </div>{:else}<button
          class="hw-button"
          disabled={busy || !connected || !restoreInput.trim()}
          onclick={() => (confirmRestore = true)}>Review restore</button
        >{/if}
      {#if !connected}<p class="hw-muted">
          Connect and unlock your device to restore a wallet.
        </p>{/if}
    </div>
  {:else}
    <div class="hw-form">
      <h3>Inspect a noun</h3>
      <p>Decode a jammed noun locally to inspect its structure.</p>
      <label for="noun-file">Jam file</label><input
        id="noun-file"
        type="file"
        accept=".jam,.psnt,.export,.sig"
        disabled={busy}
        onchange={event => {
          const file = event.currentTarget.files?.[0];
          if (file) void perform(() => inspect(file));
          event.currentTarget.value = '';
        }}
      />
      {#if noun}<h3>{nounName}</h3>
        <pre class="hw-noun">{noun}</pre>
        <button
          class="hw-button"
          onclick={() => {
            noun = '';
            nounName = '';
          }}>Clear inspection</button
        >{/if}
    </div>
  {/if}
</section>
