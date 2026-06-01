import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  NocksterDevice,
  Response,
  UpdateBundle,
  UpdateStatus,
  bytesToHex,
  parseUpdateBundleJson,
  FEATURE_SECURITY_STATUS,
  FEATURE_BUILD_INFO,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_UPDATE_BOOT_STATUS,
  MAX_UPDATE_IMAGE_SIZE,
  PROTO_V1,
  formatCheetahPubkey,
} from 'nockster-js';
import type { BuildInfo, SecurityStatus, UpdateBootStatus } from 'nockster-js';
import { mnemonicToSeed, validateMnemonicWords, isValidMnemonicWordCount } from './bip39';
import init, { ParsedTransaction, cheetah_pkh_b58 } from 'nockster-wasm';
import { createSerialTransport  } from './serial';
import { ComposerView } from './composer/Composer';
import './App.css';

const isTauri = typeof window !== 'undefined' && (
  '__TAURI__' in window || 
  '__TAURI_INTERNALS__' in window ||
  window.location.protocol === 'tauri:'
);

export function isSerialSupported(): boolean {
  if (isTauri) return true;
  return 'serial' in navigator;
}

export async function connectSerial(): Promise<SerialPort | string> {
  if (isTauri) {
    const ports = await invoke<string[]>('list_serial_ports');
    if (ports.length === 0) throw new Error('No serial ports found');
    await invoke('connect_serial', { port: ports[0], baudRate: 115200 });
    return ports[0];
  }

  const serial = navigator.serial;
  if (!serial) {
    throw new Error('Web Serial API not available in this browser.');
  }

  const port = await serial.requestPort();
  await port.open({ baudRate: 115200 });
  return port;
}

type DeviceKey = { slot: number; path: number[]; x: bigint[]; y: bigint[] };
type InputDeviceKey = { slot: number; path: number[] };
type InfoResponse = Extract<Response, { type: 'Info' }>;

const UPDATE_HARDWARE_TARGET = 'esp32s3-touch-lcd-1.47';
const UPDATE_BUILD_PROFILE_DEV = 'dev';
const UPDATE_BUILD_PROFILE_CHIP_SECURITY = 'chip-security';
const UPDATE_BUILD_PROFILE_PRODUCTION = 'production';
const UPDATE_SLOT_OTA0 = 1;
const UPDATE_SLOT_OTA1 = 2;
const UPDATE_OTA_STATE_NEW = 0;

function yesNo(value: boolean): string {
  return value ? 'yes' : 'no';
}

function formatSlotMask(mask: number): string {
  const slots: string[] = [];
  for (let slot = 0; slot < 6; slot += 1) {
    if ((mask & (1 << slot)) !== 0) {
      slots.push(String(slot));
    }
  }
  return slots.length ? slots.join(',') : '-';
}

function formatMac(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join(':');
}

function updateSlotName(slot: number): string {
  switch (slot) {
    case 0:
      return 'factory/none';
    case 1:
      return 'ota_0';
    case 2:
      return 'ota_1';
    case 0xff:
      return 'unknown';
    default:
      return 'invalid';
  }
}

function updateOtaStateName(state: number): string {
  switch (state) {
    case 0:
      return 'new';
    case 1:
      return 'pending-verify';
    case 2:
      return 'valid';
    case 3:
      return 'invalid';
    case 4:
      return 'aborted';
    case 0xfd:
      return 'unavailable';
    case 0xfe:
      return 'unknown';
    case 0xff:
      return 'undefined';
    default:
      return 'invalid';
  }
}

function updatePartitionLabel(present: boolean, offset: number, size: number): string {
  if (!present) {
    return 'missing';
  }
  return `0x${offset.toString(16)} · ${size} bytes`;
}

function assertPostInstallBootStatus(status: UpdateBootStatus): void {
  const failures: string[] = [];
  if (!status.partition_table_ok) {
    failures.push('partition table is not readable');
  }
  if (!status.ota_data_present) {
    failures.push('otadata partition is missing');
  }
  if (!status.ota0_present || !status.ota1_present) {
    failures.push('both OTA app slots must be present');
  }
  if (status.current_slot !== UPDATE_SLOT_OTA0 && status.current_slot !== UPDATE_SLOT_OTA1) {
    failures.push(`selected boot slot is ${updateSlotName(status.current_slot)}, expected ota_0 or ota_1`);
  }
  if (status.ota_state !== UPDATE_OTA_STATE_NEW) {
    failures.push(`selected OTA image state is ${updateOtaStateName(status.ota_state)}, expected new`);
  }
  if (failures.length) {
    throw new Error(`post-install activation validation failed: ${failures.join('; ')}`);
  }
}

function artifactNameFromUrl(value: string, fallback: string): string {
  try {
    const path = new URL(value).pathname.split('/').filter(Boolean).pop();
    return path ? decodeURIComponent(path) : fallback;
  } catch {
    return fallback;
  }
}

function fetchHeadersForBearerToken(token: string): Headers {
  const headers = new Headers();
  const trimmed = token.trim();
  if (trimmed) {
    headers.set('authorization', `Bearer ${trimmed}`);
  }
  return headers;
}

function isLocalReleaseHost(url: URL): boolean {
  return url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '::1';
}

function parseReleaseUrl(value: string, label: string): URL {
  try {
    const url = new URL(value);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      throw new Error(`${label} must use http or https`);
    }
    return url;
  } catch (error: any) {
    const message = error?.message ?? error?.toString() ?? 'invalid URL';
    throw new Error(`${label}: ${message}`);
  }
}

function validatePrivateReleaseUrls(bundleUrl: URL, firmwareUrl: URL, bearerToken: string): void {
  if (!bearerToken.trim()) {
    return;
  }
  if (bundleUrl.origin !== firmwareUrl.origin) {
    throw new Error('bearer-token update fetch requires bundle and firmware URLs on the same origin');
  }
  if (
    (bundleUrl.protocol !== 'https:' || firmwareUrl.protocol !== 'https:')
    && (!isLocalReleaseHost(bundleUrl) || !isLocalReleaseHost(firmwareUrl))
  ) {
    throw new Error('bearer-token update fetch requires HTTPS, except for localhost testing');
  }
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff === 0;
}

async function assertFirmwareMatchesBundle(bundle: UpdateBundle, firmware: Uint8Array): Promise<void> {
  if (bundle.manifest.image_size <= 0) {
    throw new Error('bundle image size must be nonzero');
  }
  if (bundle.manifest.image_size > MAX_UPDATE_IMAGE_SIZE) {
    throw new Error(`bundle image size exceeds ${MAX_UPDATE_IMAGE_SIZE} bytes`);
  }
  if (firmware.length !== bundle.manifest.image_size) {
    throw new Error(`firmware size mismatch: bundle expects ${bundle.manifest.image_size}, got ${firmware.length}`);
  }
  if (!globalThis.crypto?.subtle) {
    throw new Error('SHA-256 is unavailable in this browser context');
  }

  const digestInput = new ArrayBuffer(firmware.byteLength);
  new Uint8Array(digestInput).set(firmware);
  const digest = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', digestInput));
  if (!bytesEqual(digest, bundle.manifest.image_sha256)) {
    throw new Error(
      `firmware sha256 mismatch: bundle expects ${bytesToHex(bundle.manifest.image_sha256)}, got ${bytesToHex(digest)}`
    );
  }
}

function buildProfileAllowed(current: string, candidate: string): boolean {
  const supported = [
    UPDATE_BUILD_PROFILE_DEV,
    UPDATE_BUILD_PROFILE_CHIP_SECURITY,
    UPDATE_BUILD_PROFILE_PRODUCTION,
  ];
  if (!supported.includes(current) || !supported.includes(candidate)) {
    return false;
  }
  return current !== UPDATE_BUILD_PROFILE_PRODUCTION || candidate === UPDATE_BUILD_PROFILE_PRODUCTION;
}

function updateCompatibilityBlocker(
  bundle: UpdateBundle | null,
  releaseVersion: number | null,
  buildInfo: BuildInfo | null,
): string | null {
  if (!bundle) {
    return null;
  }
  const manifest = bundle.manifest;

  if (manifest.hardware_target !== UPDATE_HARDWARE_TARGET) {
    return `Bundle target ${manifest.hardware_target} does not match this device target ${UPDATE_HARDWARE_TARGET}.`;
  }
  const protocol = buildInfo?.protocol_v ?? PROTO_V1;
  if (manifest.protocol_v !== protocol) {
    return `Bundle protocol ${manifest.protocol_v} does not match device protocol ${protocol}.`;
  }
  if (manifest.image_size <= 0) {
    return 'Bundle image size must be nonzero.';
  }
  if (manifest.image_size > MAX_UPDATE_IMAGE_SIZE) {
    return `Bundle image size ${manifest.image_size} exceeds ${MAX_UPDATE_IMAGE_SIZE} bytes.`;
  }
  if (releaseVersion !== null && manifest.release_version <= releaseVersion) {
    return `Bundle release ${manifest.release_version} is not newer than device release ${releaseVersion}.`;
  }
  if (buildInfo && !buildProfileAllowed(buildInfo.build_profile, manifest.build_profile)) {
    return `Bundle profile ${manifest.build_profile} is not accepted by device profile ${buildInfo.build_profile}.`;
  }

  return null;
}

function App() {

  const isTauri = typeof window !== 'undefined' && (
    '__TAURI__' in window || 
    '__TAURI_INTERNALS__' in window ||
    window.location.protocol === 'tauri:'
  );
  const [transport] = useState(() => isTauri ? createSerialTransport() : null);
  const [device] = useState(() =>
    transport ? new NocksterDevice(transport) : new NocksterDevice()
  );
  const [connected, setConnected] = useState(false);
  const [status, setStatus] = useState<string>('');
  const [banner, setBanner] = useState<{ open: boolean; message: string } | null>(null);
  const [locked, setLocked] = useState<boolean | null>(null);
  const [attemptsRemaining, setAttemptsRemaining] = useState<number | null>(null);
  const [pin, setPin] = useState('');
  const [info, setInfo] = useState<InfoResponse | null>(null);
  const [deviceKeys, setDeviceKeys] = useState<DeviceKey[]>([]);
  const [selectedSlotState, setSelectedSlotState] = useState<number>(0);
  const selectedSlotRef = useRef(0);
  const setSelectedSlot = (slot: number) => {
    selectedSlotRef.current = slot;
    setSelectedSlotState(slot);
  };
  const selectedSlot = selectedSlotState;
  const [mnemonic, setMnemonic] = useState('');
  const [seedPassphrase, setSeedPassphrase] = useState('');
  const [seedPin, setSeedPin] = useState('');
  const [seeding, setSeeding] = useState(false);
  const [addSeedExpanded, setAddSeedExpanded] = useState(false);
  const [pinResetCurrent, setPinResetCurrent] = useState('');
  const [resettingPin, setResettingPin] = useState(false);
  const [deletingSlot, setDeletingSlot] = useState<number | null>(null);
  const [updateBundle, setUpdateBundle] = useState<UpdateBundle | null>(null);
  const [updateBundleName, setUpdateBundleName] = useState('');
  const [firmwareBytes, setFirmwareBytes] = useState<Uint8Array | null>(null);
  const [firmwareName, setFirmwareName] = useState('');
  const [updateTrustHash, setUpdateTrustHash] = useState<string | null>(null);
  const [firmwareReleaseVersion, setFirmwareReleaseVersion] = useState<number | null>(null);
  const [firmwareBuildInfo, setFirmwareBuildInfo] = useState<BuildInfo | null>(null);
  const [securityStatus, setSecurityStatus] = useState<SecurityStatus | null>(null);
  const [migratingNvs, setMigratingNvs] = useState(false);
  const [updateBootStatus, setUpdateBootStatus] = useState<UpdateBootStatus | null>(null);
  const [updatingFirmware, setUpdatingFirmware] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateStatus | null>(null);
  const [releaseBundleUrl, setReleaseBundleUrl] = useState('');
  const [releaseFirmwareUrl, setReleaseFirmwareUrl] = useState('');
  const [releaseBearerToken, setReleaseBearerToken] = useState('');
  const [fetchingRelease, setFetchingRelease] = useState(false);

  // Transaction signing state
  const [wasmReady, setWasmReady] = useState(false);
  const [tx, setTx] = useState<ParsedTransaction | null>(null);
  const [txInfo, setTxInfo] = useState<any>(null);
  const [txDetails, setTxDetails] = useState<any>(null);
  const [txBytes, setTxBytes] = useState<Uint8Array | null>(null);
  const [signingInputs, setSigningInputs] = useState<any[]>([]);
  const [signing, setSigning] = useState(false);
  const [signedTxBytes, setSignedTxBytes] = useState<Uint8Array | null>(null);
  const [activeTab, setActiveTab] = useState<'device' | 'composer'>('device');

  useEffect(() => {
    const cls = 'app-composer';
    if (activeTab === 'composer') {
      document.body.classList.add(cls);
    } else {
      document.body.classList.remove(cls);
    }
    return () => {
      document.body.classList.remove(cls);
    };
  }, [activeTab]);

  // Initialize WASM module (required for web target)
  useEffect(() => {
    init().then(() => {
      console.log('WASM initialized successfully');
      setWasmReady(true);
    }).catch(err => {
      console.error('Failed to initialize WASM:', err);
      setStatus('WASM initialization failed');
    });
  }, []);

  // Check Web Serial support
  const isSupported = isTauri || NocksterDevice.isSupported();
  const showSeedForm = connected && locked === false;
  const hasSeeds = !!info?.has_seed || deviceKeys.length > 0;
  const isInitialSeed = !hasSeeds;
  const trimmedSeedPin = seedPin.trim();
  const seedPinRequired = isInitialSeed;
  const seedPinReady = !seedPinRequired || trimmedSeedPin.length > 0;
  const trimmedMnemonicValue = mnemonic.trim();
  const wordCount = trimmedMnemonicValue ? trimmedMnemonicValue.split(/\s+/).filter(Boolean).length : 0;
  const wordCountValid = wordCount === 0 || isValidMnemonicWordCount(wordCount);
  const canSubmitSeed = trimmedMnemonicValue.length > 0 && seedPinReady && wordCountValid;
  const slotSummary = Array.from(new Map(deviceKeys.map((pub) => [pub.slot, pub])).values()).sort(
    (a, b) => a.slot - b.slot
  );
  const secureUpdateAvailable = !!info && (info.features & FEATURE_SECURE_UPDATE) !== 0;
  const securityStatusAvailable = !!info && (info.features & FEATURE_SECURITY_STATUS) !== 0;
  const releaseInfoAvailable = !!info && (info.features & FEATURE_RELEASE_INFO) !== 0;
  const buildInfoAvailable = !!info && (info.features & FEATURE_BUILD_INFO) !== 0;
  const updateBootStatusAvailable = !!info && (info.features & FEATURE_UPDATE_BOOT_STATUS) !== 0;
  const nvsMigrationReady =
    securityStatusAvailable &&
    !!securityStatus &&
    securityStatus.nvs_initialized &&
    securityStatus.nvs_schema_version !== 2 &&
    securityStatus.chip_security_available &&
    securityStatus.hmac_user_key_slots !== 0;
  const updateBlockReason = updateCompatibilityBlocker(
    updateBundle,
    firmwareReleaseVersion,
    firmwareBuildInfo,
  );
  const updateBlocked = updateBlockReason !== null;
  const updatePercent = updateProgress && updateProgress.image_size > 0
    ? Math.min(100, Math.round((updateProgress.bytes_received / updateProgress.image_size) * 100))
    : 0;

  useEffect(() => {
    if (isInitialSeed) {
      setAddSeedExpanded(true);
    } else {
      setAddSeedExpanded(false);
      setSeedPin('');
    }
  }, [isInitialSeed]);

  const formatDerivationPath = (path: number[]) => {
    if (!path || path.length === 0) {
      return 'm';
    }
    const parts = path.map((component) => {
      const hardened = (component & 0x80000000) !== 0;
      const index = component & 0x7fffffff;
      return `${index}${hardened ? "'" : ''}`;
    });
    return `m/${parts.join('/')}`;
  };


  const sleep = (ms: number) => new Promise(res => setTimeout(res, ms));

  const [availablePorts, setAvailablePorts] = useState<string[]>([]);
  const [selectedPort, setSelectedPort] = useState<string>('');

  const showPortSelector = async () => {
    console.log('showPortSelector called, isTauri:', isTauri, 'transport:', transport);
    if (isTauri && transport && transport.getAvailablePorts) {
      console.log('Calling getAvailablePorts...');
      const ports = await transport.getAvailablePorts();
      console.log('Got ports:', ports);
      setAvailablePorts(ports);
      if (ports.length > 0) setSelectedPort(ports[0]);
    }
  };

  const connect = async () => {
    try {
      console.log('Connect clicked, isTauri:', isTauri);
      setStatus('Connecting...');
      
      if (isTauri && transport && selectedPort && transport.setSelectedPort) {
        console.log('Setting port:', selectedPort);
        transport.setSelectedPort(selectedPort);
      }
      
      await device.connect();
      setConnected(true);
      setStatus('Connected!');
      await sleep(1000);
      await refreshStatus();
    } catch (error: any) {
      console.error('Connection error:', error);
      setStatus(`Connection failed: ${error.message}`);
    }
  };

  const disconnect = async () => {
    try {
      await device.disconnect();
      setConnected(false);
      setLocked(null);
      setAttemptsRemaining(null);
      setInfo(null);
      setDeviceKeys([]);
      setSelectedSlot(0);
      setUpdateTrustHash(null);
      setFirmwareReleaseVersion(null);
      setFirmwareBuildInfo(null);
      setSecurityStatus(null);
      setUpdateBootStatus(null);
      setUpdateProgress(null);
      setStatus('Disconnected');
    } catch (error: any) {
      setStatus(`Disconnect failed: ${error.message}`);
    }
  };

  const refreshStatus = async (preferSlot?: number, infoOverride?: InfoResponse) => {
    try {
      const lockStatus = await device.getLockStatus();
      setLocked(lockStatus.locked);
      setAttemptsRemaining(lockStatus.attempts_remaining);

      const deviceInfo = infoOverride ?? (await device.getInfo());
      if (deviceInfo.type === 'Info') {
        setInfo(deviceInfo);
        if ((deviceInfo.features & FEATURE_RELEASE_INFO) !== 0) {
          try {
            const release = await device.getReleaseInfo();
            setFirmwareReleaseVersion(Number(release.release_version));
          } catch (err: any) {
            console.warn('getReleaseInfo failed', err);
            setFirmwareReleaseVersion(null);
          }
        } else {
          setFirmwareReleaseVersion(null);
        }
        if ((deviceInfo.features & FEATURE_BUILD_INFO) !== 0) {
          try {
            setFirmwareBuildInfo(await device.getBuildInfo());
          } catch (err: any) {
            console.warn('getBuildInfo failed', err);
            setFirmwareBuildInfo(null);
          }
        } else {
          setFirmwareBuildInfo(null);
        }
        if ((deviceInfo.features & FEATURE_SECURITY_STATUS) !== 0) {
          try {
            setSecurityStatus(await device.getSecurityStatus());
          } catch (err: any) {
            console.warn('getSecurityStatus failed', err);
            setSecurityStatus(null);
          }
        } else {
          setSecurityStatus(null);
        }
        if ((deviceInfo.features & FEATURE_UPDATE_BOOT_STATUS) !== 0) {
          try {
            setUpdateBootStatus(await device.getUpdateBootStatus());
          } catch (err: any) {
            console.warn('getUpdateBootStatus failed', err);
            setUpdateBootStatus(null);
          }
        } else {
          setUpdateBootStatus(null);
        }

        const pubsRaw = Array.isArray(deviceInfo.cheetah_pubs)
          ? deviceInfo.cheetah_pubs
          : [];
        const normalizedKeys = pubsRaw.map((pub) => ({
          slot: Number(pub.slot),
          path: Array.isArray(pub.path) ? pub.path.map((value) => Number(value)) : [],
          x: pub.x,
          y: pub.y,
        }));
        setDeviceKeys(normalizedKeys);

        const slotNumbers = normalizedKeys.map((pub) => pub.slot);
        if (slotNumbers.length === 0) {
          if (selectedSlotRef.current !== 0) {
            setSelectedSlot(0);
          }
          return;
        }

        const currentSlot = selectedSlotRef.current;
        let desiredSlot = preferSlot ?? currentSlot;
        if (!slotNumbers.includes(desiredSlot)) {
          desiredSlot = slotNumbers[0];
        }

        const shouldSelect =
          !lockStatus.locked &&
          slotNumbers.includes(desiredSlot) &&
          (preferSlot !== undefined || desiredSlot !== currentSlot);

        if (shouldSelect) {
          try {
            await device.selectSeed(desiredSlot);
          } catch (err: any) {
            console.warn('selectSeed failed', err);
          }
        }

        if (desiredSlot !== currentSlot) {
          setSelectedSlot(desiredSlot);
        }
      }
    } catch (error: any) {
      setStatus(`Status check failed: ${error.message}`);
    }
  };

  const [pollMs] = useState(2000);
  const refreshingRef = useRef(false);
  const deviceBusyRef = useRef(false);
  const [deviceBusy, setDeviceBusy] = useState(false);

  useEffect(() => {
    if (!connected) return;

    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      if (cancelled) return;
      // Don't poll if device is busy with a long operation
      if (!refreshingRef.current && !deviceBusyRef.current) {
        try {
          refreshingRef.current = true;
          await refreshStatus();
        } finally {
          refreshingRef.current = false;
        }
      }
      if (!cancelled) {
        timer = window.setTimeout(tick, pollMs);
      }
    };

    timer = window.setTimeout(tick, pollMs);
    const onVis = () => {
      if (document.hidden) {
        if (timer) window.clearTimeout(timer);
      } else {
        if (timer) window.clearTimeout(timer);
        timer = window.setTimeout(tick, pollMs);
      }
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
      document.removeEventListener('visibilitychange', onVis);
    };
  }, [connected, pollMs, refreshStatus]);

  const unlock = async () => {
    if (!pin) {
      setStatus('Please enter PIN');
      return;
    }

    try {
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setStatus('Unlocking...');
      await device.unlock(pin);
      setStatus('Unlocked successfully!');
      setPin('');
      await refreshStatus();
    } catch (error: any) {
      setStatus(`Unlock failed: ${error.message}`);
      await refreshStatus(); // Refresh attempts remaining
    } finally {
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const lock = async () => {
    try {
      setStatus('Locking...');
      await device.lock();
      setStatus('Locked successfully!');
      await refreshStatus();
    } catch (error: any) {
      setStatus(`Lock failed: ${error.message}`);
    }
  };

  const resetDevice = async () => {
    if (!connected) {
      return;
    }
    const confirmed = window.confirm(
      'This will erase the seed and PIN from the device. Continue?'
    );
    if (!confirmed) {
      return;
    }
    try {
      setStatus('Resetting device...');
      await device.reset();
      setMnemonic('');
      setSeedPassphrase('');
      setSeedPin('');
      setDeviceKeys([]);
      setSelectedSlot(0);
      await refreshStatus();
      setStatus('Device reset to factory state');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Reset failed: ${message}`);
    }
  };

  const seedDevice = async () => {
    const trimmedMnemonic = mnemonic.trim();
    const trimmedPin = seedPin.trim();
    try {
      if (!showSeedForm) {
        throw new Error('Connect and unlock the device first');
      }
      validateMnemonicWords(trimmedMnemonic);
      if (seedPinRequired && !trimmedPin) {
        throw new Error('Enter a device PIN before seeding');
      }
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setSeeding(true);
      setStatus('Seeding device...');
      const seed = await mnemonicToSeed(trimmedMnemonic, seedPassphrase);
      const prevSlots = deviceKeys.map((key) => key.slot);

      if (isInitialSeed) {
        await device.initializePIN(trimmedPin, seed);
        await refreshStatus(0);
        setStatus('Seed loaded successfully');
      } else {
        await device.addSeed(seed);
        const infoAfter = await device.getInfo();
        if (infoAfter.type !== 'Info') {
          throw new Error('Unexpected response from device after adding seed');
        }
        const pubsAfter = Array.isArray(infoAfter.cheetah_pubs)
          ? infoAfter.cheetah_pubs
          : [];
        const newSlots = pubsAfter.map((pub) => Number(pub.slot));
        const addedSlot = newSlots.find((slot) => !prevSlots.includes(slot));
        await refreshStatus(addedSlot, infoAfter);
        setStatus('Seed slot added successfully');
        setAddSeedExpanded(false);
      }
      setMnemonic('');
      setSeedPassphrase('');
      setSeedPin('');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Seeding failed: ${message}`);
    } finally {
      setSeeding(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const resetPin = async () => {
    const current = pinResetCurrent.trim();

    if (locked !== false) {
      setStatus('Unlock the device before resetting the PIN');
      return;
    }
    if (!current) {
      setStatus('Enter the current PIN');
      return;
    }

    try {
      setResettingPin(true);
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setStatus('Enter the new PIN twice on the device...');
      await device.changePinOnDevice(current);
      setStatus('Device PIN updated successfully');
      setPinResetCurrent('');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`PIN update failed: ${message}`);
    } finally {
      setResettingPin(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const migrateNvsV2 = async () => {
    const current = pinResetCurrent.trim();

    if (!current) {
      setStatus('Enter the current PIN');
      return;
    }
    if (!nvsMigrationReady) {
      setStatus('NVS v2 migration is not available on this device');
      return;
    }
    const confirmed = window.confirm('Rewrite encrypted seed storage to NVS schema v2?');
    if (!confirmed) {
      return;
    }

    try {
      setMigratingNvs(true);
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setStatus('Migrating NVS storage...');
      const nextSecurity = await device.migrateNvsV2(current);
      setSecurityStatus(nextSecurity);
      setPinResetCurrent('');
      await refreshStatus();
      setStatus('NVS storage migrated to schema v2');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`NVS migration failed: ${message}`);
      await refreshStatus();
    } finally {
      setMigratingNvs(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const refreshUpdateTrust = async () => {
    try {
      const trust = await device.getUpdateTrust();
      setUpdateTrustHash(trust.configured ? bytesToHex(trust.pubkey_sha256) : null);
      setStatus(trust.configured ? 'Update trust anchor loaded' : 'No update trust anchor configured');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update trust check failed: ${message}`);
    }
  };

  const refreshUpdateBootStatus = async () => {
    try {
      const bootStatus = await device.getUpdateBootStatus();
      setUpdateBootStatus(bootStatus);
      setStatus('Update boot status refreshed');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update boot status failed: ${message}`);
    }
  };

  const loadUpdateBundle = async (file: File) => {
    try {
      const text = await file.text();
      const bundle = parseUpdateBundleJson(text);
      let existingFirmwareWarning = '';
      if (firmwareBytes) {
        try {
          await assertFirmwareMatchesBundle(bundle, firmwareBytes);
        } catch (error: any) {
          const message = error?.message ?? error?.toString() ?? 'unknown error';
          setFirmwareBytes(null);
          setFirmwareName('');
          existingFirmwareWarning = `; cleared loaded firmware image: ${message}`;
        }
      }
      setUpdateBundle(bundle);
      setUpdateBundleName(file.name);
      setUpdateProgress(null);
      const blocker = updateCompatibilityBlocker(bundle, firmwareReleaseVersion, firmwareBuildInfo);
      if (blocker) {
        setStatus(`Loaded update bundle ${file.name}; ${blocker}${existingFirmwareWarning}`);
      } else {
        setStatus(`Loaded update bundle ${file.name}${existingFirmwareWarning}`);
      }
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Bundle load failed: ${message}`);
      setUpdateBundle(null);
      setUpdateBundleName('');
    }
  };

  const loadFirmwareImage = async (file: File) => {
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (updateBundle) {
        await assertFirmwareMatchesBundle(updateBundle, bytes);
      }
      setFirmwareBytes(bytes);
      setFirmwareName(file.name);
      setUpdateProgress(null);
      setStatus(`Loaded firmware image ${file.name}`);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Firmware load failed: ${message}`);
      setFirmwareBytes(null);
      setFirmwareName('');
    }
  };

  const fetchUpdateRelease = async () => {
    const bundleUrl = releaseBundleUrl.trim();
    const firmwareUrl = releaseFirmwareUrl.trim();
    if (!bundleUrl || !firmwareUrl) {
      setStatus('Enter both release URLs');
      return;
    }

    try {
      const parsedBundleUrl = parseReleaseUrl(bundleUrl, 'bundle URL');
      const parsedFirmwareUrl = parseReleaseUrl(firmwareUrl, 'firmware URL');
      validatePrivateReleaseUrls(parsedBundleUrl, parsedFirmwareUrl, releaseBearerToken);
      setFetchingRelease(true);
      setUpdateProgress(null);
      const headers = fetchHeadersForBearerToken(releaseBearerToken);
      const bundleResp = await fetch(parsedBundleUrl, {
        headers,
        cache: 'no-store',
        credentials: 'omit',
      });
      if (!bundleResp.ok) {
        throw new Error(`bundle fetch failed: HTTP ${bundleResp.status}`);
      }
      const bundle = parseUpdateBundleJson(await bundleResp.text());
      const blocker = updateCompatibilityBlocker(bundle, firmwareReleaseVersion, firmwareBuildInfo);
      if (blocker) {
        throw new Error(blocker);
      }

      const firmwareResp = await fetch(parsedFirmwareUrl, {
        headers,
        cache: 'no-store',
        credentials: 'omit',
      });
      if (!firmwareResp.ok) {
        throw new Error(`firmware fetch failed: HTTP ${firmwareResp.status}`);
      }
      const firmwareLength = firmwareResp.headers.get('content-length');
      if (firmwareLength !== null) {
        const parsedLength = Number(firmwareLength);
        if (!Number.isFinite(parsedLength) || parsedLength !== bundle.manifest.image_size) {
          throw new Error(`firmware size mismatch: bundle expects ${bundle.manifest.image_size}, server reports ${firmwareLength}`);
        }
      }
      const firmware = new Uint8Array(await firmwareResp.arrayBuffer());
      await assertFirmwareMatchesBundle(bundle, firmware);

      setUpdateBundle(bundle);
      setUpdateBundleName(artifactNameFromUrl(parsedBundleUrl.href, 'remote bundle'));
      setFirmwareBytes(firmware);
      setFirmwareName(artifactNameFromUrl(parsedFirmwareUrl.href, 'remote firmware'));
      setReleaseBearerToken('');
      setStatus(`Fetched update release ${bundle.manifest.release_version}`);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Release fetch failed: ${message}`);
    } finally {
      setFetchingRelease(false);
    }
  };

  const verifyUpdateManifest = async () => {
    if (!updateBundle) {
      setStatus('Load an update bundle first');
      return;
    }
    if (updateBlockReason) {
      setStatus(updateBlockReason);
      return;
    }
    try {
      setUpdatingFirmware(true);
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setStatus('Verifying update manifest on device...');
      await device.verifyUpdateBundle(updateBundle);
      setStatus('Device accepted update manifest');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update verify failed: ${message}`);
    } finally {
      setUpdatingFirmware(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const streamUpdate = async (writeFlash: boolean) => {
    if (!updateBundle || !firmwareBytes) {
      setStatus('Load an update bundle and firmware image first');
      return;
    }
    if (updateBlockReason) {
      setStatus(updateBlockReason);
      return;
    }
    try {
      await assertFirmwareMatchesBundle(updateBundle, firmwareBytes);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update image check failed: ${message}`);
      return;
    }
    if (writeFlash) {
      if (!updateBootStatusAvailable) {
        setStatus('Firmware install requires update boot status support');
        return;
      }
      const confirmed = window.confirm(
        'Install this firmware into the inactive OTA slot and activate it for next boot?'
      );
      if (!confirmed) {
        return;
      }
    }

    try {
      setUpdatingFirmware(true);
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setUpdateProgress(null);
      setStatus(writeFlash ? 'Installing firmware update...' : 'Streaming update for verification...');
      const finalStatus = await device.streamUpdateBundle(updateBundle, firmwareBytes, {
        writeFlash,
        onProgress: (progress) => setUpdateProgress(progress),
      });
      if (writeFlash && updateBootStatusAvailable) {
        const bootStatus = await device.getUpdateBootStatus();
        setUpdateBootStatus(bootStatus);
        assertPostInstallBootStatus(bootStatus);
      }
      setStatus(writeFlash
        ? `Firmware installed for next boot (${finalStatus.image_size} bytes verified)`
        : `Firmware image verified on device (${finalStatus.image_size} bytes)`);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update stream failed: ${message}`);
    } finally {
      setUpdatingFirmware(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const deleteSeedSlot = async (slot: number) => {
    if (locked !== false) {
      setStatus('Unlock the device before deleting a seed');
      return;
    }
    const confirmed = window.confirm(
      `Remove seed slot ${slot}? This cannot be undone.`
    );
    if (!confirmed) {
      return;
    }

    try {
      setDeletingSlot(slot);
      setStatus(`Deleting seed slot ${slot}...`);
      await device.deleteSeed(slot);
      await refreshStatus();
      setStatus(`Seed slot ${slot} removed`);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Delete failed: ${message}`);
    } finally {
      setDeletingSlot(null);
    }
  };

  const handleSlotChange = async (slotValue: number) => {
    if (slotValue === selectedSlotRef.current) {
      return;
    }
    try {
      setStatus(`Switching to slot ${slotValue}...`);
      setSelectedSlot(slotValue);
      await refreshStatus(slotValue);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Slot switch failed: ${message}`);
    }
  };

  const ping = async () => {
    try {
      setStatus('Pinging...');
      await device.ping();
      setStatus('Pong! Device is responsive.');
    } catch (error: any) {
      setStatus(`Ping failed: ${error.message}`);
    }
  };

  const loadTransaction = async (file: File) => {
    try {
      if (!wasmReady) {
        setStatus('WASM not ready yet, please wait...');
        return;
      }

      console.log('Loading transaction file:', file.name, file.size, 'bytes');
      setStatus('Loading transaction...');

      const bytes = new Uint8Array(await file.arrayBuffer());
      setTxBytes(bytes);
      console.log('File loaded, bytes:', bytes.length);
      console.log('First 32 bytes:', Array.from(bytes.slice(0, 32)).map(b => b.toString(16).padStart(2, '0')).join(' '));

      console.log('Creating ParsedTransaction...');
      const parsedTx = new ParsedTransaction(bytes);
      console.log('ParsedTransaction created successfully');

      const info = parsedTx.info();
      console.log('Transaction info:', info);

      const details = parsedTx.get_details();
      console.log('Transaction details from WASM:', details);

      // Convert Map to plain object for JSON.stringify
      const convertMapToObject = (obj: any): any => {
        if (obj instanceof Map) {
          const result: any = {};
          obj.forEach((value, key) => {
            result[key] = convertMapToObject(value);
          });
          return result;
        } else if (Array.isArray(obj)) {
          return obj.map(convertMapToObject);
        } else if (obj && typeof obj === 'object') {
          const result: any = {};
          for (const key in obj) {
            result[key] = convertMapToObject(obj[key]);
          }
          return result;
        }
        return obj;
      };

      const detailsObj = convertMapToObject(details);
      console.log('Converted details:', detailsObj);

      setTx(parsedTx);
      setTxInfo(info);
      setTxDetails(detailsObj);
      setSignedTxBytes(null);
      setSigningInputs([]); // Reset signing inputs
      const countLabel = detailsObj?.version === 1 ? 'spends' : 'inputs';
      setStatus(`Loaded transaction: ${info.tx_id} (${info.input_count} ${countLabel})`);

    } catch (error: any) {
      console.error('Transaction load error:', error);
      console.error('Error stack:', error.stack);
      setStatus(`Failed to load transaction: ${error.message || error.toString()}`);
    }
  };

  const signTransaction = async () => {
    if (!tx || !txInfo || !connected || locked) {
      setStatus('Device must be connected and unlocked');
      return;
    }
    if (!txBytes) {
      setStatus('Missing transaction bytes; reload the file');
      return;
    }

    try {
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setSigning(true);
      const txVersion = txDetails?.version ?? 0;
      if (txVersion === 1) {
        setSigningInputs([]);
        setStatus(`Selecting slot ${selectedSlot}...`);
        await device.selectSeed(selectedSlot);

        setStatus('Sending draft to device (approve on-device)...');
        setBanner({ open: true, message: 'Sending draft to device (approve on-device)...' });
        const signedBytes = await device.signDraft(txBytes);

        const signedParsed = new ParsedTransaction(signedBytes);
        const signedInfo = signedParsed.info();
        const signedDetails = signedParsed.get_details();

        const convertMapToObject = (obj: any): any => {
          if (obj instanceof Map) {
            const result: any = {};
            obj.forEach((value, key) => {
              result[key] = convertMapToObject(value);
            });
            return result;
          } else if (Array.isArray(obj)) {
            return obj.map(convertMapToObject);
          } else if (obj && typeof obj === 'object') {
            const result: any = {};
            for (const key in obj) {
              result[key] = convertMapToObject(obj[key]);
            }
            return result;
          }
          return obj;
        };

        setTx(signedParsed);
        setTxInfo(signedInfo);
        setTxDetails(convertMapToObject(signedDetails));
        setTxBytes(signedBytes);
        setSignedTxBytes(signedBytes);

        const filename = `${signedInfo.tx_id.slice(0, 16)}.tx`;
        const ab = new ArrayBuffer(signedBytes.byteLength);
        new Uint8Array(ab).set(signedBytes);
        const txBlob = new Blob([ab], { type: 'application/octet-stream' });
        const url = URL.createObjectURL(txBlob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);

        setStatus(`Signed and downloaded ${filename}`);
        return;
      }

      setStatus('Finding inputs to sign...');

      if (deviceKeys.length === 0) {
        throw new Error('No seed slots available on this device');
      }

      const activeDevicePubkeys = deviceKeys.filter((pub) => pub.slot === selectedSlot);
      if (activeDevicePubkeys.length === 0) {
        throw new Error('Select a seeded slot before signing');
      }

      const inputs = tx.get_signing_inputs(
        activeDevicePubkeys.map((pub) => ({
          slot: pub.slot,
          path: [...pub.path],
          x: [...pub.x],
          y: [...pub.y],
        }))
      );

      // Convert Map objects to plain objects
      const convertMapToObject = (obj: any): any => {
        if (obj instanceof Map) {
          const result: any = {};
          obj.forEach((value, key) => {
            result[key] = convertMapToObject(value);
          });
          return result;
        } else if (Array.isArray(obj)) {
          return obj.map(convertMapToObject);
        } else if (obj && typeof obj === 'object') {
          const result: any = {};
          for (const key in obj) {
            result[key] = convertMapToObject(obj[key]);
          }
          return result;
        }
        return obj;
      };

      const convertedInputs = inputs.map(convertMapToObject);
      setSigningInputs(convertedInputs);

      if (convertedInputs.length === 0) {
        throw new Error('No inputs found to sign with this device');
      }

      const keyId = (slot: number, path: number[]) => `${slot}:${path.join(',')}`;
      const keyMap = new Map<string, DeviceKey>();
      activeDevicePubkeys.forEach((pub) => {
        keyMap.set(keyId(pub.slot, pub.path), pub);
      });

      const signatures: any[] = [];
      for (const input of convertedInputs) {
        const deviceEntries: InputDeviceKey[] = input.device_keys || [];
        if (!deviceEntries.length) {
          continue;
        }

        const msg5BigInts = input.msg5.map((v: string) => BigInt(v));

        for (const entry of deviceEntries) {
          const path = (entry.path ?? []) as number[];
          const slot = Number(entry.slot ?? 0);
          if (slot !== selectedSlot) {
            continue;
          }

          const key = keyMap.get(keyId(slot, path));
          if (!key) {
            throw new Error(`Missing device key for slot ${slot} ${formatDerivationPath(path)}`);
          }

          setStatus(`Signing ${input.input_name} @ slot ${slot} · ${formatDerivationPath(path)}`);

          const sigResp = await device.call({
            type: 'SignSpendHash',
            slot,
            path,
            msg5: msg5BigInts
          });

          if (sigResp.type !== 'OkCheetahSig') {
            throw new Error(`Failed to sign input ${input.input_name}`);
          }

          signatures.push({
            input_name: input.input_name,
            pubkey_x: key.x.map((n: bigint) => n.toString()),
            pubkey_y: key.y.map((n: bigint) => n.toString()),
            chal: Array.from(sigResp.chal).map((n: bigint) => n.toString()),
            sig: Array.from(sigResp.sig).map((n: bigint) => n.toString()),
            slot
          });
        }
      }

      if (signatures.length === 0) {
        throw new Error('No inputs found to sign with available device keys');
      }

      setStatus('Reconstructing transaction with signatures...');
      tx.apply_signatures(signatures);

      const signedBytes = tx.to_bytes();
      const updatedInfo = tx.info();
      setTxInfo(updatedInfo);
      setSignedTxBytes(signedBytes);

      const filename = `${updatedInfo.tx_id.slice(0, 16)}.tx`;
      const ab = new ArrayBuffer(signedBytes.byteLength);
      new Uint8Array(ab).set(signedBytes);
      const txBlob = new Blob([ab], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(txBlob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      setStatus(`Signed ${signatures.length} signature(s) and downloaded ${filename}`);
    } catch (error: any) {
      console.error('Signing error:', error);
      const errorMsg = error.message || error.toString() || 'Unknown error';
      setStatus(`Signing failed: ${errorMsg}`);
    } finally {
      setSigning(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
      setBanner(null);
    }
  };

  const downloadSignedTx = () => {
    if (!signedTxBytes || !txInfo) return;

    const ab = new ArrayBuffer(signedTxBytes.byteLength);
    new Uint8Array(ab).set(signedTxBytes);
    const blob = new Blob([ab], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${txInfo.tx_id.slice(0, 16)}.tx`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const clearTransaction = () => {
    setTx(null);
    setTxInfo(null);
    setTxDetails(null);
    setTxBytes(null);
    setSigningInputs([]);
    setSignedTxBytes(null);
    setStatus('');
  };

  const formatDeviceAddress = (pub: DeviceKey): string => {
    if (wasmReady) {
      try {
        return cheetah_pkh_b58(
          pub.x.map((n) => n.toString()),
          pub.y.map((n) => n.toString())
        );
      } catch {
        // fall back to old (pubkey) encoding
      }
    }
    return formatCheetahPubkey(pub.x, pub.y);
  };

  return (
    <div className={activeTab === 'composer' ? 'container container-wide' : 'container'}>
      <div className={`toast ${banner?.open ? 'toast-open' : ''}`}>
        {banner?.open && (
          <div className="toast-inner">
            <div className="toast-body">
              <div className="toast-title">Device</div>
              <div className="toast-message">{banner.message}</div>
            </div>
            <button
              type="button"
              className="toast-close"
              onClick={() => setBanner((prev) => (prev ? { ...prev, open: false } : prev))}
              aria-label="Close"
            >
              ×
            </button>
          </div>
        )}
      </div>
      <div className="header-img">
        <img src="assets/nockster.png" width="400px"/>
      </div>

      <div className="tab-bar">
        <button
          type="button"
          className={`tab-btn ${activeTab === 'device' ? 'active' : ''}`}
          onClick={() => setActiveTab('device')}
        >
          device
        </button>
        <button
          type="button"
          className={`tab-btn ${activeTab === 'composer' ? 'active' : ''}`}
          onClick={() => setActiveTab('composer')}
        >
          composer
        </button>
      </div>

      {activeTab === 'composer' && (
        <div className="section section-composer">
          <h2>Transaction composer (V1)</h2>
          <p className="seed-subtitle">
            Compose an unsigned V1 transaction noun locally, then download the `.psnt`.
          </p>
          <ComposerView wasmReady={wasmReady} />
        </div>
      )}

      {activeTab === 'device' && !isSupported && (
        <div className="error">
          <p>Web Serial API not supported in this browser.</p>
          <p>Please use Chrome, Edge, or Opera.</p>
        </div>
      )}

      {activeTab === 'device' && connected && (
        <>
          {showSeedForm && (
            <div className="section">
              <div className="seed-header">
                <h2>{isInitialSeed ? 'Load a seed' : 'Add a seed slot'}</h2>
                {!isInitialSeed && (
                  <button
                    type="button"
                    onClick={() => setAddSeedExpanded((prev) => !prev)}
                    className="btn btn-small btn-secondary seed-toggle"
                  >
                    {addSeedExpanded ? 'hide form' : 'add seed'}
                  </button>
                )}
              </div>

              {(isInitialSeed || addSeedExpanded) && (
                <>
                  <p className="seed-subtitle">
                    {isInitialSeed
                      ? 'Device ready to seed. Make sure your keys are written on something that isn\'t a computer!'
                      : 'Add another BIP39 seedphrase to this device. Keep it unlocked; no new PIN required.'}
                  </p>
                  <div className="seed-form">
                    <textarea
                      className="input mnemonic-input"
                      value={mnemonic}
                      onChange={(e) => setMnemonic(e.target.value)}
                      placeholder="twelve or twenty-four words, separated by spaces"
                      spellCheck={false}
                      disabled={deviceBusy || seeding}
                    />
                    {isInitialSeed && (
                      <input
                        type="password"
                        className="input pin-input"
                        value={seedPin}
                        onChange={(e) => setSeedPin(e.target.value)}
                        placeholder="set a device PIN"
                        disabled={deviceBusy || seeding}
                        autoComplete="off"
                      />
                    )}
                    <input
                      type="text"
                      className="input passphrase-input"
                      value={seedPassphrase}
                      onChange={(e) => setSeedPassphrase(e.target.value)}
                      placeholder="optional bip39 passphrase"
                      disabled={deviceBusy || seeding}
                    />
                    <div className="seed-actions">
                      <button
                        type="button"
                        onClick={seedDevice}
                        disabled={deviceBusy || seeding || !canSubmitSeed}
                        className="btn btn-success"
                      >
                        {seeding ? 'seeding...' : isInitialSeed ? 'load seed' : 'add seed'}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setMnemonic('');
                          setSeedPassphrase('');
                          setSeedPin('');
                        }}
                        disabled={deviceBusy || seeding}
                        className="btn btn-secondary"
                      >
                        clear
                      </button>
                    </div>
                    {mnemonic.trim() && !wordCountValid && (
                      <div className="validation-text">
                        Seed words should contain 12, 15, 18, 21, or 24 words (currently {wordCount}).
                      </div>
                    )}
                    {!isInitialSeed && (
                      <div className="seed-hint">
                        Device uses your existing PIN. Unlock it in the control section before adding a seed.
                      </div>
                    )}
                  </div>
                </>
              )}
            </div>
          )}

          {status && (
            <div className="status-message">
              {status}
            </div>
          )}

          <div className="section">
            <h2>Device</h2>
            <div className="status-grid">
              <div className="status-item">
                <span className="label">lock status:</span>
                <span className={`value ${locked ? 'locked' : 'unlocked'}`}>
                  {locked === null ? '...' : locked ? 'locked' : 'unlocked'}
                </span>
              </div>
              <div className="status-item">
                <span className="label">PIN attempts:</span>
                <span className="value">
                  {attemptsRemaining === null ? '...' : attemptsRemaining}
                </span>
              </div>
              {info && (
                <>
                  <div className="status-item">
                    <span className="label">firmware:</span>
                    <span className="value">
                      v{info.fw_major}.{info.fw_minor}
                    </span>
                  </div>
                  {releaseInfoAvailable && (
                    <div className="status-item">
                      <span className="label">release:</span>
                      <span className="value">
                        {firmwareReleaseVersion === null ? '...' : firmwareReleaseVersion}
                      </span>
                    </div>
                  )}
                  <div className="status-item">
                    <span className="label">has seed:</span>
                    <span className="value">{info.has_seed ? 'yes' : 'no'}</span>
                  </div>
                  {securityStatusAvailable && securityStatus && (
                    <>
                      <div className="status-item">
                        <span className="label">nvs:</span>
                        <span className="value">
                          {securityStatus.nvs_initialized
                            ? `schema v${securityStatus.nvs_schema_version} · ${securityStatus.nvs_slot_count} slots`
                            : 'uninitialized'}
                        </span>
                      </div>
                      <div className="status-item">
                        <span className="label">chip security:</span>
                        <span className="value">
                          {securityStatus.chip_security_available ? 'visible' : 'hidden'}
                        </span>
                      </div>
                      {securityStatus.chip_security_available && (
                        <>
                          <div className="status-item full-width">
                            <span className="label">efuse:</span>
                            <span className="value">
                              secure boot {yesNo(securityStatus.secure_boot)} · flash enc {yesNo(securityStatus.flash_encryption)}
                            </span>
                          </div>
                          <div className="status-item full-width">
                            <span className="label">hmac:</span>
                            <span className="value">
                              up {formatSlotMask(securityStatus.hmac_user_key_slots)} · protected {formatSlotMask(securityStatus.read_protected_key_slots)} · {formatMac(securityStatus.mac)}
                            </span>
                          </div>
                        </>
                      )}
                    </>
                  )}
                  {info.has_seed && deviceKeys.length === 0 && (
                    <div className="status-item full-width">
                      <span className="label">public keys:</span>
                      <span className="value">unlock to view</span>
                    </div>
                  )}
                  {deviceKeys.length > 0 && (
                    <>
                      <div className="status-item full-width">
                        <span className="label">active slot:</span>
                        <select
                          value={selectedSlot}
                          onChange={(e) => handleSlotChange(Number(e.target.value))}
                          className="slot-select"
                        >
                          {slotSummary.map((pub) => (
                            <option key={pub.slot} value={pub.slot}>
                              {`slot ${pub.slot} · ${formatDerivationPath(pub.path)}`}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="status-item full-width multi-keys">
                        <span className="label">public keys:</span>
                        <div className="pubkey-list">
                          {slotSummary.map((pub, idx) => (
                            <div key={idx} className="pubkey-list-item">
                              <div className="pubkey-meta">
                                <span className="path-tag">slot {pub.slot} · {formatDerivationPath(pub.path)}</span>
                              </div>
                              <div className="pubkey-display">
                                <span className="pubkey-text">{formatDeviceAddress(pub)}</span>
                                <div className="pubkey-actions">
                                  <button
                                    onClick={() => {
                                      navigator.clipboard.writeText(formatDeviceAddress(pub));
                                      setStatus(
                                        `Copied slot ${pub.slot} ${formatDerivationPath(pub.path)} to clipboard`
                                      );
                                    }}
                                    className="btn btn-small copy-btn"
                                  >
                                    copy
                                  </button>
                                  <button
                                    onClick={() => deleteSeedSlot(pub.slot)}
                                    className="btn btn-small btn-danger"
                                    disabled={deviceBusy || deletingSlot === pub.slot || seeding || signing}
                                  >
                                    {deletingSlot === pub.slot ? 'removing...' : 'remove'}
                                  </button>
                                </div>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    </>
                  )}
                </>
              )}
            </div>
            <button onClick={() => refreshStatus()} disabled={deviceBusy} className="btn btn-small">
              refresh status
            </button>
          </div>



          <div className="section">
            <h2>Control</h2>
            <div className="pin-controls">
              {locked && (
                <input
                  type="password"
                  value={pin}
                  onChange={(e) => setPin(e.target.value)}
                  placeholder="enter PIN"
                  className="input"
                  disabled={deviceBusy}
                  onKeyPress={(e) => {
                    if (e.key === 'Enter' && locked && !deviceBusy) {
                      unlock();
                    }
                  }}
                />
              )}
              <div className="button-group" style={{ gridTemplateColumns: 'repeat(5, 1fr)' }}>
                <button
                  onClick={unlock}
                  disabled={deviceBusy || !locked || !pin}
                  className="btn btn-success"
                >
                  unlock
                </button>
                <button
                  onClick={lock}
                  disabled={deviceBusy || locked || locked === null}
                  className="btn btn-warning"
                >
                  lock
                </button>
                <button onClick={ping} disabled={deviceBusy} className="btn btn-small">
                  test
                </button>
                <button
                  onClick={resetDevice}
                  disabled={deviceBusy || seeding || signing}
                  className="btn btn-danger"
                >
                  reset
                </button>
                <button onClick={disconnect} disabled={deviceBusy} className="btn btn-secondary">
                  disconnect
                </button>
              </div>
            </div>
            {info?.has_seed && (
              <div className="reset-pin">
                <h3>Reset PIN</h3>
                <p className="reset-pin-note">Enter the current PIN here; enter the new PIN twice on the device.</p>
                <div className="reset-pin-grid">
                  <input
                    type="password"
                    className="input"
                    value={pinResetCurrent}
                    onChange={(e) => setPinResetCurrent(e.target.value)}
                    placeholder="current PIN"
                    disabled={deviceBusy || resettingPin}
                    autoComplete="off"
                  />
                </div>
                <div className="reset-pin-actions">
                  <button
                    type="button"
                    onClick={() => {
                      setPinResetCurrent('');
                    }}
                    disabled={deviceBusy || resettingPin}
                    className="btn btn-secondary btn-small"
                  >
                    clear
                  </button>
                  <button
                    type="button"
                    onClick={resetPin}
                    disabled={
                      deviceBusy ||
                      resettingPin ||
                      locked !== false ||
                      !pinResetCurrent.trim()
                    }
                    className="btn btn-success btn-small"
                  >
                    {resettingPin ? 'waiting...' : 'start PIN change'}
                  </button>
                  <button
                    type="button"
                    onClick={migrateNvsV2}
                    disabled={
                      deviceBusy ||
                      migratingNvs ||
                      !pinResetCurrent.trim() ||
                      !nvsMigrationReady
                    }
                    className="btn btn-secondary btn-small"
                  >
                    {migratingNvs ? 'migrating...' : 'migrate nvs v2'}
                  </button>
                </div>
                {securityStatus && securityStatus.nvs_initialized && (
                  <p className="reset-pin-note">
                    NVS schema v{securityStatus.nvs_schema_version}
                    {nvsMigrationReady ? ' · migration available' : ''}
                  </p>
                )}
              </div>
            )}
          </div>

          <div className="section">
            <div className="seed-header">
              <h2>Firmware update</h2>
              <div className="update-header-actions">
                <button
                  type="button"
                  onClick={refreshUpdateTrust}
                  disabled={deviceBusy || !secureUpdateAvailable}
                  className="btn btn-small btn-secondary seed-toggle"
                >
                  check trust
                </button>
                <button
                  type="button"
                  onClick={refreshUpdateBootStatus}
                  disabled={deviceBusy || !updateBootStatusAvailable}
                  className="btn btn-small btn-secondary seed-toggle"
                >
                  check boot
                </button>
              </div>
            </div>
            <div className="update-grid">
              <div className="status-item full-width">
                <span className="label">secure update:</span>
                <span className="value">{secureUpdateAvailable ? 'available' : 'unavailable'}</span>
              </div>
              {releaseInfoAvailable && (
                <div className="status-item full-width">
                  <span className="label">device release:</span>
                  <span className="value">
                    {firmwareReleaseVersion === null ? '...' : firmwareReleaseVersion}
                  </span>
                </div>
              )}
              {buildInfoAvailable && (
                <div className="status-item full-width">
                  <span className="label">device build:</span>
                  <span className="value">
                    {firmwareBuildInfo
                      ? `${firmwareBuildInfo.build_profile} · protocol ${firmwareBuildInfo.protocol_v}`
                      : '...'}
                  </span>
                </div>
              )}
              {updateBootStatusAvailable && (
                <>
                  <div className="status-item full-width">
                    <span className="label">OTA boot:</span>
                    <span className="value update-hash">
                      {updateBootStatus
                        ? `table ${updateBootStatus.partition_table_ok ? 'ok' : 'error'} · otadata ${updateBootStatus.ota_data_present ? 'present' : 'missing'} · current ${updateSlotName(updateBootStatus.current_slot)} · next ${updateSlotName(updateBootStatus.next_slot)} · ${updateOtaStateName(updateBootStatus.ota_state)}`
                        : 'not loaded'}
                    </span>
                  </div>
                  {updateBootStatus && (
                    <div className="status-item full-width">
                      <span className="label">OTA slots:</span>
                      <span className="value update-hash">
                        ota_0 {updatePartitionLabel(updateBootStatus.ota0_present, updateBootStatus.ota0_offset, updateBootStatus.ota0_size)} · ota_1 {updatePartitionLabel(updateBootStatus.ota1_present, updateBootStatus.ota1_offset, updateBootStatus.ota1_size)}
                      </span>
                    </div>
                  )}
                </>
              )}
              <div className="status-item full-width">
                <span className="label">trust anchor:</span>
                <span className="value update-hash">{updateTrustHash ?? 'not loaded'}</span>
              </div>
              {updateBundle && (
                <div className="status-item full-width">
                  <span className="label">bundle:</span>
                  <span className="value update-hash">
                    {updateBundleName} · release {updateBundle.manifest.release_version} · {updateBundle.manifest.build_profile}
                  </span>
                </div>
              )}
              {updateBlockReason && (
                <div className="validation-text full-width">
                  {updateBlockReason}
                </div>
              )}
              {firmwareBytes && (
                <div className="status-item full-width">
                  <span className="label">firmware:</span>
                  <span className="value update-hash">{firmwareName} · {firmwareBytes.length} bytes</span>
                </div>
              )}
            </div>
            <div className="update-files">
              <label className="file-control">
                <span>bundle JSON</span>
                <input
                  type="file"
                  accept=".json,application/json"
                  disabled={deviceBusy || updatingFirmware}
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    if (file) loadUpdateBundle(file);
                  }}
                  className="input"
                />
              </label>
              <label className="file-control">
                <span>firmware bin</span>
                <input
                  type="file"
                  accept=".bin,application/octet-stream"
                  disabled={deviceBusy || updatingFirmware}
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    if (file) loadFirmwareImage(file);
                  }}
                  className="input"
                />
              </label>
            </div>
            <div className="update-remote">
              <label className="file-control update-remote-url">
                <span>bundle URL</span>
                <input
                  type="url"
                  value={releaseBundleUrl}
                  disabled={deviceBusy || updatingFirmware || fetchingRelease}
                  onChange={(e) => setReleaseBundleUrl(e.target.value)}
                  className="input"
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <label className="file-control update-remote-url">
                <span>firmware URL</span>
                <input
                  type="url"
                  value={releaseFirmwareUrl}
                  disabled={deviceBusy || updatingFirmware || fetchingRelease}
                  onChange={(e) => setReleaseFirmwareUrl(e.target.value)}
                  className="input"
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <label className="file-control update-remote-token">
                <span>bearer token</span>
                <input
                  type="password"
                  value={releaseBearerToken}
                  disabled={deviceBusy || updatingFirmware || fetchingRelease}
                  onChange={(e) => setReleaseBearerToken(e.target.value)}
                  className="input"
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
              <button
                type="button"
                onClick={fetchUpdateRelease}
                disabled={deviceBusy || updatingFirmware || fetchingRelease || !releaseBundleUrl.trim() || !releaseFirmwareUrl.trim()}
                className="btn btn-secondary update-remote-fetch"
              >
                {fetchingRelease ? 'fetching...' : 'fetch release'}
              </button>
            </div>
            {updateProgress && (
              <div className="update-progress">
                <div className="update-progress-bar">
                  <div className="update-progress-fill" style={{ width: `${updatePercent}%` }} />
                </div>
                <div className="update-progress-text">
                  {updateProgress.bytes_received} / {updateProgress.image_size} bytes · {updatePercent}%
                </div>
              </div>
            )}
            <div className="button-group update-actions">
              <button
                type="button"
                onClick={verifyUpdateManifest}
                disabled={deviceBusy || updatingFirmware || !secureUpdateAvailable || !updateBundle || updateBlocked}
                className="btn btn-secondary"
              >
                verify manifest
              </button>
              <button
                type="button"
                onClick={() => streamUpdate(false)}
                disabled={deviceBusy || updatingFirmware || !secureUpdateAvailable || !updateBundle || !firmwareBytes || updateBlocked}
                className="btn btn-secondary"
              >
                verify image
              </button>
              <button
                type="button"
                onClick={() => streamUpdate(true)}
                disabled={deviceBusy || updatingFirmware || !secureUpdateAvailable || !updateBootStatusAvailable || !updateBundle || !firmwareBytes || updateBlocked}
                className="btn btn-success"
              >
                {updatingFirmware ? 'working...' : 'install'}
              </button>
            </div>
          </div>

          {wasmReady && info && (
            <div className="section">
              <h2>Transaction signing</h2>

              {!tx ? (
                <div>
                  <p>Upload an unsigned transaction draft (.draft, .wallet, or .psnt) to sign</p>
                  <input
                    type="file"
                    accept=".draft,.wallet,.psnt"
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      if (file) loadTransaction(file);
                    }}
                    className="input"
                  />
                </div>
              ) : (
                <div>
                  <h3>Transaction details</h3>
                  <pre className="tx-details">
                    {JSON.stringify(txDetails || {}, null, 2)}
                  </pre>

                  {signingInputs.length > 0 && (
                    <div className="signing-info">
                      <h4>Inputs to sign:</h4>
                      <div className="input-list">
                        {signingInputs.map((input, i) => (
                          <div key={i} className="input-item">
                            <span className="mono-small">[{input.input_name}]</span>
                            {input.device_keys && input.device_keys.length > 0 && (
                              <span className="path-small">
                                {input.device_keys
                                  .map((entry: InputDeviceKey) =>
                                    `slot ${entry.slot} · ${formatDerivationPath(entry.path ?? [])}`
                                  )
                                  .join(', ')}
                              </span>
                            )}
                            <span className="hash-small">{input.sig_hash}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  <div className="button-group">
                    {!signedTxBytes ? (
                      <>
                        <button
                          onClick={signTransaction}
                          disabled={deviceBusy || signing || locked === true}
                          className="btn btn-success"
                        >
                          {signing ? 'signing...' : 'sign transaction'}
                        </button>
                        <button onClick={clearTransaction} disabled={deviceBusy || signing} className="btn btn-secondary">
                          clear
                        </button>
                      </>
                    ) : (
                      <>
                        <button onClick={downloadSignedTx} disabled={deviceBusy} className="btn btn-success">
                          download signed .tx
                        </button>
                        <button onClick={clearTransaction} disabled={deviceBusy} className="btn btn-secondary">
                          sign another
                        </button>
                      </>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </>
      )}

      {activeTab === 'device' && !connected && (
        <div className="section">
          {isTauri && availablePorts.length === 0 && (
            <div style={{ display: 'flex', justifyContent: 'center' }}>
              
              <button onClick={showPortSelector} className="btn btn-secondary">
                select port
              </button>
            </div>
          )}
          {isTauri && availablePorts.length > 0 && (
            <select value={selectedPort} onChange={(e) => setSelectedPort(e.target.value)} className="input">
              {availablePorts.map(port => (
                <option key={port} value={port}>{port}</option>
              ))}
            </select>
          )}
          <div style={{ display: 'flex', justifyContent: 'center' }}>
            <button onClick={connect} className="btn btn-primary" disabled={isTauri && !selectedPort}>
              connect
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
