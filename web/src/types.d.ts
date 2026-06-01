// Web Serial API types
interface SerialPort {
  readable: ReadableStream<Uint8Array> | null;
  writable: WritableStream<Uint8Array> | null;
  open(options: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
}

interface Navigator {
  serial: {
    requestPort(options?: { filters?: Array<{ usbVendorId: number; usbProductId?: number }> }): Promise<SerialPort>;
  };
}

interface ImportMetaEnv {
  readonly VITE_NOCKSTER_RELEASE_INDEX_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
