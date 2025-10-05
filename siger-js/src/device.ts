import { COBSEncoder, COBSFrameReader } from './cobs';
import {
  Request,
  Response,
  Msg,
  serializeMsg,
  deserializeMsg,
  PROTO_V1,
  getErrorMessage,
} from './protocol';

/**
 * Siger hardware wallet device connection via Web Serial
 */
export class SigerDevice {
  private port: SerialPort | null = null;
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null;
  private frameReader = new COBSFrameReader();
  private nextMsgId = 1;
  private pendingCalls = new Map<number, {
    resolve: (response: Response) => void;
    reject: (error: Error) => void;
  }>();
  private debug: boolean;

  constructor(options?: { debug?: boolean }) {
    this.debug = options?.debug ?? false;
  }

  /**
   * Check if Web Serial API is available
   */
  static isSupported(): boolean {
    return 'serial' in navigator;
  }

  /**
   * Request user to select device
   */
  async connect(): Promise<void> {
    if (!SigerDevice.isSupported()) {
      throw new Error('Web Serial API not supported in this browser');
    }

    // Request port
    this.port = await navigator.serial.requestPort({
      filters: [
        // ESP32-S3 USB-JTAG-Serial
        { usbVendorId: 0x303a, usbProductId: 0x1001 },
      ],
    });

    // Open with same baud rate as CLI
    await this.port.open({ baudRate: 115200 });

    // Start reading
    this.startReading();
  }

  /**
   * Disconnect from device
   */
  async disconnect(): Promise<void> {
    // Cancel any pending calls
    for (const [, { reject }] of this.pendingCalls) {
      reject(new Error('Device disconnected'));
    }
    this.pendingCalls.clear();

    // Close reader/writer
    if (this.reader) {
      await this.reader.cancel();
      this.reader = null;
    }
    if (this.writer) {
      await this.writer.close();
      this.writer = null;
    }

    // Close port
    if (this.port) {
      await this.port.close();
      this.port = null;
    }
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.port !== null && this.writer !== null;
  }

  /**
   * Send request and wait for response
   */
  async call(request: Request): Promise<Response> {
    if (!this.isConnected()) {
      throw new Error('Device not connected');
    }

    const msgId = this.nextMsgId++;

    // Serialize message
    const msg: Msg<Request> = {
      v: PROTO_V1,
      id: msgId,
      msg: request,
    };
    const serialized = serializeMsg(msg);

    if (this.debug) {
      console.log('Sending:', {
        msgId,
        request: request.type,
        serializedBytes: Array.from(serialized).map(b => b.toString(16).padStart(2, '0')).join(' '),
        length: serialized.length
      });
    }

    // Encode with COBS
    const encoded = COBSEncoder.encode(serialized);

    if (this.debug) {
      console.log('COBS:', {
        encodedBytes: Array.from(encoded).map(b => b.toString(16).padStart(2, '0')).join(' '),
        length: encoded.length
      });
    }

    // Create promise for response
    const promise = new Promise<Response>((resolve, reject) => {
      this.pendingCalls.set(msgId, { resolve, reject });
    });

    // Send
    await this.writer!.write(encoded);

    // Wait for response
    return promise;
  }

  /**
   * Convenience methods
   */
  async getInfo() {
    return await this.call({ type: 'GetInfo' });
  }

  async ping() {
    return await this.call({ type: 'Ping' });
  }

  async getLockStatus() {
    const resp = await this.call({ type: 'GetLockStatus' });
    if (resp.type !== 'OkLockStatus') {
      throw new Error('Unexpected response type');
    }
    return resp;
  }

  async unlock(pin: string) {
    const resp = await this.call({ type: 'Unlock', pin });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    return resp;
  }

  async lock() {
    const resp = await this.call({ type: 'Lock' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    return resp;
  }

  async initializePIN(pin: string, seed64: Uint8Array) {
    const resp = await this.call({ type: 'InitializePIN', pin, seed64 });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    return resp;
  }

  async setSeed(seed64: Uint8Array) {
    if (seed64.length !== 64) {
      throw new Error('seed must be 64 bytes');
    }
    const resp = await this.call({ type: 'SetSeed', seed64 });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  /**
   * Start reading from serial port
   */
  private async startReading() {
    if (!this.port || !this.port.readable) {
      return;
    }

    this.reader = this.port.readable!.getReader();
    this.writer = this.port.writable!.getWriter();

    try {
      while (true) {
        const { value, done } = await this.reader.read();
        if (done) break;

        // Feed to frame reader
        const frame = this.frameReader.push(value);
        if (frame) {
          this.handleFrame(frame);
        }
      }
    } catch (error) {
      console.error('Read error:', error);
    } finally {
      this.reader.releaseLock();
    }
  }

  /**
   * Handle a complete COBS frame
   */
  private handleFrame(frame: Uint8Array) {
    try {
      if (this.debug) {
        console.log('Received frame:', {
          frameBytes: Array.from(frame).map(b => b.toString(16).padStart(2, '0')).join(' '),
          length: frame.length
        });
      }

      const msg = deserializeMsg(frame);

      if (this.debug) {
        console.log('Deserialized:', {
          msgId: msg.id,
          msgIdHex: '0x' + msg.id.toString(16),
          version: msg.v,
          response: msg.msg.type,
          pendingIds: Array.from(this.pendingCalls.keys())
        });
      }

      // Find pending call
      const pending = this.pendingCalls.get(msg.id);
      if (!pending) {
        if (this.debug) {
          console.warn('Received response for unknown message ID:', msg.id,
            'Pending IDs:', Array.from(this.pendingCalls.keys()));
        }
        return;
      }

      this.pendingCalls.delete(msg.id);
      pending.resolve(msg.msg);
    } catch (error) {
      if (this.debug) {
        console.error('Failed to deserialize frame:', error);
        console.error('Frame bytes:', Array.from(frame).map(b => b.toString(16).padStart(2, '0')).join(' '));
      }
    }
  }
}
