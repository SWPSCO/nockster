import {
  NocksterDevice,
  PROTO_V1,
  FEATURE_CHEETAH,
  FEATURE_FRAG,
  getErrorMessage,
  type Response
} from '@swps/nockster-js';

export const NOCKSTER_HID_FILTER = { vendorId: 0x303a, productId: 0x2001 };
export type NocksterHIDDevice = HIDDevice & {
  vendorId?: number;
  productId?: number;
  productName?: string;
};
export type HardwareAddress = { address: string; slot: number; path: number[] };
export type DeviceInfo = Extract<Response, { type: 'Info' }>;
export type PublicKeyAddress = (x: string[], y: string[]) => string;

export function isNockster(device: NocksterHIDDevice): boolean {
  return (
    device.vendorId === NOCKSTER_HID_FILTER.vendorId &&
    device.productId === NOCKSTER_HID_FILTER.productId
  );
}

export function requireDeviceInfo(response: Response): DeviceInfo {
  if (response.type === 'Err') throw new Error(getErrorMessage(response.code));
  if (response.type !== 'Info') throw new Error('Unexpected device information response');
  if (response.proto_v !== PROTO_V1 || !(response.features & FEATURE_CHEETAH)) {
    throw new Error('The device does not support the Nockchain wallet protocol');
  }
  return response;
}

export function hardwareAddresses(
  info: DeviceInfo,
  toAddress: PublicKeyAddress
): HardwareAddress[] {
  const seen = new Set<string>();
  return info.cheetah_pubs
    .map(pub => ({
      address: toAddress(pub.x.map(String), pub.y.map(String)),
      slot: pub.slot,
      path: [...pub.path]
    }))
    .sort((a, b) => a.slot - b.slot)
    .filter(pub => {
      if (seen.has(pub.address)) return false;
      seen.add(pub.address);
      return true;
    });
}

export async function selectSigningWallet(
  device: Pick<NocksterDevice, 'getLockStatus' | 'getInfo' | 'selectSeed'>,
  address: string,
  toAddress: PublicKeyAddress
): Promise<void> {
  if ((await device.getLockStatus()).locked) {
    throw new Error('Unlock your Nockster in Hardware Wallet and try again');
  }
  const info = requireDeviceInfo(await device.getInfo());
  if (!(info.features & FEATURE_FRAG))
    throw new Error('Device does not support transaction drafts');
  const match = hardwareAddresses(info, toAddress).find(
    pub => pub.address === address && !pub.path.length
  );
  if (!match) throw new Error('This Nockster does not contain the signing wallet');
  // Resolve the slot from the current public keys: deleting a seed can move slots.
  await device.selectSeed(match.slot);
}

export async function withSigningDevice<T>(
  address: string,
  toAddress: PublicKeyAddress,
  action: (device: NocksterDevice) => Promise<T>
): Promise<T> {
  if (!navigator.hid) throw new Error('WebHID is unavailable in this context');
  const devices = (await navigator.hid.getDevices()).filter(isNockster);
  if (!devices.length) throw new Error('Open Hardware Wallet and connect your Nockster first');
  let lastError: unknown;
  for (const hid of devices) {
    const device = new NocksterDevice({ debug: false });
    let matched = false;
    let disconnected = false;
    let opening = true;
    let closing: Promise<void> | undefined;
    const close = () => (closing ??= device.disconnect().catch(() => undefined));
    const onDisconnect = (event: Event) => {
      if ((event as HIDConnectionEvent).device !== hid) return;
      disconnected = true;
      if (!opening) void close();
    };
    navigator.hid.addEventListener('disconnect', onDisconnect);
    try {
      await device.connectHidDevice(hid);
      opening = false;
      if (disconnected) throw new Error('Device disconnected');
      await selectSigningWallet(device, address, toAddress);
      matched = true;
      return await action(device);
    } catch (err) {
      // Never retry a signing request (including a rejected approval) on another device.
      if (matched || disconnected) throw err;
      lastError = err;
    } finally {
      navigator.hid.removeEventListener('disconnect', onDisconnect);
      await close();
    }
  }
  throw lastError instanceof Error
    ? lastError
    : new Error('Connect and unlock the Nockster containing this wallet');
}
