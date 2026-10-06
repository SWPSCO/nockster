import { get, writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import {
  NocksterDevice,
  FEATURE_SEED_LABELS,
  FEATURE_BUILD_INFO,
  FEATURE_RELEASE_INFO,
  FEATURE_SECURITY_STATUS,
  type BuildInfo,
  type ReleaseInfo,
  type SecurityStatus
} from '@swps/nockster-js';
import {
  requireDeviceInfo,
  hardwareAddresses,
  selectSigningWallet,
  type DeviceInfo,
  type HardwareAddress,
  type SigningDeviceProvider
} from '../../../../packages/wallet/src/lib/utils/hardwareDevice';
import { HardwareTransport, type HardwareDeviceDescriptor } from './transport';
import { hardwareCrypto } from './crypto';

interface SessionState {
  connection: 'disconnected' | 'connecting' | 'connected';
  connectionId: number;
  descriptor: HardwareDeviceDescriptor | null;
  busy: string;
  error: string;
  locked: boolean;
  attempts: number | null;
  info: DeviceInfo | null;
  addresses: HardwareAddress[];
  labels: Record<number, string>;
  build: BuildInfo | null;
  release: ReleaseInfo | null;
  security: SecurityStatus | null;
}

const empty = (): SessionState => ({
  connection: 'disconnected',
  connectionId: 0,
  descriptor: null,
  busy: '',
  error: '',
  locked: true,
  attempts: null,
  info: null,
  addresses: [],
  labels: {},
  build: null,
  release: null,
  security: null
});
const state = writable<SessionState>(empty());
let device: NocksterDevice | null = null;
let operation = 0;

function update(patch: Partial<SessionState>) {
  state.update(current => ({ ...current, ...patch }));
}

async function disconnect(error = '') {
  const current = device;
  device = null;
  operation++;
  state.set({ ...empty(), error });
  await current?.disconnect().catch(() => undefined);
}

async function refresh(current: NocksterDevice) {
  const status = await current.getLockStatus();
  if (device !== current) throw new Error('Device disconnected');
  update({ locked: status.locked, attempts: status.attempts_remaining });
  if (status.locked) {
    update({ info: null, addresses: [], labels: {}, build: null, release: null, security: null });
    return;
  }
  const info = requireDeviceInfo(await current.getInfo());
  const crypto = await hardwareCrypto();
  const labels = info.features & FEATURE_SEED_LABELS ? await current.getSeedLabels() : [];
  const build = info.features & FEATURE_BUILD_INFO ? await current.getBuildInfo() : null;
  const release = info.features & FEATURE_RELEASE_INFO ? await current.getReleaseInfo() : null;
  const security =
    info.features & FEATURE_SECURITY_STATUS ? await current.getSecurityStatus() : null;
  if (device !== current) throw new Error('Device disconnected');
  update({
    info,
    labels: Object.fromEntries(labels.map(entry => [entry.slot, entry.label])),
    addresses: hardwareAddresses(info, crypto.cheetah_pkh_b58),
    build,
    release,
    security
  });
}

async function run<T>(
  label: string,
  action: (current: NocksterDevice) => Promise<T>,
  allowDisconnect = false
): Promise<T> {
  const snapshot = get(state);
  if (snapshot.busy) throw new Error(`Finish the current operation: ${snapshot.busy}`);
  const current = device;
  if (!current || snapshot.connection !== 'connected')
    throw new Error('Connect your Nockster in Hardware first');
  const id = ++operation;
  update({ busy: label, error: '' });
  try {
    const result = await action(current);
    if (device !== current && !allowDisconnect) throw new Error('Device disconnected');
    return result;
  } catch (error) {
    if (id === operation) update({ error: error instanceof Error ? error.message : String(error) });
    throw error;
  } finally {
    if (id === operation) update({ busy: '' });
  }
}

async function connect(descriptor: HardwareDeviceDescriptor) {
  if (get(state).connection !== 'disconnected')
    throw new Error('Disconnect the current device first');
  const id = ++operation;
  update({
    connection: 'connecting',
    connectionId: id,
    descriptor,
    busy: 'Connecting to Nockster…',
    error: ''
  });
  const transport = new HardwareTransport(descriptor.id, error => {
    if (device === current) void disconnect(error.message);
  });
  const current = new NocksterDevice(transport);
  device = current;
  try {
    await current.connect();
    if (device !== current) throw new Error('Connection cancelled');
    update({ connection: 'connected' });
    await refresh(current);
  } catch (error) {
    if (device === current)
      await disconnect(error instanceof Error ? error.message : String(error));
    throw error;
  } finally {
    if (id === operation) update({ busy: '' });
  }
}

const signingProvider: SigningDeviceProvider = (address, toAddress, action) =>
  run('Review and approve the transaction on your Nockster', async current => {
    await selectSigningWallet(current, address, toAddress);
    return action(current);
  });

export const hardwareSession = {
  subscribe: state.subscribe,
  connect,
  disconnect,
  run,
  refresh,
  signingProvider,
  list: () => invoke<HardwareDeviceDescriptor[]>('hardware_devices'),
  clearError: () => update({ error: '' }),
  showError: (error: unknown) =>
    update({ error: error instanceof Error ? error.message : String(error) }),
  reboot: async () => {
    await run('Restarting your Nockster…', current => current.reboot(), true);
    await disconnect();
  },
  refreshInfo: () => run('Reading device information…', refresh),
  unlock: (pin: string) =>
    run('Unlocking device…', async current => {
      try {
        await current.unlock(pin);
      } finally {
        if (device === current) await refresh(current);
      }
    }),
  lock: () =>
    run('Locking device…', async current => {
      await current.lock();
      await refresh(current);
    })
};

export async function perform(action: () => Promise<unknown>) {
  hardwareSession.clearError();
  try {
    await action();
  } catch (error) {
    hardwareSession.showError(error);
  }
}
