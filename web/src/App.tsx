import { useState, useEffect, useRef } from 'react';
import { SigerDevice, formatCheetahPubkey } from 'siger-js';
import { mnemonicToSeed, validateMnemonicWords } from './bip39';
import init, { ParsedTransaction } from 'siger-wasm';
import './App.css';

function App() {
  const [device] = useState(() => new SigerDevice());
  const [connected, setConnected] = useState(false);
  const [status, setStatus] = useState<string>('');
  const [locked, setLocked] = useState<boolean | null>(null);
  const [attemptsRemaining, setAttemptsRemaining] = useState<number | null>(null);
  const [pin, setPin] = useState('');
  const [info, setInfo] = useState<any>(null);
  const [mnemonic, setMnemonic] = useState('');
  const [seedPassphrase, setSeedPassphrase] = useState('');
  const [seedPin, setSeedPin] = useState('');
  const [seeding, setSeeding] = useState(false);

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
  const readyToSeed = connected && locked === false && info?.has_seed === false;
  const hasSeedPin = seedPin.trim().length > 0;

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
      setStatus('Disconnected');
    } catch (error: any) {
      setStatus(`Disconnect failed: ${error.message}`);
    }
  };

  const refreshStatus = async () => {
    try {
      const lockStatus = await device.getLockStatus();
      setLocked(lockStatus.locked);
      setAttemptsRemaining(lockStatus.attempts_remaining);

      const deviceInfo = await device.getInfo();
      setInfo(deviceInfo);
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

  const seedDevice = async () => {
    const trimmedMnemonic = mnemonic.trim();
    const trimmedPin = seedPin.trim();
    try {
      if (!readyToSeed) {
        throw new Error('Connect and unlock a blank device first');
      }
      validateMnemonicWords(trimmedMnemonic);
      if (!trimmedPin) {
        throw new Error('Enter a device PIN before seeding');
      }
      setSeeding(true);
      setStatus('Seeding device...');
      const seed = await mnemonicToSeed(trimmedMnemonic, seedPassphrase);
      await device.initializePIN(trimmedPin, seed);
      await refreshStatus();
      setMnemonic('');
      setSeedPassphrase('');
      setSeedPin('');
      setStatus('Seed loaded successfully');
    } catch (error: any) {
      const message = error?.message ?? error?.toString() ?? 'unknown error';
      setStatus(`Seeding failed: ${message}`);
    } finally {
      setSeeding(false);
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

      // Get device public key
      const pubkeyResp = await device.call({ type: 'GetCheetahPub', path: [] });
      if (pubkeyResp.type !== 'OkCheetahPub') {
        throw new Error('Failed to get device public key');
      }

      const devicePubkeys = [{ x: pubkeyResp.x, y: pubkeyResp.y }];
      const inputs = tx.get_signing_inputs(devicePubkeys);

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

      // Sign each input
      const signatures = [];
      for (const input of convertedInputs) {
        setStatus(`Signing input: ${input.input_name}`);

        // Convert msg5 strings to BigInt array
        const msg5BigInts = input.msg5.map((v: string) => BigInt(v));

        const sigResp = await device.call({
          type: 'SignSpendHash',
          path: [],
          msg5: msg5BigInts
        });

        if (sigResp.type === 'OkCheetahSig') {
          signatures.push({
            input_name: input.input_name,
            pubkey_x: Array.from(pubkeyResp.x).map(n => n.toString()),
            pubkey_y: Array.from(pubkeyResp.y).map(n => n.toString()),
            chal: Array.from(sigResp.chal).map(n => n.toString()),
            sig: Array.from(sigResp.sig).map(n => n.toString())
          });
        } else {
          throw new Error(`Failed to sign input ${input.input_name}`);
        }
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

      setStatus(`✓ Signed ${signatures.length} input(s) and downloaded ${filename}`);
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
          {readyToSeed && (
            <div className="section">
              <h2>Load a seed</h2>
              <p className="seed-subtitle">
                Device ready to seed. Make sure your keys are <b>written down</b>, on something that isn't a computer.
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
                <input
                  type="password"
                  className="input pin-input"
                  value={seedPin}
                  onChange={(e) => setSeedPin(e.target.value)}
                  placeholder="set a device PIN"
                  disabled={seeding}
                  autoComplete="off"
                />
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
                    disabled={seeding || !mnemonic.trim() || !hasSeedPin}
                    className="btn btn-success"
                  >
                    {seeding ? 'seeding...' : 'load seed'}
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
              </div>
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
              <div className="button-group" style={{ gridTemplateColumns: '1fr 1fr 1fr 1fr' }}>
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
                <button onClick={disconnect} className="btn btn-secondary">
                  disconnect
                </button>
              </div>
            </div>
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
                  {info.has_seed && info.cheetah_x && info.cheetah_y && (
                    <div className="status-item full-width">
                      <span className="label">public key:</span>
                      <div className="pubkey-display">
                        <span className="pubkey-text">{formatCheetahPubkey(info.cheetah_x, info.cheetah_y)}</span>
                        <button
                          onClick={() => {
                            navigator.clipboard.writeText(formatCheetahPubkey(info.cheetah_x, info.cheetah_y));
                            setStatus('Public key copied to clipboard');
                          }}
                          className="btn btn-small copy-btn"
                        >
                          copy
                        </button>
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
            <button onClick={refreshStatus} className="btn btn-small">
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
