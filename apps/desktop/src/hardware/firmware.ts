import { get, writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import {
  FEATURE_SECURE_UPDATE,
  assertPostInstallUpdateBootStatus,
  assertUpdateBundleCompatible,
  assertUpdateFirmwareMatchesBundle,
  fetchLatestUpdateRelease,
  parseUpdateBundleJson,
  type UpdateBundle,
  type UpdateStatus
} from '@swps/nockster-js';
import { hardwareSession } from './session';
import { canReadBootStatus, encryptedBootStatusRelease } from './firmwarePolicy';

const releaseIndex = 'https://bin.aeroe.io/nockster/updates/latest.json';

interface FirmwareState {
  connectionId: number;
  bundle: UpdateBundle | null;
  firmware: Uint8Array | null;
  name: string;
  progress: UpdateStatus | null;
  installing: boolean;
  installed: boolean;
  bootChecked: boolean;
}
const empty = (): FirmwareState => ({
  connectionId: 0,
  bundle: null,
  firmware: null,
  name: '',
  progress: null,
  installing: false,
  installed: false,
  bootChecked: false
});
const state = writable<FirmwareState>(empty());
hardwareSession.subscribe(session => {
  const current = get(state);
  if (session.connectionId !== current.connectionId && !current.installing) state.set(empty());
});

function assertCompatible(bundle: UpdateBundle) {
  const device = get(hardwareSession);
  if (!device.info || !(device.info.features & FEATURE_SECURE_UPDATE)) {
    throw new Error('Connect and unlock a Nockster that supports signed firmware updates');
  }
  assertUpdateBundleCompatible(bundle, {
    releaseVersion: device.release?.release_version,
    buildInfo: device.build,
    protocolV: device.info.proto_v
  });
  if (
    device.security?.flash_encryption !== false &&
    bundle.manifest.release_version < encryptedBootStatusRelease
  ) {
    throw new Error(
      `Flash encryption requires firmware release ${encryptedBootStatusRelease} or newer`
    );
  }
}

async function prepare(bundle: UpdateBundle, firmware: Uint8Array, name: string) {
  assertCompatible(bundle);
  await assertUpdateFirmwareMatchesBundle(bundle, firmware);
  state.set({
    ...empty(),
    bundle,
    firmware,
    name,
    connectionId: get(hardwareSession).connectionId
  });
}

export const firmwareUpdate = {
  subscribe: state.subscribe,
  async latest() {
    await hardwareSession.run('Checking for firmware updates…', async () => {
      state.set(empty());
      const release = await fetchLatestUpdateRelease(releaseIndex, {
        validateBundle: assertCompatible
      });
      await prepare(release.bundle, release.firmware, release.firmwareName);
    });
  },
  async load(bundleFile: File, imageFile: File) {
    await hardwareSession.run('Checking firmware files…', async () => {
      state.set(empty());
      if (bundleFile.size > 65536) throw new Error('The update manifest is too large');
      if (imageFile.size > 16 * 1024 * 1024) throw new Error('The firmware image is too large');
      await prepare(
        parseUpdateBundleJson(await bundleFile.text()),
        new Uint8Array(await imageFile.arrayBuffer()),
        imageFile.name
      );
    });
  },
  async install() {
    const prepared = get(state);
    if (!prepared.bundle || !prepared.firmware)
      throw new Error('Choose a verified firmware update first');
    if (prepared.connectionId !== get(hardwareSession).connectionId)
      throw new Error('Check the update for the connected device before installing');
    const { bundle, firmware } = prepared;
    await hardwareSession.run(
      'Installing firmware. Keep your Nockster connected.',
      async device => {
        assertCompatible(bundle);
        await assertUpdateFirmwareMatchesBundle(bundle, firmware);
        await device.verifyUpdateBundle(bundle);
        const snapshot = get(hardwareSession);
        let wakeLock: WakeLockSentinel | undefined;
        await invoke('hardware_protect_update', { active: true });
        state.update(s => ({ ...s, installing: true, installed: false, bootChecked: false }));
        try {
          wakeLock = await navigator.wakeLock?.request('screen').catch(() => undefined);
          await device.streamUpdateBundle(bundle, firmware, {
            writeFlash: true,
            chunkSize: 256,
            onProgress: progress => state.update(s => ({ ...s, progress }))
          });
          const bootChecked = canReadBootStatus(
            snapshot.info!.features,
            snapshot.release?.release_version ?? null,
            snapshot.security
          );
          if (bootChecked) assertPostInstallUpdateBootStatus(await device.getUpdateBootStatus());
          state.update(s => ({ ...s, installed: true, bootChecked }));
        } finally {
          state.update(s => ({ ...s, installing: false }));
          await wakeLock?.release().catch(() => undefined);
          await invoke('hardware_protect_update', { active: false });
        }
      }
    );
  }
};
