import { Suspense, lazy, useState, useEffect, useMemo, useRef, useCallback } from 'react';
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
  parseUpdateReleaseIndexJson,
  FEATURE_SECURITY_STATUS,
  FEATURE_BUILD_INFO,
  FEATURE_SEED_LABELS,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_UPDATE_BOOT_STATUS,
  FEATURE_DEVICE_REBOOT,
  FEATURE_DEVICE_ADDRESS_BOOK,
  MAX_DEVICE_ADDRESS_BOOK_ENTRIES,
  MAX_ADDRESS_BOOK_LABEL_LEN,
  MAX_ADDRESS_BOOK_PKH_LEN,
  MAX_VAULT_PREIMAGE_LEN,
  hexToBytes,
} from 'nockster-js';
import type {
  BuildInfo,
  DeviceAddressBookEntry,
  FetchedUpdateRelease,
  SecurityStatus,
  SeedSlotLabel,
  UpdateBootStatus,
  VaultEntryInfo,
} from 'nockster-js';
import { mnemonicToSeed, validateMnemonicWords, isValidMnemonicWordCount } from './bip39';
import { createSerialTransport } from './serial';
import { NounInspector } from './NounInspector';
import type { WalletAddress } from './composer/types';
import {
  NOCKBLOCKS_API_KEY_STORAGE_KEY,
  fetchNockblocksNotes,
} from './composer/nockblocks';
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
type SlotBalance = {
  status: 'ok' | 'error';
  nicks?: number;
  notes?: number;
  error?: string;
};
type DeviceStatusSnapshot = {
  info: InfoResponse | null;
  releaseVersion: number | null;
  buildInfo: BuildInfo | null;
  updateBootStatus: UpdateBootStatus | null;
};
const DEFAULT_RELEASE_INDEX_PATH = 'https://bin.aeroe.io/nockster/updates/latest.json';
const RELEASE_INDEX_STORAGE_KEY = 'nockster.update.releaseIndexUrl.v1';

type ConfirmOptions = {
  title?: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
};

// The official SWPSCo firmware-update signing key (SHA-256 of the public key).
// When the device's configured trust anchor matches this, we show a verified badge.
const OFFICIAL_TRUST_ANCHOR = '5aa46209222080a2ce107e25d427c3d9ada6cb77be25d7d2a3df8959b7fa2602';
const MAX_SEED_LABEL_LEN = 32;
const AUTO_BALANCE_REFRESH_MS = 60_000;
const NICKS_PER_NOCK = 1n << 16n;
const NOCK_DEC_SCALE = 10n ** 6n;

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

function convertMapToObject(obj: any): any {
  if (obj instanceof Map) {
    const result: any = {};
    obj.forEach((value, key) => {
      result[key] = convertMapToObject(value);
    });
    return result;
  }
  if (Array.isArray(obj)) {
    return obj.map(convertMapToObject);
  }
  if (obj && typeof obj === 'object') {
    const result: any = {};
    for (const key in obj) {
      result[key] = convertMapToObject(obj[key]);
    }
    return result;
  }
  return obj;
}

function defaultReleaseIndexSource(): string {
  return import.meta.env.VITE_NOCKSTER_RELEASE_INDEX_URL?.trim() || DEFAULT_RELEASE_INDEX_PATH;
}

function configuredReleaseIndexUrl(source?: string): URL {
  const configured = source?.trim() || defaultReleaseIndexSource();
  const base = typeof window === 'undefined' ? 'http://localhost/' : window.location.href;
  return parseMaybeRelativeReleaseUrl(configured, new URL(base), 'release index URL');
}

function validSeedLabel(label: string): boolean {
  return label.length <= MAX_SEED_LABEL_LEN
    && Array.from(label).every((ch) => {
      const code = ch.charCodeAt(0);
      return code === 0x20 || (code >= 0x21 && code <= 0x7e);
    });
}

function validDeviceAddressLabel(label: string): boolean {
  return label.length > 0
    && label.length <= MAX_ADDRESS_BOOK_LABEL_LEN
    && Array.from(label).every((ch) => {
      const code = ch.charCodeAt(0);
      return code === 0x20 || (code >= 0x21 && code <= 0x7e);
    });
}

function validDeviceAddressPkh(pkh: string): boolean {
  return pkh.length > 0
    && pkh.length <= MAX_ADDRESS_BOOK_PKH_LEN
    && /^[1-9A-HJ-NP-Za-km-z]+$/.test(pkh);
}

function normalizeDeviceAddressEntry(entry: DeviceAddressBookEntry): DeviceAddressBookEntry {
  return {
    label: entry.label.trim(),
    pkh: entry.pkh.trim(),
  };
}

function formatNicksCompact(nicks: number): string {
  if (!Number.isFinite(nicks)) return '-';
  const value = BigInt(Math.trunc(nicks));
  if (value < NICKS_PER_NOCK) {
    return `${value.toString()} n`;
  }
  const whole = value / NICKS_PER_NOCK;
  const frac = value % NICKS_PER_NOCK;
  if (frac === 0n) return `${whole.toString()} N`;
  const fracStr = ((frac * NOCK_DEC_SCALE) / NICKS_PER_NOCK)
    .toString()
    .padStart(6, '0')
    .replace(/0+$/, '');
  return `${whole.toString()}.${fracStr} N`;
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
  const [seedLabels, setSeedLabels] = useState<SeedSlotLabel[]>([]);
  const [labelDrafts, setLabelDrafts] = useState<Record<number, string>>({});
  const [savingLabelSlot, setSavingLabelSlot] = useState<number | null>(null);
  const [deviceAddressBook, setDeviceAddressBook] = useState<DeviceAddressBookEntry[]>([]);
  const [addressBookLabel, setAddressBookLabel] = useState('');
  const [addressBookPkh, setAddressBookPkh] = useState('');
  const [addressBookStatus, setAddressBookStatus] = useState('');
  const [syncingAddressBook, setSyncingAddressBook] = useState(false);
  const [walletPanelView, setWalletPanelView] = useState<'slots' | 'addresses' | 'vault'>('slots');

  // Preimage vault state
  const [vaultEntries, setVaultEntries] = useState<VaultEntryInfo[] | null>(null);
  const [vaultBusy, setVaultBusy] = useState(false);
  const [vaultStatus, setVaultStatus] = useState('');
  const [vaultLabel, setVaultLabel] = useState('');
  const [vaultSecretHex, setVaultSecretHex] = useState('');
  const [vaultInputIsJam, setVaultInputIsJam] = useState(false);
  const [vaultRevealed, setVaultRevealed] = useState<{
    slot: number;
    label: string;
    jamHex: string;
    atomHex: string | null;
  } | null>(null);
  const [slotBalances, setSlotBalances] = useState<Record<number, SlotBalance>>({});
  const [balanceStatus, setBalanceStatus] = useState('');
  const [syncingBalances, setSyncingBalances] = useState(false);
  const lastAutoBalanceRefreshAtRef = useRef(0);
  const [deviceNockblocksKey, setDeviceNockblocksKey] = useState('');
  const [nockblocksKeyDraft, setNockblocksKeyDraft] = useState('');
  const [selectedSlotState, setSelectedSlotState] = useState<number>(0);

  const saveNockblocksKey = () => {
    const trimmed = nockblocksKeyDraft.trim();
    if (!trimmed) return;
    localStorage.setItem(NOCKBLOCKS_API_KEY_STORAGE_KEY, trimmed);
    setDeviceNockblocksKey(trimmed);
    setNockblocksKeyDraft('');
    setStatus('Saved Nockblocks API key');
  };
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
  const [updateBootStatus, setUpdateBootStatus] = useState<UpdateBootStatus | null>(null);
  const [updatingFirmware, setUpdatingFirmware] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateStatus | null>(null);
  const [releaseBundleUrl, setReleaseBundleUrl] = useState('');
  const [releaseFirmwareUrl, setReleaseFirmwareUrl] = useState('');
  const [releaseBearerToken, setReleaseBearerToken] = useState('');
  const [releaseIndexSource, setReleaseIndexSource] = useState(() => {
    if (typeof window === 'undefined') return defaultReleaseIndexSource();
    return localStorage.getItem(RELEASE_INDEX_STORAGE_KEY)?.trim() || defaultReleaseIndexSource();
  });
  const [releaseIndexDraft, setReleaseIndexDraft] = useState(releaseIndexSource);
  const [fetchingRelease, setFetchingRelease] = useState(false);
  const [advancedUpdateExpanded, setAdvancedUpdateExpanded] = useState(false);
  // Latest release version advertised by the update index, fetched index-only
  // (no firmware download) so we can compare against the device before offering
  // an install. null = unknown / not checked yet.
  const [latestReleaseVersion, setLatestReleaseVersion] = useState<number | null>(null);
  const [checkingLatestRelease, setCheckingLatestRelease] = useState(false);
  const [latestReleaseError, setLatestReleaseError] = useState<string | null>(null);
  const [updatesModalOpen, setUpdatesModalOpen] = useState(false);

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
    const composerCls = 'app-composer';
    const deviceCls = 'app-device';
    document.body.classList.toggle(composerCls, activeTab === 'composer');
    document.body.classList.toggle(deviceCls, activeTab === 'device');
    return () => {
      document.body.classList.remove(composerCls);
      document.body.classList.remove(deviceCls);
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
  const seedLabelsAvailable = !!info && (info.features & FEATURE_SEED_LABELS) !== 0;
  const releaseInfoAvailable = !!info && (info.features & FEATURE_RELEASE_INFO) !== 0;
  const buildInfoAvailable = !!info && (info.features & FEATURE_BUILD_INFO) !== 0;
  const updateBootStatusAvailable = !!info && (info.features & FEATURE_UPDATE_BOOT_STATUS) !== 0;
  const deviceRebootAvailable = !!info && (info.features & FEATURE_DEVICE_REBOOT) !== 0;
  const deviceAddressBookAvailable = !!info && (info.features & FEATURE_DEVICE_ADDRESS_BOOK) !== 0;
  const updateBlockReason = getUpdateBundleCompatibilityBlocker(updateBundle, {
    releaseVersion: firmwareReleaseVersion,
    buildInfo: firmwareBuildInfo,
  });
  const updateBlocked = updateBlockReason !== null;
  // Compare the device's installed release against the index. We only know the
  // answer once both numbers are in hand; otherwise treat it as "unknown" and
  // let the install-time check be the backstop.
  const updateIsNewer =
    latestReleaseVersion !== null &&
    firmwareReleaseVersion !== null &&
    latestReleaseVersion > firmwareReleaseVersion;
  const updateUpToDate =
    latestReleaseVersion !== null &&
    firmwareReleaseVersion !== null &&
    latestReleaseVersion <= firmwareReleaseVersion;
  const updatePercent = updateProgress && updateProgress.image_size > 0
    ? Math.min(100, Math.round((updateProgress.bytes_received / updateProgress.image_size) * 100))
    : 0;
  const seedLabelMap = useMemo(() => {
    const labels = new Map<number, string>();
    for (const entry of seedLabels) {
      labels.set(Number(entry.slot), entry.label);
    }
    return labels;
  }, [seedLabels]);
  const latestReleaseIndexLabel = (() => {
    try {
      return configuredReleaseIndexUrl(releaseIndexSource).href;
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

  // Read just the release index (not the firmware) so we can show the latest
  // available version and gate the install button without a device attached.
  const checkLatestReleaseVersion = useCallback(async () => {
    let url: URL;
    try {
      url = configuredReleaseIndexUrl(releaseIndexSource);
    } catch (error: any) {
      setLatestReleaseError(error?.message ?? 'invalid release index URL');
      setLatestReleaseVersion(null);
      return;
    }
    setCheckingLatestRelease(true);
    setLatestReleaseError(null);
    try {
      const res = await fetch(url.href, { cache: 'no-store' });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      const text = await res.text();
      const index = parseUpdateReleaseIndexJson(text, url);
      const version = index.metadata.releaseVersion;
      setLatestReleaseVersion(version ?? null);
      if (version === undefined) {
        setLatestReleaseError('index has no release_version');
      }
    } catch (error: any) {
      setLatestReleaseVersion(null);
      setLatestReleaseError(error?.message ?? 'could not reach update server');
    } finally {
      setCheckingLatestRelease(false);
    }
  }, [releaseIndexSource]);

  // Check once on load and whenever the configured index changes, so the splash
  // and the device panel both know the latest available version.
  useEffect(() => {
    void checkLatestReleaseVersion();
  }, [checkLatestReleaseVersion]);

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

  const deriveDevicePkh = useCallback((pub: DeviceKey): string | null => {
    if (!wasmReady || !wasm) return null;
    try {
      const address = wasm.cheetah_pkh_b58(
        pub.x.map((n) => n.toString()),
        pub.y.map((n) => n.toString())
      );
      return validDeviceAddressPkh(address) ? address : null;
    } catch {
      return null;
    }
  }, [wasm, wasmReady]);

  const walletAddresses = useMemo<WalletAddress[]>(() => {
    return Array.from(new Map(deviceKeys.map((pub) => [pub.slot, pub])).values())
      .sort((a, b) => a.slot - b.slot)
      .flatMap((pub) => {
        const address = deriveDevicePkh(pub);
        if (!address) return [];
        return [
          {
            slot: pub.slot,
            path: pub.path,
            pathLabel: formatDerivationPath(pub.path),
            address,
            alias: seedLabelMap.get(pub.slot)?.trim() || `wallet slot ${pub.slot}`,
          },
        ];
      });
  }, [deriveDevicePkh, deviceKeys, seedLabelMap]);

  const walletBySlot = useMemo(() => {
    const wallets = new Map<number, WalletAddress>();
    for (const wallet of walletAddresses) {
      wallets.set(wallet.slot, wallet);
    }
    return wallets;
  }, [walletAddresses]);
  const walletBalanceKey = useMemo(
    () => walletAddresses.map((wallet) => `${wallet.slot}:${wallet.address}`).sort().join('|'),
    [walletAddresses]
  );

  useEffect(() => {
    const saved = typeof window === 'undefined'
      ? ''
      : localStorage.getItem(NOCKBLOCKS_API_KEY_STORAGE_KEY)?.trim() || '';
    if (saved) {
      setDeviceNockblocksKey(saved);
    }
  }, []);

  useEffect(() => {
    if (activeTab !== 'device' || typeof window === 'undefined') return;
    const saved = localStorage.getItem(NOCKBLOCKS_API_KEY_STORAGE_KEY)?.trim() || '';
    if (saved && saved !== deviceNockblocksKey) {
      setDeviceNockblocksKey(saved);
    }
  }, [activeTab, deviceNockblocksKey]);

  const refreshWalletBalances = useCallback(async (quiet = false) => {
    lastAutoBalanceRefreshAtRef.current = Date.now();
    const key = deviceNockblocksKey.trim();
    if (!key) {
      setSlotBalances({});
      if (!quiet) setBalanceStatus('Nockblocks API key is not configured');
      return;
    }
    if (walletAddresses.length === 0) {
      setSlotBalances({});
      setBalanceStatus('');
      return;
    }

    setSyncingBalances(true);
    if (!quiet) setBalanceStatus('refreshing balances...');
    const next: Record<number, SlotBalance> = {};
    try {
      await Promise.all(walletAddresses.map(async (wallet) => {
        try {
          const imported = await fetchNockblocksNotes({
            address: wallet.address,
            apiKey: key,
          });
          next[wallet.slot] = {
            status: 'ok',
            nicks: imported.nicks,
            notes: imported.notes.length,
          };
        } catch (err: any) {
          next[wallet.slot] = {
            status: 'error',
            error: err?.message ?? String(err),
          };
        }
      }));

      setSlotBalances(next);
      const failures = Object.values(next).filter((entry) => entry.status === 'error').length;
      if (!quiet || failures > 0) {
        setBalanceStatus(
          failures > 0
            ? `balance refresh failed for ${failures} slot${failures === 1 ? '' : 's'}`
            : 'balances refreshed'
        );
      }
    } finally {
      setSyncingBalances(false);
    }
  }, [deviceNockblocksKey, walletAddresses]);

  useEffect(() => {
    if (!connected || activeTab !== 'device' || !walletBalanceKey || !deviceNockblocksKey.trim()) {
      return;
    }
    const now = Date.now();
    if (now - lastAutoBalanceRefreshAtRef.current < AUTO_BALANCE_REFRESH_MS) {
      return;
    }
    lastAutoBalanceRefreshAtRef.current = now;
    void refreshWalletBalances(true);
  }, [activeTab, connected, deviceNockblocksKey, refreshWalletBalances, walletBalanceKey]);


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
    setSeedLabels([]);
    setLabelDrafts({});
    setDeviceAddressBook([]);
    setAddressBookLabel('');
    setAddressBookPkh('');
    setAddressBookStatus('');
    setSlotBalances({});
    setBalanceStatus('');
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
    setLoadingDevice(true);
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

        if ((deviceInfo.features & FEATURE_SECURE_UPDATE) !== 0) {
          try {
            const trust = await device.getUpdateTrust();
            setUpdateTrustHash(trust.configured ? bytesToHex(trust.pubkey_sha256) : null);
          } catch (err: any) {
            console.warn('getUpdateTrust failed', err);
            setUpdateTrustHash(null);
          }
        } else {
          setUpdateTrustHash(null);
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

        let nextLabels: SeedSlotLabel[] = [];
        if ((deviceInfo.features & FEATURE_SEED_LABELS) !== 0) {
          try {
            nextLabels = await device.getSeedLabels();
          } catch (err: any) {
            console.warn('getSeedLabels failed', err);
          }
        }
        setSeedLabels(nextLabels);
        setLabelDrafts((current) => {
          const labelsBySlot = new Map(nextLabels.map((entry) => [Number(entry.slot), entry.label]));
          const slots = new Set(normalizedKeys.map((pub) => pub.slot));
          const next: Record<number, string> = {};
          for (const slot of slots) {
            const currentValue = current[slot] ?? '';
            const storedValue = labelsBySlot.get(slot) ?? '';
            next[slot] = currentValue.trim() ? currentValue : storedValue || currentValue;
          }
          return next;
        });

        let nextAddressBook: DeviceAddressBookEntry[] = [];
        if (!lockStatus.locked && (deviceInfo.features & FEATURE_DEVICE_ADDRESS_BOOK) !== 0) {
          try {
            nextAddressBook = await device.getAddressBook();
          } catch (err: any) {
            console.warn('getAddressBook failed', err);
          }
        }
        setDeviceAddressBook(nextAddressBook);

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
    } finally {
      setLoadingDevice(false);
    }
  };

  const deviceBusyRef = useRef(false);
  const [deviceBusy, setDeviceBusy] = useState(false);
  const [loadingDevice, setLoadingDevice] = useState(false);

  // In-app confirmation dialog (replaces window.confirm). Promise-based so call
  // sites stay `const ok = await askConfirm({...})`.
  const [confirmState, setConfirmState] = useState<ConfirmOptions | null>(null);
  const confirmResolver = useRef<((ok: boolean) => void) | null>(null);
  const askConfirm = useCallback((opts: ConfirmOptions): Promise<boolean> => {
    return new Promise((resolve) => {
      confirmResolver.current = resolve;
      setConfirmState(opts);
    });
  }, []);
  const resolveConfirm = useCallback((ok: boolean) => {
    setConfirmState(null);
    const resolve = confirmResolver.current;
    confirmResolver.current = null;
    resolve?.(ok);
  }, []);
  const canSignComposerDraft = connected && locked === false && !deviceBusy && !signing;
  const composerSignDisabledReason = !connected
    ? 'connect device to sign'
    : locked !== false
      ? 'unlock device to sign'
      : deviceBusy || signing
        ? 'device is busy'
        : undefined;

  // No continuous polling. The device re-derives every pubkey on each getInfo, so a
  // background poll meant constant re-reads (slow over USB) and could race in-flight
  // writes (briefly rendering un-derivable keys). State is refreshed on demand instead:
  // after connect / unlock / lock / seed add+remove / reset, and via the refresh button.

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
    const confirmed = await askConfirm({
      title: 'Reset device',
      message: 'This will erase the seed and PIN from the device. This cannot be undone.',
      confirmLabel: 'Erase device',
      danger: true,
    });
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
      setSeedLabels([]);
      setLabelDrafts({});
      setDeviceAddressBook([]);
      setAddressBookLabel('');
      setAddressBookPkh('');
      setAddressBookStatus('');
      setSlotBalances({});
      setBalanceStatus('');
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
  ): Promise<FetchedUpdateRelease> => fetchLatestUpdateReleaseFromIndex(configuredReleaseIndexUrl(releaseIndexSource), {
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
      const rebootNow = await askConfirm({
        title: 'Firmware installed',
        message: `${installStatus}\n\nReboot now to start it?`,
        confirmLabel: 'Reboot now',
        cancelLabel: 'Later',
      });
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
    if (!(await askConfirm({
      title: 'Reboot device',
      message: 'Reboot the device now?',
      confirmLabel: 'Reboot',
    }))) {
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
      const confirmed = await askConfirm({
        title: 'Install firmware',
        message: 'Install this firmware into the inactive OTA slot and activate it for next boot?',
        confirmLabel: 'Install',
      });
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
    setUpdatesModalOpen(false);

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

      // Now that the device version is known, refuse to "update" to the same or
      // an older release instead of surprising the user with a rollback error.
      if (
        latestReleaseVersion !== null &&
        snapshot.releaseVersion !== null &&
        latestReleaseVersion <= snapshot.releaseVersion
      ) {
        setStatus(`Already on the latest firmware (release ${snapshot.releaseVersion})`);
        return;
      }

      const confirmed = await askConfirm({
        title: 'Update firmware',
        message: 'Fetch the latest signed firmware and install it into the inactive OTA slot?',
        confirmLabel: 'Fetch & install',
      });
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
    const confirmed = await askConfirm({
      title: 'Remove seed slot',
      message: `Remove seed slot ${slot}? This cannot be undone.`,
      confirmLabel: 'Remove',
      danger: true,
    });
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

  const parseTransactionBytes = (bytes: Uint8Array): {
    parsedTx: ParsedTransactionInstance;
    info: any;
    details: any;
  } => {
    if (!wasm) {
      throw new Error('WASM API unavailable, refresh the page and try again');
    }

    const parsedTx = new wasm.ParsedTransaction(bytes);
    const info = parsedTx.info();
    const details = convertMapToObject(parsedTx.get_details());
    return { parsedTx, info, details };
  };

  const setLoadedTransaction = (
    bytes: Uint8Array,
    loaded: { parsedTx: ParsedTransactionInstance; info: any; details: any }
  ) => {
    setTx(loaded.parsedTx);
    setTxInfo(loaded.info);
    setTxDetails(loaded.details);
    setTxBytes(bytes);
    setSignedTxBytes(null);
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
      console.log('File loaded, bytes:', bytes.length);
      console.log('First 32 bytes:', Array.from(bytes.slice(0, 32)).map(b => b.toString(16).padStart(2, '0')).join(' '));

      console.log('Creating ParsedTransaction...');
      const loaded = parseTransactionBytes(bytes);
      console.log('ParsedTransaction created successfully');
      console.log('Transaction info:', loaded.info);
      console.log('Converted details:', loaded.details);

      setLoadedTransaction(bytes, loaded);
      setStatus(`Loaded transaction: ${loaded.info.tx_id} (${loaded.info.input_count} spends)`);

    } catch (error: any) {
      console.error('Transaction load error:', error);
      console.error('Error stack:', error.stack);
      setStatus(`Failed to load transaction: ${error.message || error.toString()}`);
    }
  };

  const signDraftBytes = async (
    draftBytes: Uint8Array,
    loadedInput?: { parsedTx: ParsedTransactionInstance; info: any; details: any }
  ) => {
    const loaded = loadedInput ?? parseTransactionBytes(draftBytes);
    if (!connected || locked) {
      setStatus('Device must be connected and unlocked');
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
      setLoadedTransaction(draftBytes, loaded);
      const txVersion = loaded.details?.version ?? loaded.info?.version ?? 0;
      if (txVersion !== 1) {
        throw new Error('Only Bythos/V1 transaction drafts are supported');
      }

      setStatus(`Selecting slot ${selectedSlot}...`);
      await device.selectSeed(selectedSlot);

      setStatus('Sending draft to device (approve on-device)...');
      setBanner({ open: true, message: 'Sending draft to device (approve on-device)...' });
      const signedBytes = await device.signDraft(draftBytes);

      const signedLoaded = parseTransactionBytes(signedBytes);

      setTx(signedLoaded.parsedTx);
      setTxInfo(signedLoaded.info);
      setTxDetails(signedLoaded.details);
      setTxBytes(signedBytes);
      setSignedTxBytes(signedBytes);

      const filename = `${signedLoaded.info.tx_id.slice(0, 16)}.tx`;
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

  const signTransaction = async () => {
    if (!tx || !txInfo || !txBytes) {
      setStatus('Missing transaction bytes; reload the file');
      return;
    }
    await signDraftBytes(txBytes, { parsedTx: tx, info: txInfo, details: txDetails });
  };

  const signComposerDraft = async (draft: { psnt: Uint8Array; filename: string; summaryJson: string }) => {
    try {
      if (!wasmReady) {
        setStatus('WASM not ready yet, please wait...');
        return;
      }
      setStatus(`Loaded composer draft ${draft.filename}`);
      const bytes = new Uint8Array(draft.psnt);
      await signDraftBytes(bytes);
    } catch (error: any) {
      setStatus(`Composer signing failed: ${error?.message ?? String(error)}`);
    }
  };

  const downloadBytesAs = (filename: string, bytes: Uint8Array) => {
    const ab = new ArrayBuffer(bytes.byteLength);
    new Uint8Array(ab).set(bytes);
    const blob = new Blob([ab], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const downloadSignedTx = () => {
    if (!signedTxBytes || !txInfo) return;
    downloadBytesAs(`${txInfo.tx_id.slice(0, 16)}.tx`, signedTxBytes);
  };

  const vaultCommitmentB58 = useCallback(
    (commitment: bigint[]): string => {
      if (!wasm) return commitment.map((limb) => limb.toString(16)).join(':');
      try {
        return wasm.tip5_limbs_b58(commitment.map((limb) => limb.toString()));
      } catch {
        return commitment.map((limb) => limb.toString(16)).join(':');
      }
    },
    [wasm],
  );

  const refreshVault = async () => {
    setVaultBusy(true);
    setVaultStatus('');
    try {
      setVaultEntries(await device.vaultList());
    } catch (error: any) {
      setVaultStatus(`Vault list failed: ${error?.message ?? String(error)}`);
    } finally {
      setVaultBusy(false);
    }
  };

  const storeVaultSecret = async () => {
    if (!wasm) {
      setVaultStatus('WASM not ready yet');
      return;
    }
    let raw: Uint8Array;
    try {
      raw = hexToBytes(vaultSecretHex.trim().replace(/^0x/, ''));
    } catch (error: any) {
      setVaultStatus(`Invalid hex: ${error?.message ?? String(error)}`);
      return;
    }
    if (raw.length === 0) {
      setVaultStatus('Secret is empty');
      return;
    }
    const preimage = vaultInputIsJam ? raw : wasm.jam_byte_atom(raw);
    if (preimage.length > MAX_VAULT_PREIMAGE_LEN) {
      setVaultStatus(`Preimage too large (${preimage.length} > ${MAX_VAULT_PREIMAGE_LEN} bytes jammed)`);
      return;
    }
    let commitment = '';
    try {
      commitment = wasm.noun_commitment_b58(preimage);
    } catch {
      setVaultStatus('Input is not a valid jammed noun');
      return;
    }
    setVaultBusy(true);
    setVaultStatus(`Confirm on device — commitment ${commitment}`);
    try {
      const entries = await device.vaultStore(vaultLabel.trim(), preimage);
      setVaultEntries(entries);
      setVaultLabel('');
      setVaultSecretHex('');
      setVaultStatus(`Stored. Commitment ${commitment}`);
    } catch (error: any) {
      setVaultStatus(`Store failed: ${error?.message ?? String(error)}`);
    } finally {
      setVaultBusy(false);
    }
  };

  const revealVaultSecret = async (entry: VaultEntryInfo) => {
    if (!wasm) {
      setVaultStatus('WASM not ready yet');
      return;
    }
    setVaultBusy(true);
    setVaultStatus('Confirm reveal on device...');
    setVaultRevealed(null);
    try {
      const { preimage } = await device.vaultReveal(entry.slot);
      let atomHex: string | null = null;
      try {
        atomHex = bytesToHex(wasm.cue_byte_atom(preimage));
      } catch {
        atomHex = null; // preimage is a cell noun; only the jam is meaningful
      }
      setVaultRevealed({
        slot: entry.slot,
        label: entry.label,
        jamHex: bytesToHex(preimage),
        atomHex,
      });
      setVaultStatus('');
    } catch (error: any) {
      setVaultStatus(`Reveal failed: ${error?.message ?? String(error)}`);
    } finally {
      setVaultBusy(false);
    }
  };

  const deleteVaultSecret = async (entry: VaultEntryInfo) => {
    setVaultBusy(true);
    setVaultStatus('Confirm delete on device...');
    try {
      const entries = await device.vaultDelete(entry.slot);
      setVaultEntries(entries);
      if (vaultRevealed?.slot === entry.slot) {
        setVaultRevealed(null);
      }
      setVaultStatus('Deleted.');
    } catch (error: any) {
      setVaultStatus(`Delete failed: ${error?.message ?? String(error)}`);
    } finally {
      setVaultBusy(false);
    }
  };

  const exportWatchOnly = async (slot: number, label: string) => {
    if (!wasm) {
      setStatus('WASM not ready yet');
      return;
    }
    setStatus('Confirm watch-only export on device...');
    try {
      const { x, y, chain_code } = await device.getMasterPubkey(slot);
      const bytes = wasm.build_master_pubkey_export(
        x.map((limb) => limb.toString()),
        y.map((limb) => limb.toString()),
        chain_code,
      );
      const name = label ? label.replace(/[^a-zA-Z0-9-_]/g, '-') : `slot-${slot}`;
      downloadBytesAs(`master-pubkey-${name}.export`, bytes);
      setStatus(
        'Watch-only keyfile downloaded. Import it with: nockchain-wallet import-master-pubkey --file <path>',
      );
    } catch (error: any) {
      setStatus(`Watch-only export failed: ${error?.message ?? String(error)}`);
    }
  };

  const verifyReceiveAddress = async (wallet: WalletAddress) => {
    if (locked !== false) {
      setStatus('Unlock the device before verifying a receive address');
      return;
    }
    deviceBusyRef.current = true;
    setDeviceBusy(true);
    setStatus('Compare the address with the device screen, then approve on-device...');
    try {
      await device.showAddress(wallet.slot, wallet.path);
      setStatus(`Device confirmed receive address for slot ${wallet.slot}`);
    } catch (error: any) {
      setStatus(`Receive address verification failed: ${error?.message ?? String(error)}`);
    } finally {
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const signMessageWithSlot = async (wallet: WalletAddress) => {
    if (locked !== false) {
      setStatus('Unlock the device before signing a message');
      return;
    }
    const message = window.prompt('Message to sign on the device:');
    if (message === null || message === '') return;
    deviceBusyRef.current = true;
    setDeviceBusy(true);
    setStatus('Review the message on the device screen, then approve on-device...');
    try {
      const { chal, sig } = await device.signMessage(
        wallet.slot,
        wallet.path,
        new TextEncoder().encode(message),
      );
      const hex = (limbs: bigint[]) =>
        limbs.map((l) => l.toString(16).padStart(16, '0')).join('');
      setStatus(`Signed. chal=${hex(chal)} sig=${hex(sig)}`);
    } catch (error: any) {
      setStatus(`Message signing failed: ${error?.message ?? String(error)}`);
    } finally {
      deviceBusyRef.current = false;
      setDeviceBusy(false);
    }
  };

  const importWalletKeyfile = async (file: File) => {
    if (!wasm) {
      setStatus('WASM not ready yet');
      return;
    }
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const summary = wasm.parse_wallet_keyfile(bytes) as {
        seedphrases: string[];
        coil_pub_count: number;
        coil_prv_count: number;
        versions: number[];
      };
      if (summary.seedphrases.length > 0) {
        setMnemonic(summary.seedphrases[0]);
        setStatus(
          `Keyfile parsed: found a seed phrase (${summary.coil_prv_count} private, ` +
            `${summary.coil_pub_count} public keys). Review the filled-in phrase and load it.`,
        );
      } else {
        setStatus(
          'Keyfile parsed, but it contains no seed phrase — only derived keys ' +
            `(${summary.coil_prv_count} private, ${summary.coil_pub_count} public). ` +
            'Nockster slots store BIP39 seeds, so import the original phrase instead.',
        );
      }
    } catch (error: any) {
      setStatus(`Keyfile import failed: ${error?.message ?? String(error)}`);
    }
  };

  const clearTransaction = () => {
    setTx(null);
    setTxInfo(null);
    setTxDetails(null);
    setTxBytes(null);
    setSignedTxBytes(null);
    setStatus('');
  };

  const deviceStateLabel = locked === null ? 'checking' : locked ? 'locked' : 'unlocked';
  const workStateLabel = deviceBusy || seeding || signing || updatingFirmware || fetchingRelease ? 'busy' : 'ready';
  const topStatusLabel = workStateLabel === 'busy' ? 'busy' : connected ? deviceStateLabel : 'offline';
  const firmwareSummaryLabel = info
    ? releaseInfoAvailable
      ? `fw v${info.fw_major}.${info.fw_minor} / rel ${firmwareReleaseVersion === null ? '...' : firmwareReleaseVersion}`
      : `fw v${info.fw_major}.${info.fw_minor}`
    : 'fw ...';

  const saveReleaseIndexSource = () => {
    try {
      configuredReleaseIndexUrl(releaseIndexDraft);
      const trimmed = releaseIndexDraft.trim() || defaultReleaseIndexSource();
      setReleaseIndexSource(trimmed);
      if (trimmed === defaultReleaseIndexSource()) {
        localStorage.removeItem(RELEASE_INDEX_STORAGE_KEY);
      } else {
        localStorage.setItem(RELEASE_INDEX_STORAGE_KEY, trimmed);
      }
      setStatus('Update source saved');
    } catch (error: any) {
      setStatus(error?.message ?? String(error));
    }
  };

  const resetReleaseIndexSource = () => {
    const source = defaultReleaseIndexSource();
    setReleaseIndexSource(source);
    setReleaseIndexDraft(source);
    localStorage.removeItem(RELEASE_INDEX_STORAGE_KEY);
    setStatus('Update source reset');
  };

  const saveSeedLabel = async (slot: number) => {
    const label = (labelDrafts[slot] ?? '').trim();
    if (!seedLabelsAvailable) {
      setStatus('Seed labels are not available on this firmware');
      return;
    }
    if (locked !== false) {
      setStatus('Unlock the device before renaming a wallet slot');
      return;
    }
    if (!validSeedLabel(label)) {
      setStatus(`Nickname must be printable ASCII and at most ${MAX_SEED_LABEL_LEN} bytes`);
      return;
    }

    try {
      setSavingLabelSlot(slot);
      await device.setSeedLabel(slot, label);
      const labels = await device.getSeedLabels();
      setSeedLabels(labels);
      setLabelDrafts((current) => ({ ...current, [slot]: label }));
      setStatus(label ? `Saved nickname for slot ${slot}` : `Cleared nickname for slot ${slot}`);
    } catch (error: any) {
      setStatus(`Nickname save failed: ${error?.message ?? String(error)}`);
    } finally {
      setSavingLabelSlot(null);
    }
  };

  const saveDeviceAddressBookEntries = async (
    entries: DeviceAddressBookEntry[],
    successMessage: string,
  ) => {
    if (!deviceAddressBookAvailable) {
      setAddressBookStatus(info ? 'Update firmware to use the on-device address book' : 'Read device status before editing the address book');
      return;
    }
    if (locked !== false) {
      setAddressBookStatus('Unlock the device before editing the address book');
      return;
    }

    const normalized = entries.map(normalizeDeviceAddressEntry);
    if (normalized.length > MAX_DEVICE_ADDRESS_BOOK_ENTRIES) {
      setAddressBookStatus(`Device address book holds at most ${MAX_DEVICE_ADDRESS_BOOK_ENTRIES} entries`);
      return;
    }
    for (const entry of normalized) {
      if (!validDeviceAddressLabel(entry.label)) {
        setAddressBookStatus(`Labels must be printable ASCII and at most ${MAX_ADDRESS_BOOK_LABEL_LEN} bytes`);
        return;
      }
      if (!validDeviceAddressPkh(entry.pkh)) {
        setAddressBookStatus(`PKHs must be base58 and at most ${MAX_ADDRESS_BOOK_PKH_LEN} chars`);
        return;
      }
    }

    try {
      setSyncingAddressBook(true);
      await device.setAddressBook(normalized);
      const reloaded = await device.getAddressBook();
      setDeviceAddressBook(reloaded);
      setAddressBookStatus(successMessage);
      setWalletPanelView('addresses');
    } catch (error: any) {
      setAddressBookStatus(`Address book save failed: ${error?.message ?? String(error)}`);
    } finally {
      setSyncingAddressBook(false);
    }
  };

  const refreshDeviceAddressBook = async () => {
    if (!deviceAddressBookAvailable) {
      setAddressBookStatus(info ? 'Update firmware to use the on-device address book' : 'Read device status before loading the address book');
      return;
    }
    if (locked !== false) {
      setAddressBookStatus('Unlock the device before reading the address book');
      return;
    }

    try {
      setSyncingAddressBook(true);
      const entries = await device.getAddressBook();
      setDeviceAddressBook(entries);
      setAddressBookStatus(`Loaded ${entries.length} address${entries.length === 1 ? '' : 'es'}`);
    } catch (error: any) {
      setAddressBookStatus(`Address book load failed: ${error?.message ?? String(error)}`);
    } finally {
      setSyncingAddressBook(false);
    }
  };

  const upsertDeviceAddressBookEntry = async (entry: DeviceAddressBookEntry, message: string) => {
    const normalized = normalizeDeviceAddressEntry(entry);
    const existingIndex = deviceAddressBook.findIndex(
      (candidate) => candidate.label.trim().toLowerCase() === normalized.label.toLowerCase()
    );
    if (existingIndex < 0 && deviceAddressBook.length >= MAX_DEVICE_ADDRESS_BOOK_ENTRIES) {
      setAddressBookStatus(`Device address book holds at most ${MAX_DEVICE_ADDRESS_BOOK_ENTRIES} entries`);
      return;
    }

    const next = [...deviceAddressBook];
    if (existingIndex >= 0) {
      next[existingIndex] = normalized;
    } else {
      next.push(normalized);
    }
    await saveDeviceAddressBookEntries(next, message);
  };

  const addDeviceAddressBookEntry = async () => {
    const label = addressBookLabel.trim();
    const pkh = addressBookPkh.trim();
    if (!validDeviceAddressLabel(label)) {
      setAddressBookStatus(`Labels must be printable ASCII and at most ${MAX_ADDRESS_BOOK_LABEL_LEN} bytes`);
      return;
    }
    if (!validDeviceAddressPkh(pkh)) {
      setAddressBookStatus(`PKHs must be base58 and at most ${MAX_ADDRESS_BOOK_PKH_LEN} chars`);
      return;
    }

    await upsertDeviceAddressBookEntry({ label, pkh }, `Saved ${label}`);
    setAddressBookLabel('');
    setAddressBookPkh('');
  };

  const saveWalletAddressToDeviceBook = async (wallet: WalletAddress) => {
    const label = (
      labelDrafts[wallet.slot]?.trim()
      || seedLabelMap.get(wallet.slot)?.trim()
      || wallet.alias.trim()
    )
      || `slot ${wallet.slot}`;
    await upsertDeviceAddressBookEntry(
      { label, pkh: wallet.address },
      `Saved ${label} to device address book`
    );
  };

  const removeDeviceAddressBookEntry = async (index: number) => {
    const entry = deviceAddressBook[index];
    if (!entry) return;
    const next = deviceAddressBook.filter((_, candidateIndex) => candidateIndex !== index);
    await saveDeviceAddressBookEntries(next, `Removed ${entry.label}`);
  };

  return (
    <div className={
      activeTab === 'composer'
        ? 'container container-wide'
        : activeTab === 'device'
          ? 'container container-device'
          : 'container'
    }>
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
      {confirmState && (
        <div className="modal-overlay" onClick={() => resolveConfirm(false)}>
          <div className="modal-card modal-card-confirm" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title">{confirmState.title ?? 'Confirm'}</div>
              <button
                type="button"
                className="toast-close"
                onClick={() => resolveConfirm(false)}
                aria-label="Close"
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <p className="modal-note confirm-message">{confirmState.message}</p>
            </div>
            <div className="modal-actions">
              <button
                type="button"
                className="btn btn-small btn-secondary"
                onClick={() => resolveConfirm(false)}
              >
                {confirmState.cancelLabel ?? 'Cancel'}
              </button>
              <button
                type="button"
                className={`btn btn-small ${confirmState.danger ? 'btn-danger' : 'btn-primary'}`}
                onClick={() => resolveConfirm(true)}
              >
                {confirmState.confirmLabel ?? 'Confirm'}
              </button>
            </div>
          </div>
        </div>
      )}
      {updatesModalOpen && (
        <div className="modal-overlay" onClick={() => setUpdatesModalOpen(false)}>
          <div className="modal-card" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title">Firmware updates</div>
              <button
                type="button"
                className="toast-close"
                onClick={() => setUpdatesModalOpen(false)}
                aria-label="Close"
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <div className="status-item full-width">
                <span className="label">latest available:</span>
                <span className="value">
                  {checkingLatestRelease
                    ? 'checking...'
                    : latestReleaseVersion !== null
                    ? `release ${latestReleaseVersion}`
                    : latestReleaseError ?? 'unknown'}
                </span>
              </div>
              {connected && releaseInfoAvailable && (
                <div className="status-item full-width">
                  <span className="label">installed:</span>
                  <span className="value">
                    {firmwareReleaseVersion === null ? '...' : `release ${firmwareReleaseVersion}`}
                  </span>
                </div>
              )}
              <p className="modal-note">
                {connected && updateUpToDate
                  ? 'Your device is already on the latest release.'
                  : connected && updateIsNewer
                  ? 'A newer release is available. Installing writes it to the inactive OTA slot and verifies it on-device before activation.'
                  : 'Connecting reads the version installed on your device and installs the latest signed firmware only if it is newer.'}
              </p>
            </div>
            <div className="modal-actions">
              <button
                type="button"
                className="btn btn-small btn-secondary"
                onClick={() => void checkLatestReleaseVersion()}
                disabled={checkingLatestRelease}
              >
                recheck
              </button>
              <button
                type="button"
                className="btn btn-small btn-primary"
                onClick={installLatestUpdate}
                disabled={
                  !isSupported ||
                  updatingFirmware ||
                  fetchingRelease ||
                  deviceBusy ||
                  (connected && updateUpToDate)
                }
              >
                {updatingFirmware || fetchingRelease
                  ? 'updating...'
                  : connected && updateUpToDate
                  ? 'up to date'
                  : connected
                  ? 'install latest'
                  : 'connect & install latest'}
              </button>
            </div>
          </div>
        </div>
      )}
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
          <div className="composer-title-row">
            <div>
              <h2>Transaction composer</h2>
              <p className="seed-subtitle">
                Build a V1 draft from synced notes, then sign it on the connected device.
              </p>
            </div>
          </div>
          <Suspense fallback={<div className="status-message">loading composer...</div>}>
            <ComposerView
              wasmReady={wasmReady}
              walletAddresses={walletAddresses}
              deviceAddressBook={deviceAddressBook}
              onSignDraft={signComposerDraft}
              canSignDraft={canSignComposerDraft}
              signingDraft={signing}
              signDraftDisabledReason={composerSignDisabledReason}
            />
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
        <div className="device-page device-page-connected">
          <div className="device-topbar">
            <div className="device-title">
              <span className="device-eyebrow">Nockster</span>
              <h2>Device console</h2>
            </div>
            <div className="device-metrics" aria-label="Device summary">
              {loadingDevice && (
                <span className="device-pill device-pill-loading" aria-live="polite">
                  <span className="spinner" aria-hidden="true" />
                  syncing
                </span>
              )}
              <span className={`device-pill ${locked === false && workStateLabel !== 'busy' ? 'device-pill-good' : topStatusLabel !== 'offline' ? 'device-pill-warn' : ''}`}>
                {topStatusLabel}
              </span>
              <span className="device-pill">{firmwareSummaryLabel}</span>
            </div>
          </div>
          {(deviceKeys.length > 0 || info?.has_seed) && (
            <div className="section device-panel device-wallet-panel">
              <div className="device-panel-header">
                <div>
                  <h2>Wallet</h2>
                  <div className="wallet-panel-tabs" role="tablist" aria-label="Wallet panel">
                    <button
                      type="button"
                      role="tab"
                      aria-selected={walletPanelView === 'slots'}
                      className={`wallet-panel-tab ${walletPanelView === 'slots' ? 'active' : ''}`}
                      onClick={() => setWalletPanelView('slots')}
                    >
                      slots
                    </button>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={walletPanelView === 'addresses'}
                      className={`wallet-panel-tab ${walletPanelView === 'addresses' ? 'active' : ''}`}
                      onClick={() => setWalletPanelView('addresses')}
                    >
                      addresses{deviceAddressBook.length ? ` ${deviceAddressBook.length}` : ''}
                    </button>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={walletPanelView === 'vault'}
                      className={`wallet-panel-tab ${walletPanelView === 'vault' ? 'active' : ''}`}
                      onClick={() => setWalletPanelView('vault')}
                    >
                      vault{vaultEntries?.length ? ` ${vaultEntries.length}` : ''}
                    </button>
                  </div>
                </div>
                <div className="device-panel-actions">
                  {walletPanelView === 'slots' ? (
                    <>
                      {showSeedForm && !isInitialSeed && (
                        <button
                          type="button"
                          onClick={() => setAddSeedExpanded((prev) => !prev)}
                          className="btn btn-small btn-secondary"
                        >
                          {addSeedExpanded ? 'hide seed' : 'add seed'}
                        </button>
                      )}
                      {deviceNockblocksKey.trim() && (
                        <button
                          type="button"
                          onClick={() => void refreshWalletBalances(false)}
                          disabled={syncingBalances}
                          className="btn btn-small btn-secondary"
                        >
                          {syncingBalances ? 'balances...' : 'balances'}
                        </button>
                      )}
                    </>
                  ) : walletPanelView === 'addresses' ? (
                    <button
                      type="button"
                      onClick={() => void refreshDeviceAddressBook()}
                      className="btn btn-small btn-secondary"
                      disabled={!deviceAddressBookAvailable || locked !== false || syncingAddressBook}
                    >
                      {syncingAddressBook ? 'loading...' : 'refresh'}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void refreshVault()}
                      className="btn btn-small btn-secondary"
                      disabled={vaultBusy || locked !== false}
                    >
                      {vaultBusy ? 'working...' : vaultEntries === null ? 'load' : 'refresh'}
                    </button>
                  )}
                </div>
              </div>
              {deviceKeys.length === 0 ? (
                <div className="device-empty-state">
                  {locked === false && loadingDevice ? (
                    <span className="device-loading-line">
                      <span className="spinner" aria-hidden="true" />
                      Loading wallet…
                    </span>
                  ) : (
                    'Unlock to view wallet slots.'
                  )}
                </div>
              ) : walletPanelView === 'slots' ? (
                <>
                  {!deviceNockblocksKey.trim() && (
                    <div className="nb-key-callout">
                      <div className="nb-key-callout-text">
                        <strong>Connect Nockblocks to see balances</strong>
                        <span>
                          A Nockblocks API key is required to load wallet balances and notes.
                          Paste yours below, or grab one at nockblocks.com.
                        </span>
                      </div>
                      <div className="nb-key-callout-form">
                        <input
                          type="password"
                          className="input"
                          value={nockblocksKeyDraft}
                          onChange={(e) => setNockblocksKeyDraft(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') saveNockblocksKey();
                          }}
                          placeholder="nockblocks api key"
                          autoComplete="off"
                          spellCheck={false}
                        />
                        <button
                          type="button"
                          className="btn btn-small btn-primary"
                          onClick={saveNockblocksKey}
                          disabled={!nockblocksKeyDraft.trim()}
                        >
                          save key
                        </button>
                        <a
                          className="nb-key-link"
                          href="https://nockblocks.com"
                          target="_blank"
                          rel="noreferrer"
                        >
                          get a key ↗
                        </a>
                      </div>
                    </div>
                  )}
                  <div className="wallet-slot-grid">
                    {slotSummary.map((pub) => {
                      const storedLabel = seedLabelMap.get(pub.slot) ?? '';
                      const draftLabel = labelDrafts[pub.slot] ?? storedLabel;
                      const wallet = walletBySlot.get(pub.slot);
                      const walletAddress = wallet?.address ?? deriveDevicePkh(pub);
                      const addressReady = !!walletAddress;
                      const displayAddress = walletAddress ?? 'deriving address…';
                      const balance = slotBalances[pub.slot];
                      const balanceText = !deviceNockblocksKey.trim()
                        ? ''
                        : balance?.status === 'ok'
                          ? `${formatNicksCompact(balance.nicks ?? 0)} · ${balance.notes ?? 0} notes`
                          : balance?.status === 'error'
                            ? 'balance unavailable'
                            : syncingBalances ? 'loading balance...' : 'balance not loaded';
                      return (
                        <div
                          key={pub.slot}
                          className={`wallet-slot-card ${selectedSlot === pub.slot ? 'active' : ''}`}
                        >
                          <div className="wallet-slot-head">
                            <div>
                              <div className="wallet-slot-title">
                                {storedLabel || `slot ${pub.slot}`}
                              </div>
                              <div className="path-tag">{formatDerivationPath(pub.path)}</div>
                            </div>
                            <button
                              type="button"
                              onClick={() => handleSlotChange(pub.slot)}
                              disabled={deviceBusy || locked !== false || selectedSlot === pub.slot}
                              className="btn btn-small btn-secondary"
                            >
                              {selectedSlot === pub.slot ? 'active' : 'select'}
                            </button>
                          </div>
                          {selectedSlot === pub.slot && (
                          <div className="wallet-slot-label-row">
                            <input
                              className="input wallet-label-input"
                              value={draftLabel}
                              placeholder="nickname"
                              maxLength={MAX_SEED_LABEL_LEN}
                              disabled={!seedLabelsAvailable || locked !== false || savingLabelSlot === pub.slot}
                              onChange={(event) => {
                                const next = event.target.value;
                                setLabelDrafts((current) => ({ ...current, [pub.slot]: next }));
                              }}
                              onKeyDown={(event) => {
                                if (event.key === 'Enter') {
                                  void saveSeedLabel(pub.slot);
                                }
                              }}
                            />
                            <button
                              type="button"
                              className="btn btn-small btn-secondary"
                              disabled={
                                !seedLabelsAvailable ||
                                locked !== false ||
                                savingLabelSlot === pub.slot ||
                                draftLabel.trim() === storedLabel
                              }
                              onClick={() => void saveSeedLabel(pub.slot)}
                            >
                              {savingLabelSlot === pub.slot ? 'saving...' : 'save'}
                            </button>
                          </div>
                          )}
                          <div className="wallet-slot-address">
                            <span className={`pubkey-text ${addressReady ? '' : 'pubkey-pending'}`}>{displayAddress}</span>
                            <button
                              type="button"
                              onClick={() => {
                                navigator.clipboard.writeText(displayAddress);
                                setStatus(`Copied slot ${pub.slot} address to clipboard`);
                              }}
                              className="btn btn-small copy-btn"
                              disabled={!addressReady}
                            >
                              copy
                            </button>
                          </div>
                          {balanceText && (
                            <div className={`wallet-slot-balance ${balance?.status === 'error' ? 'error' : ''}`}>
                              {balanceText}
                              {balance?.status === 'error' && balance.error ? `: ${balance.error}` : ''}
                            </div>
                          )}
                          {selectedSlot === pub.slot && (
                          <div className="wallet-slot-actions">
                            <button
                              type="button"
                              onClick={() => {
                                if (!walletAddress) {
                                  setStatus('Wallet PKH is not ready yet');
                                  return;
                                }
                                void verifyReceiveAddress(wallet ?? {
                                  slot: pub.slot,
                                  path: pub.path,
                                  pathLabel: formatDerivationPath(pub.path),
                                  address: walletAddress,
                                  alias: storedLabel || `wallet slot ${pub.slot}`,
                                });
                              }}
                              className="btn btn-small btn-secondary"
                              disabled={deviceBusy || locked !== false || !walletAddress}
                            >
                              verify on device
                            </button>
                            <button
                              type="button"
                              onClick={() =>
                                void signMessageWithSlot(wallet ?? {
                                  slot: pub.slot,
                                  path: pub.path,
                                  pathLabel: formatDerivationPath(pub.path),
                                  address: walletAddress ?? '',
                                  alias: storedLabel || `wallet slot ${pub.slot}`,
                                })
                              }
                              className="btn btn-small btn-secondary"
                              disabled={deviceBusy || locked !== false}
                              title="Sign an arbitrary message with this slot's key (reviewed on-device)"
                            >
                              sign message
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                if (!walletAddress) {
                                  setAddressBookStatus('Wallet PKH is not ready yet');
                                  return;
                                }
                                void saveWalletAddressToDeviceBook(wallet ?? {
                                  slot: pub.slot,
                                  path: pub.path,
                                  pathLabel: formatDerivationPath(pub.path),
                                  address: walletAddress,
                                  alias: storedLabel || `wallet slot ${pub.slot}`,
                                });
                              }}
                              className="btn btn-small btn-secondary"
                              disabled={
                                !deviceAddressBookAvailable ||
                                locked !== false ||
                                syncingAddressBook ||
                                !walletAddress
                              }
                            >
                              save to book
                            </button>
                            <button
                              type="button"
                              onClick={() => void exportWatchOnly(pub.slot, storedLabel)}
                              className="btn btn-small btn-secondary"
                              disabled={deviceBusy || locked !== false || !wasmReady}
                              title="Export master pubkey + chain code for nockchain-wallet watch-only import (confirmed on device)"
                            >
                              export watch-only
                            </button>
                            <button
                              type="button"
                              onClick={() => deleteSeedSlot(pub.slot)}
                              className="btn btn-small btn-danger"
                              disabled={deviceBusy || deletingSlot === pub.slot || seeding || signing}
                            >
                              {deletingSlot === pub.slot ? 'removing...' : 'remove'}
                            </button>
                          </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {balanceStatus && <div className="device-inline-status">{balanceStatus}</div>}
                </>
              ) : walletPanelView === 'addresses' ? (
                !deviceAddressBookAvailable ? (
                <div className="device-empty-state">
                  {info ? 'Address book unavailable on this firmware.' : 'Reading device status...'}
                </div>
              ) : locked !== false ? (
                <div className="device-empty-state">Unlock to edit device addresses.</div>
              ) : (
                <>
                  <div className="device-address-form device-address-form-compact">
                    <input
                      className="input"
                      value={addressBookLabel}
                      maxLength={MAX_ADDRESS_BOOK_LABEL_LEN}
                      placeholder="label"
                      disabled={syncingAddressBook}
                      onChange={(event) => setAddressBookLabel(event.target.value)}
                    />
                    <input
                      className="input"
                      value={addressBookPkh}
                      maxLength={MAX_ADDRESS_BOOK_PKH_LEN}
                      placeholder="pkh"
                      disabled={syncingAddressBook}
                      spellCheck={false}
                      onChange={(event) => setAddressBookPkh(event.target.value)}
                    />
                    <button
                      type="button"
                      className="btn btn-small btn-primary"
                      onClick={() => void addDeviceAddressBookEntry()}
                      disabled={syncingAddressBook}
                    >
                      save
                    </button>
                  </div>
                  {deviceAddressBook.length === 0 ? (
                    <div className="device-empty-state">No saved addresses.</div>
                  ) : (
                    <div className="device-address-list device-address-list-compact">
                      {deviceAddressBook.map((entry, index) => (
                        <div className="device-address-row" key={`${entry.label}:${entry.pkh}:${index}`}>
                          <div className="device-address-main">
                            <strong>{entry.label}</strong>
                            <button
                              type="button"
                              className="pubkey-text pubkey-copy"
                              onClick={() => {
                                navigator.clipboard.writeText(entry.pkh);
                                setStatus(`Copied ${entry.label} address`);
                              }}
                            >
                              {entry.pkh}
                            </button>
                          </div>
                          <button
                            type="button"
                            className="btn btn-small btn-danger"
                            onClick={() => void removeDeviceAddressBookEntry(index)}
                            disabled={syncingAddressBook}
                          >
                            remove
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </>
                )
              ) : locked !== false ? (
                <div className="device-empty-state">Unlock to use the vault.</div>
              ) : (
                <>
                  <p className="seed-subtitle">
                    Encrypted on-device storage for %hax lock preimages (HTLC secrets,
                    commit-reveal values). The device computes each Tip5 commitment itself and
                    confirms store and reveal on-screen.
                  </p>
                  {vaultEntries !== null && (
                    vaultEntries.length === 0 ? (
                      <div className="device-empty-state">No stored secrets.</div>
                    ) : (
                      <div className="device-address-list device-address-list-compact">
                        {vaultEntries.map((entry) => (
                          <div className="device-address-row" key={entry.slot}>
                            <div className="device-address-main">
                              <strong>{entry.label || `slot ${entry.slot}`}</strong>
                              <button
                                type="button"
                                className="pubkey-text pubkey-copy"
                                onClick={() => {
                                  const b58 = vaultCommitmentB58(entry.commitment);
                                  navigator.clipboard.writeText(b58);
                                  setVaultStatus('Copied commitment');
                                }}
                                title="Copy commitment (the %hax lock value)"
                              >
                                {vaultCommitmentB58(entry.commitment)}
                              </button>
                            </div>
                            <button
                              type="button"
                              className="btn btn-small btn-secondary"
                              onClick={() => void revealVaultSecret(entry)}
                              disabled={vaultBusy}
                            >
                              reveal
                            </button>
                            <button
                              type="button"
                              className="btn btn-small btn-danger"
                              onClick={() => void deleteVaultSecret(entry)}
                              disabled={vaultBusy}
                            >
                              delete
                            </button>
                          </div>
                        ))}
                      </div>
                    )
                  )}
                  {vaultRevealed && (
                    <div className="device-inline-status vault-revealed">
                      <div>
                        <strong>{vaultRevealed.label || `slot ${vaultRevealed.slot}`}</strong>{' '}
                        revealed{vaultRevealed.atomHex === null ? ' (cell noun, jam shown)' : ''}:
                      </div>
                      <div className="pubkey-text">
                        {vaultRevealed.atomHex ?? vaultRevealed.jamHex}
                      </div>
                      <div className="seed-actions">
                        <button
                          type="button"
                          className="btn btn-small btn-secondary"
                          onClick={() => {
                            navigator.clipboard.writeText(
                              vaultRevealed.atomHex ?? vaultRevealed.jamHex,
                            );
                            setVaultStatus('Copied secret');
                          }}
                        >
                          copy
                        </button>
                        <button
                          type="button"
                          className="btn btn-small btn-secondary"
                          onClick={() =>
                            downloadBytesAs(
                              `preimage-${vaultRevealed.label || vaultRevealed.slot}.jam`,
                              hexToBytes(vaultRevealed.jamHex),
                            )
                          }
                        >
                          download .jam
                        </button>
                        <button
                          type="button"
                          className="btn btn-small btn-secondary"
                          onClick={() => setVaultRevealed(null)}
                        >
                          hide
                        </button>
                      </div>
                    </div>
                  )}
                  <div className="device-address-form device-address-form-compact">
                    <input
                      className="input"
                      value={vaultLabel}
                      maxLength={32}
                      placeholder="label"
                      disabled={vaultBusy}
                      onChange={(event) => setVaultLabel(event.target.value)}
                    />
                    <input
                      className="input"
                      value={vaultSecretHex}
                      placeholder={vaultInputIsJam ? 'jammed noun (hex)' : 'secret bytes (hex)'}
                      disabled={vaultBusy}
                      spellCheck={false}
                      onChange={(event) => setVaultSecretHex(event.target.value)}
                    />
                    <button
                      type="button"
                      className="btn btn-small btn-primary"
                      onClick={() => void storeVaultSecret()}
                      disabled={vaultBusy || !vaultSecretHex.trim() || !wasmReady}
                    >
                      store
                    </button>
                  </div>
                  <label className="seed-hint vault-jam-toggle">
                    <input
                      type="checkbox"
                      checked={vaultInputIsJam}
                      onChange={(event) => setVaultInputIsJam(event.target.checked)}
                      disabled={vaultBusy}
                    />{' '}
                    input is already a jammed noun (otherwise bytes are wrapped as an atom)
                  </label>
                </>
              )}
              {walletPanelView === 'addresses' && addressBookStatus && (
                <div className="device-inline-status">{addressBookStatus}</div>
              )}
              {walletPanelView === 'vault' && vaultStatus && (
                <div className="device-inline-status">{vaultStatus}</div>
              )}
            </div>
          )}

          {showSeedForm && (isInitialSeed || addSeedExpanded) && (
            <div className="section device-panel device-seed-panel">
              <div className="seed-header">
                <h2>{isInitialSeed ? 'Load a seed' : 'Add a seed slot'}</h2>
                {!isInitialSeed && (
                  <button
                    type="button"
                    onClick={() => setAddSeedExpanded((prev) => !prev)}
                    className="btn btn-small btn-secondary seed-toggle"
                  >
                    close
                  </button>
                )}
              </div>

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
                <div className="seed-hint">
                  Have a nockchain-wallet <code>keys.export</code> file?{' '}
                  <label className="keyfile-import-label">
                    <input
                      type="file"
                      style={{ display: 'none' }}
                      disabled={deviceBusy || seeding || !wasmReady}
                      onChange={(e) => {
                        const file = e.target.files?.[0];
                        if (file) void importWalletKeyfile(file);
                        e.target.value = '';
                      }}
                    />
                    <span className="keyfile-import-link">import it</span>
                  </label>{' '}
                  to fill in the seed phrase it contains.
                </div>
                {!isInitialSeed && (
                  <div className="seed-hint">
                    Device uses your existing PIN. Unlock it in the control section before adding a seed.
                  </div>
                )}
              </div>
            </div>
          )}

          {status && (
            <div className="status-message device-status-message">
              {status}
            </div>
          )}

          <div className="device-controls-row">
          <div className="section device-panel device-overview-panel">
            <h2>Status</h2>
            <div className="status-grid">
              <div className="status-item">
                <span className="label">lock status:</span>
                <span className={`value ${locked === true ? 'locked' : locked === false ? 'unlocked' : ''}`}>
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
                    <span className="label">seed:</span>
                    <span className="value">{info.has_seed ? 'loaded' : 'empty'}</span>
                  </div>
                  {securityStatusAvailable && securityStatus && (
                    <>
                      <div className="status-item">
                        <span className="label">storage:</span>
                        <span className="value">
                          {securityStatus.nvs_initialized ? 'initialized' : 'empty'}
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
                </>
              )}
            </div>
            <button onClick={() => refreshStatus()} disabled={deviceBusy} className="btn btn-small">
              refresh status
            </button>
          </div>

          <div className="section device-panel device-control-panel">
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
                </div>
              </div>
            )}
          </div>
          </div>{/* device-controls-row */}

          <NounInspector wasm={wasm} />

          <div className="section device-panel device-update-panel">
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
                    !updateBootStatusAvailable ||
                    updateUpToDate
                  }
                  className="btn btn-small btn-primary seed-toggle"
                >
                  {updatingFirmware || fetchingRelease
                    ? 'updating...'
                    : updateUpToDate
                    ? 'up to date'
                    : updateIsNewer
                    ? `update to rel ${latestReleaseVersion}`
                    : 'update firmware'}
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
              <div className="status-item full-width">
                <span className="label">latest available:</span>
                <span className="value">
                  {checkingLatestRelease
                    ? 'checking...'
                    : latestReleaseVersion !== null
                    ? `release ${latestReleaseVersion}${updateUpToDate ? ' (up to date)' : updateIsNewer ? ' (newer)' : ''}`
                    : latestReleaseError ?? 'unknown'}
                </span>
              </div>
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
                <span className="value update-hash">
                  {!secureUpdateAvailable
                    ? 'unavailable'
                    : !updateTrustHash
                      ? 'not configured'
                      : (
                        <>
                          {updateTrustHash.toLowerCase() === OFFICIAL_TRUST_ANCHOR && (
                            <span className="trust-verified" title="Official SWPSCo signing key">
                              ✓ SWPSCo!
                            </span>
                          )}
                          {updateTrustHash}
                        </>
                      )}
                </span>
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
                <details className="device-subdetails">
                  <summary>Update source</summary>
                  <div className="device-subdetails-body">
                    <label className="file-control">
                      <span>latest release index</span>
                      <input
                        type="url"
                        value={releaseIndexDraft}
                        disabled={deviceBusy || updatingFirmware || fetchingRelease}
                        onChange={(e) => setReleaseIndexDraft(e.target.value)}
                        className="input"
                        autoComplete="off"
                        spellCheck={false}
                      />
                    </label>
                    <div className="status-item full-width">
                      <span className="label">active:</span>
                      <span className="value update-hash">{latestReleaseIndexLabel}</span>
                    </div>
                    <div className="button-group update-source-actions">
                      <button
                        type="button"
                        onClick={saveReleaseIndexSource}
                        disabled={deviceBusy || updatingFirmware || fetchingRelease}
                        className="btn btn-small btn-secondary"
                      >
                        save source
                      </button>
                      <button
                        type="button"
                        onClick={resetReleaseIndexSource}
                        disabled={deviceBusy || updatingFirmware || fetchingRelease}
                        className="btn btn-small btn-secondary"
                      >
                        reset source
                      </button>
                    </div>
                  </div>
                </details>
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
            <div className="section device-panel device-signing-panel">
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
        </div>
      )}

      {activeTab === 'device' && !connected && (
        <div className="device-page device-page-disconnected">
          <div className="device-topbar">
            <div className="device-title">
              <span className="device-eyebrow">Nockster</span>
              <h2>Device console</h2>
            </div>
            <div className="device-metrics" aria-label="Device summary">
              <span className="device-pill device-pill-warn">offline</span>
            </div>
          </div>
          <div className="section connect-panel device-panel device-connect-panel">
            <div className="connect-hero-art" aria-hidden="true">
              <span className="connect-spark connect-spark-a">✦</span>
              <span className="connect-spark connect-spark-b">✦</span>
              <span className="connect-spark connect-spark-c">✦</span>
              <div className="connect-coin">
                <span className="connect-coin-face">N</span>
                <span className="connect-status-dot">offline</span>
              </div>
            </div>
            <div className="connect-hero-body connect-main">
              <span className="connect-eyebrow">Hardware wallet</span>
              <h2 className="connect-hero-title">Plug in your Nockster.</h2>
              <p className="connect-hero-text">
                Connect over USB to manage seeds, check
                balances, and sign transactions.
              </p>
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
              <div className="connect-cta-row">
                <button
                  onClick={connect}
                  className="btn btn-primary"
                  disabled={!isSupported || deviceBusy || updatingFirmware || fetchingRelease || (isTauri && !selectedPort)}
                >
                  {deviceBusy ? 'connecting...' : 'connect device'}
                </button>
                <button
                  onClick={() => {
                    setUpdatesModalOpen(true);
                    void checkLatestReleaseVersion();
                  }}
                  className="btn btn-secondary"
                  disabled={updatingFirmware || fetchingRelease || deviceBusy}
                >
                  {updatingFirmware || fetchingRelease ? 'updating...' : 'firmware updates'}
                </button>
              </div>
              <span className={`connect-hint ${isSupported ? 'ready' : ''}`}>
                {isSupported
                  ? 'Web Serial ready · Chrome, Edge, or Opera'
                  : 'Requires Chrome, Edge, or Opera (Web Serial).'}
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
