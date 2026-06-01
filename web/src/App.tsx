import { Suspense, lazy, useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  NocksterDevice,
  Response,
  UpdateBundle,
  UpdateStatus,
  bytesToHex,
  parseUpdateBundleJson,
  assertUpdateFirmwareMatchesBundle,
  getUpdateBundleCompatibilityBlocker,
  assertPostInstallUpdateBootStatus,
  updateSlotName,
  updateOtaStateName,
  fetchUpdateReleaseArtifacts,
  fetchLatestUpdateRelease as fetchLatestUpdateReleaseFromIndex,
  FEATURE_SECURITY_STATUS,
  FEATURE_BUILD_INFO,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_UPDATE_BOOT_STATUS,
  FEATURE_DEVICE_REBOOT,
  formatCheetahPubkey,
} from 'nockster-js';
import type { BuildInfo, FetchedUpdateRelease, SecurityStatus, UpdateBootStatus } from 'nockster-js';
import { mnemonicToSeed, validateMnemonicWords, isValidMnemonicWordCount } from './bip39';
import { createSerialTransport } from './serial';
import './App.css';

const ComposerView = lazy(() =>
  import('./composer/Composer').then((module) => ({ default: module.ComposerView }))
);

type NocksterWasm = typeof import('nockster-wasm');
type ParsedTransactionInstance = InstanceType<NocksterWasm['ParsedTransaction']>;

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
type InfoResponse = Extract<Response, { type: 'Info' }>;
type DeviceStatusSnapshot = {
  info: InfoResponse | null;
  releaseVersion: number | null;
  buildInfo: BuildInfo | null;
  updateBootStatus: UpdateBootStatus | null;
};
const DEFAULT_RELEASE_INDEX_PATH = '/updates/latest.json';

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

function updatePartitionLabel(present: boolean, offset: number, size: number): string {
  if (!present) {
    return 'missing';
  }
  return `0x${offset.toString(16)} · ${size} bytes`;
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

function parseMaybeRelativeReleaseUrl(value: string, base: URL, label: string): URL {
  try {
    const url = new URL(value, base);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      throw new Error(`${label} must use http or https`);
    }
    return url;
  } catch (error: any) {
    const message = error?.message ?? error?.toString() ?? 'invalid URL';
    throw new Error(`${label}: ${message}`);
  }
}

function configuredReleaseIndexUrl(): URL {
  const configured = import.meta.env.VITE_NOCKSTER_RELEASE_INDEX_URL?.trim() || DEFAULT_RELEASE_INDEX_PATH;
  const base = typeof window === 'undefined' ? 'http://localhost/' : window.location.href;
  return parseMaybeRelativeReleaseUrl(configured, new URL(base), 'release index URL');
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
  const [advancedUpdateExpanded, setAdvancedUpdateExpanded] = useState(false);

  // Transaction signing state
  const [wasmReady, setWasmReady] = useState(false);
  const [wasm, setWasm] = useState<NocksterWasm | null>(null);
  const [tx, setTx] = useState<ParsedTransactionInstance | null>(null);
  const [txInfo, setTxInfo] = useState<any>(null);
  const [txDetails, setTxDetails] = useState<any>(null);
  const [txBytes, setTxBytes] = useState<Uint8Array | null>(null);
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
    let cancelled = false;

    import('nockster-wasm')
      .then(async (module) => {
        await module.default();
        if (cancelled) {
          return;
        }
        console.log('WASM initialized successfully');
        setWasm(module);
        setWasmReady(true);
      })
      .catch(err => {
        if (cancelled) {
          return;
        }
        console.error('Failed to initialize WASM:', err);
        setStatus('WASM initialization failed');
      });

    return () => {
      cancelled = true;
    };
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
  const deviceRebootAvailable = !!info && (info.features & FEATURE_DEVICE_REBOOT) !== 0;
  const nvsMigrationReady =
    securityStatusAvailable &&
    !!securityStatus &&
    securityStatus.nvs_initialized &&
    securityStatus.nvs_schema_version !== 2 &&
    securityStatus.chip_security_available &&
    securityStatus.hmac_user_key_slots !== 0;
  const updateBlockReason = getUpdateBundleCompatibilityBlocker(updateBundle, {
    releaseVersion: firmwareReleaseVersion,
    buildInfo: firmwareBuildInfo,
  });
  const updateBlocked = updateBlockReason !== null;
  const updatePercent = updateProgress && updateProgress.image_size > 0
    ? Math.min(100, Math.round((updateProgress.bytes_received / updateProgress.image_size) * 100))
    : 0;
  const latestReleaseIndexLabel = (() => {
    try {
      return configuredReleaseIndexUrl().href;
    } catch {
      return 'invalid release index';
    }
  })();

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

  const connectDevice = async (): Promise<DeviceStatusSnapshot | null> => {
    console.log('Connect requested, isTauri:', isTauri);
    setStatus('Connecting...');

    if (isTauri && transport && selectedPort && transport.setSelectedPort) {
      console.log('Setting port:', selectedPort);
      transport.setSelectedPort(selectedPort);
    }

    await device.connect();
    setConnected(true);
    setStatus('Connected!');
    await sleep(1000);
    return await refreshStatus();
  };

  const connect = async () => {
    try {
      await connectDevice();
    } catch (error: any) {
      console.error('Connection error:', error);
      setStatus(`Connection failed: ${error.message}`);
    }
  };

  const clearConnectedDeviceState = () => {
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
  };

  const disconnect = async () => {
    try {
      await device.disconnect();
      clearConnectedDeviceState();
      setStatus('Disconnected');
    } catch (error: any) {
      setStatus(`Disconnect failed: ${error.message}`);
    }
  };

  const refreshStatus = async (
    preferSlot?: number,
    infoOverride?: InfoResponse,
  ): Promise<DeviceStatusSnapshot | null> => {
    try {
      const lockStatus = await device.getLockStatus();
      setLocked(lockStatus.locked);
      setAttemptsRemaining(lockStatus.attempts_remaining);

      const deviceInfo = infoOverride ?? (await device.getInfo());
      if (deviceInfo.type === 'Info') {
        let nextReleaseVersion: number | null = null;
        let nextBuildInfo: BuildInfo | null = null;
        let nextUpdateBootStatus: UpdateBootStatus | null = null;

        setInfo(deviceInfo);
        if ((deviceInfo.features & FEATURE_RELEASE_INFO) !== 0) {
          try {
            const release = await device.getReleaseInfo();
            nextReleaseVersion = Number(release.release_version);
          } catch (err: any) {
            console.warn('getReleaseInfo failed', err);
          }
        }
        setFirmwareReleaseVersion(nextReleaseVersion);

        if ((deviceInfo.features & FEATURE_BUILD_INFO) !== 0) {
          try {
            nextBuildInfo = await device.getBuildInfo();
          } catch (err: any) {
            console.warn('getBuildInfo failed', err);
          }
        }
        setFirmwareBuildInfo(nextBuildInfo);

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
            nextUpdateBootStatus = await device.getUpdateBootStatus();
          } catch (err: any) {
            console.warn('getUpdateBootStatus failed', err);
          }
        }
        setUpdateBootStatus(nextUpdateBootStatus);

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
          return {
            info: deviceInfo,
            releaseVersion: nextReleaseVersion,
            buildInfo: nextBuildInfo,
            updateBootStatus: nextUpdateBootStatus,
          };
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

        return {
          info: deviceInfo,
          releaseVersion: nextReleaseVersion,
          buildInfo: nextBuildInfo,
          updateBootStatus: nextUpdateBootStatus,
        };
      }
      return {
        info: null,
        releaseVersion: null,
        buildInfo: null,
        updateBootStatus: null,
      };
    } catch (error: any) {
      setStatus(`Status check failed: ${error.message}`);
      return null;
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
          await assertUpdateFirmwareMatchesBundle(bundle, firmwareBytes);
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
      const blocker = getUpdateBundleCompatibilityBlocker(bundle, {
        releaseVersion: firmwareReleaseVersion,
        buildInfo: firmwareBuildInfo,
      });
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
        await assertUpdateFirmwareMatchesBundle(updateBundle, bytes);
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

  const assertUpdateBundleCompatible = (
    bundle: UpdateBundle,
    deviceReleaseVersion: number | null = firmwareReleaseVersion,
    deviceBuildInfo: BuildInfo | null = firmwareBuildInfo,
  ): void => {
    const blocker = getUpdateBundleCompatibilityBlocker(bundle, {
      releaseVersion: deviceReleaseVersion,
      buildInfo: deviceBuildInfo,
    });
    if (blocker) {
      throw new Error(blocker);
    }
  };

  const stageUpdateRelease = (release: FetchedUpdateRelease) => {
    setUpdateBundle(release.bundle);
    setUpdateBundleName(release.bundleName);
    setFirmwareBytes(release.firmware);
    setFirmwareName(release.firmwareName);
    setUpdateProgress(null);
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
      setFetchingRelease(true);
      setUpdateProgress(null);
      const release = await fetchUpdateReleaseArtifacts(parsedBundleUrl, parsedFirmwareUrl, {
        bearerToken: releaseBearerToken,
        validateBundle: assertUpdateBundleCompatible,
      });

      stageUpdateRelease(release);
      setReleaseBearerToken('');
      setStatus(`Fetched update release ${release.bundle.manifest.release_version}`);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Release fetch failed: ${message}`);
    } finally {
      setFetchingRelease(false);
    }
  };

  const fetchLatestUpdateRelease = async (
    deviceReleaseVersion: number | null = firmwareReleaseVersion,
    deviceBuildInfo: BuildInfo | null = firmwareBuildInfo,
  ): Promise<FetchedUpdateRelease> => fetchLatestUpdateReleaseFromIndex(configuredReleaseIndexUrl(), {
    validateBundle: (bundle) => assertUpdateBundleCompatible(bundle, deviceReleaseVersion, deviceBuildInfo),
  });

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

  const runUpdateStream = async (
    bundle: UpdateBundle,
    firmware: Uint8Array,
    writeFlash: boolean,
    deviceReleaseVersion: number | null = firmwareReleaseVersion,
    deviceBuildInfo: BuildInfo | null = firmwareBuildInfo,
    canReadUpdateBootStatus: boolean = updateBootStatusAvailable,
  ): Promise<UpdateStatus> => {
    try {
      await assertUpdateFirmwareMatchesBundle(bundle, firmware);
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      throw new Error(`Update image check failed: ${message}`);
    }

    const blocker = getUpdateBundleCompatibilityBlocker(bundle, {
      releaseVersion: deviceReleaseVersion,
      buildInfo: deviceBuildInfo,
    });
    if (blocker) {
      throw new Error(blocker);
    }

    const finalStatus = await device.streamUpdateBundle(bundle, firmware, {
      writeFlash,
      onProgress: (progress) => setUpdateProgress(progress),
    });
    if (writeFlash && canReadUpdateBootStatus) {
      const bootStatus = await device.getUpdateBootStatus();
      setUpdateBootStatus(bootStatus);
      assertPostInstallUpdateBootStatus(bootStatus);
    }
    return finalStatus;
  };

  const rebootConnectedDevice = async (successStatus: string) => {
    setStatus('Rebooting device...');
    await device.reboot();
    try {
      await device.disconnect();
    } catch {
      // The USB device may already have disappeared because the reboot succeeded.
    }
    clearConnectedDeviceState();
    setStatus(successStatus);
  };

  const offerPostInstallReboot = async (
    installStatus: string,
    canReboot: boolean,
    options: { autoReboot?: boolean } = {},
  ) => {
    if (!canReboot) {
      setStatus(`${installStatus}; press reset or replug to boot it.`);
      return;
    }

    if (!options.autoReboot) {
      const rebootNow = window.confirm(`${installStatus}\n\nReboot now to start it?`);
      if (!rebootNow) {
        setStatus(`${installStatus}; reboot when ready to start it.`);
        return;
      }
    }

    try {
      await rebootConnectedDevice('Device rebooting into the installed firmware. Reconnect after it appears.');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`${installStatus}; reboot command failed: ${message}. Press reset or replug to finish.`);
    }
  };

  const rebootDevice = async () => {
    if (!connected) {
      setStatus('Connect the device first');
      return;
    }
    if (!deviceRebootAvailable) {
      setStatus('Reboot command is not available on this firmware');
      return;
    }
    if (!window.confirm('Reboot the device now?')) {
      return;
    }

    try {
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      await rebootConnectedDevice('Device rebooting. Reconnect after it appears.');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Reboot failed: ${message}`);
    } finally {
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
      const finalStatus = await runUpdateStream(updateBundle, firmwareBytes, writeFlash);
      const doneStatus = writeFlash
        ? `Firmware installed for next boot (${finalStatus.image_size} bytes verified)`
        : `Firmware image verified on device (${finalStatus.image_size} bytes)`;
      if (writeFlash) {
        await offerPostInstallReboot(doneStatus, deviceRebootAvailable);
      } else {
        setStatus(doneStatus);
      }
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Update stream failed: ${message}`);
    } finally {
      setUpdatingFirmware(false);
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const readUpdateDeviceSnapshot = async (): Promise<DeviceStatusSnapshot> => {
    const snapshot = connected ? await refreshStatus() : await connectDevice();
    if (!snapshot?.info) {
      throw new Error('Could not read device firmware metadata');
    }
    return snapshot;
  };

  const installLatestUpdate = async () => {
    if (!connected && !isSupported) {
      setStatus('WebHID/Web Serial API not supported in this browser');
      return;
    }

    try {
      setFetchingRelease(true);
      setUpdatingFirmware(true);
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setUpdateProgress(null);
      const snapshot = await readUpdateDeviceSnapshot();
      const features = snapshot.info?.features ?? 0;
      const canReadUpdateBootStatus = (features & FEATURE_UPDATE_BOOT_STATUS) !== 0;
      const canRebootDevice = (features & FEATURE_DEVICE_REBOOT) !== 0;
      if ((features & FEATURE_SECURE_UPDATE) === 0) {
        throw new Error('Secure update is not available on this firmware');
      }
      if (!canReadUpdateBootStatus) {
        throw new Error('Firmware install requires update boot status support');
      }

      const confirmed = window.confirm(
        'Fetch the latest signed firmware from this site and install it into the inactive OTA slot?'
      );
      if (!confirmed) {
        setStatus('Firmware update cancelled');
        return;
      }

      setStatus('Fetching latest firmware release...');
      const release = await fetchLatestUpdateRelease(snapshot.releaseVersion, snapshot.buildInfo);
      stageUpdateRelease(release);
      setStatus('Installing latest firmware update...');
      const finalStatus = await runUpdateStream(
        release.bundle,
        release.firmware,
        true,
        snapshot.releaseVersion,
        snapshot.buildInfo,
        canReadUpdateBootStatus,
      );
      await offerPostInstallReboot(
        `Firmware release ${release.bundle.manifest.release_version} installed for next boot (${finalStatus.image_size} bytes verified)`,
        canRebootDevice,
        { autoReboot: true },
      );
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Latest update failed: ${message}`);
    } finally {
      setFetchingRelease(false);
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
      if (!wasm) {
        setStatus('WASM API unavailable, refresh the page and try again');
        return;
      }

      console.log('Loading transaction file:', file.name, file.size, 'bytes');
      setStatus('Loading transaction...');

      const bytes = new Uint8Array(await file.arrayBuffer());
      setTxBytes(bytes);
      console.log('File loaded, bytes:', bytes.length);
      console.log('First 32 bytes:', Array.from(bytes.slice(0, 32)).map(b => b.toString(16).padStart(2, '0')).join(' '));

      console.log('Creating ParsedTransaction...');
      const parsedTx = new wasm.ParsedTransaction(bytes);
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
      setStatus(`Loaded transaction: ${info.tx_id} (${info.input_count} spends)`);

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
    if (!wasm) {
      setStatus('WASM API unavailable, refresh the page and try again');
      return;
    }

    try {
      deviceBusyRef.current = true;
      setDeviceBusy(true);
      setSigning(true);
      const txVersion = txDetails?.version ?? 0;
      if (txVersion !== 1) {
        throw new Error('Only Bythos/V1 transaction drafts are supported');
      }

      setStatus(`Selecting slot ${selectedSlot}...`);
      await device.selectSeed(selectedSlot);

      setStatus('Sending draft to device (approve on-device)...');
      setBanner({ open: true, message: 'Sending draft to device (approve on-device)...' });
      const signedBytes = await device.signDraft(txBytes);

      const signedParsed = new wasm.ParsedTransaction(signedBytes);
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
    setSignedTxBytes(null);
    setStatus('');
  };

  const formatDeviceAddress = (pub: DeviceKey): string => {
    if (wasmReady && wasm) {
      try {
        return wasm.cheetah_pkh_b58(
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
          <Suspense fallback={<div className="status-message">loading composer...</div>}>
            <ComposerView wasmReady={wasmReady} />
          </Suspense>
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
              <div className="button-group control-actions">
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
                <button
                  type="button"
                  onClick={rebootDevice}
                  disabled={deviceBusy || !deviceRebootAvailable}
                  className="btn btn-small btn-secondary seed-toggle"
                >
                  reboot
                </button>
                <button
                  type="button"
                  onClick={installLatestUpdate}
                  disabled={
                    deviceBusy ||
                    updatingFirmware ||
                    fetchingRelease ||
                    !secureUpdateAvailable ||
                    !updateBootStatusAvailable
                  }
                  className="btn btn-small btn-primary seed-toggle"
                >
                  {updatingFirmware || fetchingRelease ? 'updating...' : 'update firmware'}
                </button>
                <button
                  type="button"
                  onClick={() => setAdvancedUpdateExpanded((expanded) => !expanded)}
                  disabled={deviceBusy || updatingFirmware || fetchingRelease}
                  className="btn btn-small btn-secondary seed-toggle"
                >
                  {advancedUpdateExpanded ? 'hide advanced' : 'advanced'}
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
              <div className="status-item full-width">
                <span className="label">latest source:</span>
                <span className="value update-hash">{latestReleaseIndexLabel}</span>
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
            {advancedUpdateExpanded && (
              <div className="update-advanced">
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
            )}
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
        <div className="section connect-panel">
          {isTauri && availablePorts.length === 0 && (
            <div className="connect-actions">
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
          <div className="update-grid">
            <div className="status-item full-width">
              <span className="label">latest source:</span>
              <span className="value update-hash">{latestReleaseIndexLabel}</span>
            </div>
          </div>
          <div className="connect-actions">
            <button
              onClick={installLatestUpdate}
              className="btn btn-primary"
              disabled={
                !isSupported ||
                updatingFirmware ||
                fetchingRelease ||
                deviceBusy ||
                (isTauri && !selectedPort)
              }
            >
              {updatingFirmware || fetchingRelease ? 'updating...' : 'update firmware'}
            </button>
            <button
              onClick={connect}
              className="btn btn-secondary"
              disabled={!isSupported || deviceBusy || updatingFirmware || fetchingRelease || (isTauri && !selectedPort)}
            >
              connect
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
