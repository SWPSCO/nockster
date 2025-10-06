import { useState, useEffect, useRef } from 'react';
import { SigerDevice, formatCheetahPubkey } from 'siger-js';
import type { Response } from 'siger-js';
import { mnemonicToSeed, validateMnemonicWords, isValidMnemonicWordCount } from './bip39';
import init, { ParsedTransaction } from 'siger-wasm';
import './App.css';

type DeviceKey = { slot: number; path: number[]; x: bigint[]; y: bigint[] };
type InputDeviceKey = { slot: number; path: number[] };
type InfoResponse = Extract<Response, { type: 'Info' }>;

function App() {
  const [device] = useState(() => new SigerDevice());
  const [connected, setConnected] = useState(false);
  const [status, setStatus] = useState<string>('');
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
  const [pinResetNew, setPinResetNew] = useState('');
  const [pinResetConfirm, setPinResetConfirm] = useState('');
  const [resettingPin, setResettingPin] = useState(false);

  // Transaction signing state
  const [wasmReady, setWasmReady] = useState(false);
  const [tx, setTx] = useState<ParsedTransaction | null>(null);
  const [txInfo, setTxInfo] = useState<any>(null);
  const [txDetails, setTxDetails] = useState<any>(null);
  const [signingInputs, setSigningInputs] = useState<any[]>([]);
  const [signing, setSigning] = useState(false);
  const [signedTxBytes, setSignedTxBytes] = useState<Uint8Array | null>(null);

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
  const isSupported = SigerDevice.isSupported();
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

  const connect = async () => {
    try {
      setStatus('Connecting...');
      await device.connect();
      setConnected(true);
      setStatus('Connected!');
      await sleep(1000);
      await refreshStatus();
    } catch (error: any) {
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

  useEffect(() => {
    if (!connected) return;

    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      if (cancelled) return;
      if (!refreshingRef.current) {
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
      setStatus('Unlocking...');
      await device.unlock(pin);
      setStatus('Unlocked successfully!');
      setPin('');
      await refreshStatus();
    } catch (error: any) {
      setStatus(`Unlock failed: ${error.message}`);
      await refreshStatus(); // Refresh attempts remaining
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
    }
  };

  const resetPin = async () => {
    const current = pinResetCurrent.trim();
    const next = pinResetNew.trim();
    const confirm = pinResetConfirm.trim();

    if (locked !== false) {
      setStatus('Unlock the device before resetting the PIN');
      return;
    }
    if (!current) {
      setStatus('Enter the current PIN');
      return;
    }
    if (!next) {
      setStatus('Enter a new PIN');
      return;
    }
    if (next !== confirm) {
      setStatus('New PIN entries do not match');
      return;
    }

    try {
      setResettingPin(true);
      setStatus('Updating device PIN...');
      await device.resetPIN(current, next);
      setStatus('Device PIN updated successfully');
      setPinResetCurrent('');
      setPinResetNew('');
      setPinResetConfirm('');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`PIN update failed: ${message}`);
    } finally {
      setResettingPin(false);
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
      setStatus(`Loaded transaction: ${info.tx_id} (${info.input_count} inputs)`);

      // Don't automatically find signing inputs - wait for user to click "prepare to sign"
    } catch (error: any) {
      console.error('Transaction load error:', error);
      console.error('Error stack:', error.stack);
      setStatus(`Failed to load transaction: ${error.message || error.toString()}`);
    }
  };

  const signTransaction = async () => {
    if (!tx || !connected || locked) {
      setStatus('Device must be connected and unlocked');
      return;
    }

    try {
      setSigning(true);
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

      setStatus('Applying signatures in browser...');
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

      setStatus(`✓ Signed ${signatures.length} signature(s) and downloaded ${filename}`);
    } catch (error: any) {
      console.error('Signing error:', error);
      const errorMsg = error.message || error.toString() || 'Unknown error';
      setStatus(`Signing failed: ${errorMsg}`);
    } finally {
      setSigning(false);
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
    setSigningInputs([]);
    setSignedTxBytes(null);
    setStatus('');
  };

  if (!isSupported) {
    return (
      <div className="container">
        <h1>Siger hardware wallet</h1>
        <div className="error">
          <p>Web Serial API not supported in this browser.</p>
          <p>Please use Chrome, Edge, or Opera.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="container">
      <h1>Siger hardware wallet</h1>

      {connected && (
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
                      ? 'Device ready to seed. Make sure your keys are written down somewhere safe.'
                      : 'Add another BIP39 seed to this device. Keep it unlocked; no new PIN required.'}
                  </p>
                  <div className="seed-form">
                    <textarea
                      className="input mnemonic-input"
                      value={mnemonic}
                      onChange={(e) => setMnemonic(e.target.value)}
                      placeholder="twelve or twenty-four words, separated by spaces"
                      spellCheck={false}
                      disabled={seeding}
                    />
                    {isInitialSeed && (
                      <input
                        type="password"
                        className="input pin-input"
                        value={seedPin}
                        onChange={(e) => setSeedPin(e.target.value)}
                        placeholder="set a device PIN"
                        disabled={seeding}
                        autoComplete="off"
                      />
                    )}
                    <input
                      type="text"
                      className="input passphrase-input"
                      value={seedPassphrase}
                      onChange={(e) => setSeedPassphrase(e.target.value)}
                      placeholder="optional bip39 passphrase"
                      disabled={seeding}
                    />
                    <div className="seed-actions">
                      <button
                        type="button"
                        onClick={seedDevice}
                        disabled={seeding || !canSubmitSeed}
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
                        disabled={seeding}
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
                  onKeyPress={(e) => {
                    if (e.key === 'Enter' && locked) {
                      unlock();
                    }
                  }}
                />
              )}
              <div className="button-group" style={{ gridTemplateColumns: 'repeat(5, 1fr)' }}>
                <button
                  onClick={unlock}
                  disabled={!locked || !pin}
                  className="btn btn-success"
                >
                  unlock
                </button>
                <button
                  onClick={lock}
                  disabled={locked || locked === null}
                  className="btn btn-warning"
                >
                  lock
                </button>
                <button onClick={ping} className="btn btn-small">
                  test
                </button>
                <button
                  onClick={resetDevice}
                  disabled={seeding || signing}
                  className="btn btn-danger"
                >
                  reset
                </button>
                <button onClick={disconnect} className="btn btn-secondary">
                  disconnect
                </button>
              </div>
            </div>
            {info?.has_seed && (
              <div className="reset-pin">
                <h3>Reset PIN</h3>
                <p className="reset-pin-note">Requires the device to be unlocked.</p>
                <div className="reset-pin-grid">
                  <input
                    type="password"
                    className="input"
                    value={pinResetCurrent}
                    onChange={(e) => setPinResetCurrent(e.target.value)}
                    placeholder="current PIN"
                    disabled={resettingPin}
                    autoComplete="off"
                  />
                  <input
                    type="password"
                    className="input"
                    value={pinResetNew}
                    onChange={(e) => setPinResetNew(e.target.value)}
                    placeholder="new PIN"
                    disabled={resettingPin}
                    autoComplete="off"
                  />
                  <input
                    type="password"
                    className="input"
                    value={pinResetConfirm}
                    onChange={(e) => setPinResetConfirm(e.target.value)}
                    placeholder="confirm new PIN"
                    disabled={resettingPin}
                    autoComplete="off"
                  />
                </div>
                <div className="reset-pin-actions">
                  <button
                    type="button"
                    onClick={() => {
                      setPinResetCurrent('');
                      setPinResetNew('');
                      setPinResetConfirm('');
                    }}
                    disabled={resettingPin}
                    className="btn btn-secondary btn-small"
                  >
                    clear
                  </button>
                  <button
                    type="button"
                    onClick={resetPin}
                    disabled={
                      resettingPin ||
                      locked !== false ||
                      !pinResetCurrent.trim() ||
                      !pinResetNew.trim() ||
                      pinResetNew !== pinResetConfirm
                    }
                    className="btn btn-success btn-small"
                  >
                    {resettingPin ? 'updating...' : 'update PIN'}
                  </button>
                </div>
              </div>
            )}
          </div>

          <div className="section">
            <h2>Device status</h2>
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
                  <div className="status-item">
                    <span className="label">has seed:</span>
                    <span className="value">{info.has_seed ? 'yes' : 'no'}</span>
                  </div>
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
                                <span className="pubkey-text">{formatCheetahPubkey(pub.x, pub.y)}</span>
                                <button
                                  onClick={() => {
                                    navigator.clipboard.writeText(formatCheetahPubkey(pub.x, pub.y));
                                    setStatus(
                                      `Copied slot ${pub.slot} ${formatDerivationPath(pub.path)} to clipboard`
                                    );
                                  }}
                                  className="btn btn-small copy-btn"
                                >
                                  copy
                                </button>
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
            <button onClick={() => refreshStatus()} className="btn btn-small">
              refresh status
            </button>
          </div>

          {wasmReady && (
            <div className="section">
              <h2>Transaction signing</h2>

              {!tx ? (
                <div>
                  <p>Upload a transaction draft (.draft file) to sign</p>
                  <input
                    type="file"
                    accept=".draft"
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
                          disabled={signing || locked === true}
                          className="btn btn-success"
                        >
                          {signing ? 'signing...' : 'sign transaction'}
                        </button>
                        <button onClick={clearTransaction} className="btn btn-secondary">
                          clear
                        </button>
                      </>
                    ) : (
                      <>
                        <button onClick={downloadSignedTx} className="btn btn-success">
                          download signed .tx
                        </button>
                        <button onClick={clearTransaction} className="btn btn-secondary">
                          sign another
                        </button>
                      </>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}

          {status && (
            <div className="status-message">
              {status}
            </div>
          )}
        </>
      )}

      {!connected && (
        <div className="section">
          <div style={{ display: 'flex', justifyContent: 'center' }}>
            <button onClick={connect} className="btn btn-primary">
              connect device
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
