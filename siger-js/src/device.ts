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

export interface SerialTransport {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  write(data: Uint8Array): Promise<void>;
  startReading(onData: (data: Uint8Array) => void): void;
  isConnected(): boolean;
}

export class SigerDevice {
  private transport: SerialTransport | null = null;
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

  constructor(transportOrOptions?: SerialTransport | { debug?: boolean }) {
    if (transportOrOptions && 'connect' in transportOrOptions) {
      this.transport = transportOrOptions;
      this.debug = false;
    } else {
      this.debug = (transportOrOptions as { debug?: boolean })?.debug ?? false;
    }
  }

  static isSupported(): boolean {
    return 'serial' in navigator || ('__TAURI__' in window);
  }

  async connect(): Promise<void> {
    if (this.transport) {
      await this.transport.connect();
      this.transport.startReading((data) => {
        const frame = this.frameReader.push(data);
        if (frame) {
          this.handleFrame(frame);
        }
      });
      return;
    }

    if (!('serial' in navigator)) {
      throw new Error('Web Serial API not supported in this browser');
    }

    this.port = await navigator.serial.requestPort({
      filters: [
        { usbVendorId: 0x303a, usbProductId: 0x1001 },
      ],
    });

    await this.port.open({ baudRate: 115200 });
    this.startReading();
  }

  async disconnect(): Promise<void> {
    for (const [, { reject }] of this.pendingCalls) {
      reject(new Error('Device disconnected'));
    }
    this.pendingCalls.clear();

    if (this.transport) {
      await this.transport.disconnect();
      return;
    }

    if (this.reader) {
      await this.reader.cancel();
      this.reader = null;
    }
    if (this.writer) {
      await this.writer.close();
      this.writer = null;
    }
    if (this.port) {
      await this.port.close();
      this.port = null;
    }
  }

  isConnected(): boolean {
    if (this.transport) {
      return this.transport.isConnected();
    }
    return this.port !== null && this.writer !== null;
  }

  async call(request: Request, timeoutMs: number = 30000): Promise<Response> {
    if (!this.isConnected()) {
      throw new Error('Device not connected');
    }

    const msgId = this.nextMsgId++;
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

    const encoded = COBSEncoder.encode(serialized);

    if (this.debug) {
      console.log('COBS:', {
        encodedBytes: Array.from(encoded).map(b => b.toString(16).padStart(2, '0')).join(' '),
        length: encoded.length
      });
    }

    const promise = new Promise<Response>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingCalls.delete(msgId);
        reject(new Error(`Request timeout after ${timeoutMs}ms (msgId: ${msgId}, type: ${request.type})`));
      }, timeoutMs);

      this.pendingCalls.set(msgId, {
        resolve: (response: Response) => {
          clearTimeout(timer);
          resolve(response);
        },
        reject: (error: Error) => {
          clearTimeout(timer);
          reject(error);
        }
      });
    });

    if (this.transport) {
      await this.transport.write(encoded);
    } else {
      await this.writer!.write(encoded);
    }

    return promise;
  }

  async getInfo() {
    return await this.call({ type: 'GetInfo' });
  }

  async ping() {
    return await this.call({ type: 'Ping' });
  }

  async reset() {
    const resp = await this.call({ type: 'Reset' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
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

  async addSeed(seed64: Uint8Array) {
    if (seed64.length !== 64) {
      throw new Error('seed must be 64 bytes');
    }
    const resp = await this.call({ type: 'AddSeed', seed64 });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  async deleteSeed(slot: number) {
    const resp = await this.call({ type: 'DeleteSeed', slot });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  async resetPIN(currentPin: string, newPin: string) {
    const resp = await this.call({ type: 'ResetPIN', current_pin: currentPin, new_pin: newPin });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  async selectSeed(slot: number) {
    const resp = await this.call({ type: 'SelectSeed', slot });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
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