import { invoke } from '@tauri-apps/api/core';

export interface HardwareDeviceDescriptor {
  id: string;
  name: string;
  serial: string | null;
}
export type NativeInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export class HardwareTransport {
  private session: number | null = null;
  private reading = false;
  private stopped = false;
  private readonly id: string;
  private readonly failed: (error: Error) => void;
  private readonly call: NativeInvoke;
  constructor(id: string, failed: (error: Error) => void, call: NativeInvoke = invoke) {
    this.id = id;
    this.failed = failed;
    this.call = call;
  }

  isConnected() {
    return this.session !== null && !this.stopped;
  }

  async connect() {
    if (this.session !== null) throw new Error('Hardware transport is already connected');
    this.stopped = false;
    const session = await this.call<number>('hardware_open', { id: this.id });
    if (this.stopped) {
      await this.call('hardware_close', { session });
      throw new Error('Connection cancelled');
    }
    this.session = session;
  }

  async disconnect() {
    this.stopped = true;
    const session = this.session;
    this.session = null;
    if (session !== null) await this.call('hardware_close', { session });
  }

  async write(data: Uint8Array) {
    if (!this.isConnected()) throw new Error('Connect your Nockster first');
    try {
      await this.call('hardware_write', { session: this.session, data: Array.from(data) });
    } catch (error) {
      this.fail(error);
      throw error;
    }
  }

  startReading(onData: (data: Uint8Array) => void) {
    if (this.reading || !this.isConnected()) return;
    this.reading = true;
    const session = this.session;
    void (async () => {
      try {
        while (!this.stopped && this.session === session) {
          const bytes = await this.call<number[]>('hardware_read', { session });
          if (this.stopped || this.session !== session) break;
          if (bytes.length) onData(new Uint8Array(bytes));
          else await new Promise(resolve => setTimeout(resolve, 8));
        }
      } catch (error) {
        if (!this.stopped && this.session === session) this.fail(error);
      } finally {
        this.reading = false;
      }
    })();
  }

  private fail(error: unknown) {
    if (this.stopped) return;
    this.stopped = true;
    this.failed(error instanceof Error ? error : new Error(String(error)));
  }
}
