import { invoke } from '@tauri-apps/api/core';

export interface SerialTransport {
  isConnected(): boolean;
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  write(data: Uint8Array): Promise<void>;
  startReading(onData: (data: Uint8Array) => void): void;
  getAvailablePorts?(): Promise<string[]>;
  setSelectedPort?(port: string): void;
}

function detectTauri(): boolean {
  return typeof window !== 'undefined' && (
    '__TAURI__' in window || 
    '__TAURI_INTERNALS__' in window ||
    window.location.protocol === 'tauri:'
  );
}

class WebSerialTransport implements SerialTransport {
  private port: SerialPort | null = null;
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null;

  isConnected(): boolean {
    return this.port !== null && this.writer !== null;
  }

  async connect(): Promise<void> {
    console.log('WebSerialTransport.connect');
    this.port = await navigator.serial.requestPort({
      filters: [{ usbVendorId: 0x303a, usbProductId: 0x1001 }],
    });
    await this.port.open({ baudRate: 115200 });
  }

  async disconnect(): Promise<void> {
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

  async write(data: Uint8Array): Promise<void> {
    if (!this.writer) throw new Error('Not connected');
    await this.writer.write(data);
  }

  startReading(onData: (data: Uint8Array) => void): void {
    if (!this.port?.readable) return;
    this.reader = this.port.readable.getReader();
    this.writer = this.port.writable!.getWriter();

    (async () => {
      try {
        while (this.reader) {
          const { value, done } = await this.reader.read();
          if (done) break;
          onData(value);
        }
      } catch (error) {
        console.error('Read error:', error);
      }
    })();
  }
}

class TauriSerialTransport implements SerialTransport {
  private connected = false;
  private portPath: string | null = null;
  private selectedPort: string | null = null;

  isConnected(): boolean {
    return this.connected;
  }

  async getAvailablePorts(): Promise<string[]> {
    const allPorts = await invoke<string[]>('list_serial_ports');
    console.log(allPorts);
    return allPorts.filter(port => 
      (port.includes('tty') && !port.includes('Bluetooth') && !port.includes('debug'))
    );
  }

  setSelectedPort(port: string): void {
    this.selectedPort = port;
  }

  async connect(): Promise<void> {
    if (!this.selectedPort) {
      throw new Error('No port selected. Call setSelectedPort() first.');
    }
    
    this.portPath = this.selectedPort;
    console.log('Connecting to:', this.portPath);
    await invoke('connect_serial', { port: this.portPath, baudRate: 115200 });
    this.connected = true;
  }

  async disconnect(): Promise<void> {
    if (this.connected && this.portPath) {
      await invoke('disconnect_serial', { port: this.portPath });
      this.connected = false;
    }
  }

  async write(data: Uint8Array): Promise<void> {
    if (!this.connected) throw new Error('Not connected');
    await invoke('serial_write', { data: Array.from(data) });
  }

  startReading(onData: (data: Uint8Array) => void): void {
    const poll = async () => {
      while (this.connected) {
        try {
          const data = await invoke<number[]>('serial_read');
          if (data.length > 0) {
            onData(new Uint8Array(data));
          }
          await new Promise(resolve => setTimeout(resolve, 10));
        } catch (err) {
          console.error('Read error:', err);
        }
      }
    };
    poll();
  }
}

export function createSerialTransport(): SerialTransport {
  const isTauri = detectTauri();
  console.log('createSerialTransport - isTauri:', isTauri);
  return isTauri ? new TauriSerialTransport() : new WebSerialTransport();
}

