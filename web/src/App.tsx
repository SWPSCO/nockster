import { useState, useEffect } from 'react';
import { SigerDevice, formatCheetahPubkey } from 'siger-js';
import { init, ParsedTransaction } from 'siger-wasm';
import './App.css';

function App() {
  const [device] = useState(() => new SigerDevice());
  const [connected, setConnected] = useState(false);
  const [status, setStatus] = useState<string>('');
  const [locked, setLocked] = useState<boolean | null>(null);
  const [attemptsRemaining, setAttemptsRemaining] = useState<number | null>(null);
  const [pin, setPin] = useState('');
  const [info, setInfo] = useState<any>(null);

  // Transaction signing state
  const [wasmReady, setWasmReady] = useState(false);
  const [tx, setTx] = useState<ParsedTransaction | null>(null);
  const [txInfo, setTxInfo] = useState<any>(null);
  const [txDetails, setTxDetails] = useState<any>(null);
  const [signingInputs, setSigningInputs] = useState<any[]>([]);
  const [signing, setSigning] = useState(false);
  const [signedTxBytes, setSignedTxBytes] = useState<Uint8Array | null>(null);

  // Initialize WASM
  useEffect(() => {
    console.log('Initializing WASM...');
    try {
      init();
      console.log('WASM initialized successfully');
      setWasmReady(true);
    } catch (err: any) {
      console.error('WASM init failed:', err);
      console.error('Error stack:', err.stack);
    }
  }, []);

  // Check Web Serial support
  const isSupported = SigerDevice.isSupported();

  const connect = async () => {
    try {
      setStatus('Connecting...');
      await device.connect();
      setConnected(true);
      setStatus('Connected!');
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

  const prepareToSign = async () => {
    if (!tx || !connected || locked) {
      setStatus('Device must be connected and unlocked');
      return;
    }

    try {
      setStatus('Finding inputs to sign...');

      const response = await device.call({ type: 'GetCheetahPub', path: [] });
      if (response.type !== 'OkCheetahPub') {
        throw new Error('Failed to get device public key');
      }

      const devicePubkeys = [{ x: response.x, y: response.y }];
      const inputs = tx.get_signing_inputs(devicePubkeys);

      setSigningInputs(inputs);
      setStatus(`Found ${inputs.length} input(s) to sign`);
    } catch (error: any) {
      setStatus(`Failed to prepare signing: ${error.message}`);
    }
  };

  const signTransaction = async () => {
    if (!tx || !connected || locked || signingInputs.length === 0) {
      setStatus('Cannot sign: device must be connected, unlocked, and inputs prepared');
      return;
    }

    try {
      setSigning(true);
      setStatus('Signing transaction...');

      // Get device public key
      const pubkeyResp = await device.call({ type: 'GetCheetahPub', path: [] });
      if (pubkeyResp.type !== 'OkCheetahPub') {
        throw new Error('Failed to get device public key');
      }

      const signatures = [];
      for (const input of signingInputs) {
        setStatus(`Signing input: ${input.input_name}`);

        const sigResp = await device.call({
          type: 'SignSpendHash',
          path: [],
          msg5: input.msg5
        });

        if (sigResp.type === 'OkCheetahSig') {
          signatures.push({
            input_name: input.input_name,
            pubkey_x: pubkeyResp.x,
            pubkey_y: pubkeyResp.y,
            chal: sigResp.chal,
            sig: sigResp.sig
          });
        } else {
          throw new Error(`Failed to sign input ${input.input_name}`);
        }
      }

      // Apply signatures
      tx.apply_signatures(signatures);

      // Get signed bytes
      const signedBytes = new Uint8Array(tx.to_bytes());
      setSignedTxBytes(signedBytes);

      // Get new tx info
      const newInfo = tx.info();
      setTxInfo(newInfo);

      setStatus(`Successfully signed ${signatures.length} input(s)! New TX ID: ${newInfo.tx_id}`);
    } catch (error: any) {
      setStatus(`Signing failed: ${error.message}`);
    } finally {
      setSigning(false);
    }
  };

  const downloadSignedTx = () => {
    if (!signedTxBytes || !txInfo) return;

    const blob = new Blob([signedTxBytes as any], { type: 'application/octet-stream' });
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
                            <span className="mono-small">{input.input_name}</span>
                            <span className="hash-small">{input.sig_hash}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  <div className="button-group">
                    {!signedTxBytes ? (
                      <>
                        {signingInputs.length === 0 ? (
                          <button
                            onClick={prepareToSign}
                            disabled={!connected || locked === true}
                            className="btn btn-primary"
                          >
                            sign
                          </button>
                        ) : (
                          <button
                            onClick={signTransaction}
                            disabled={signing || locked === true}
                            className="btn btn-success"
                          >
                            {signing ? 'signing...' : 'sign transaction'}
                          </button>
                        )}
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
