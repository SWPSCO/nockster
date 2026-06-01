// Web Serial API types
interface SerialPort {
  readable: ReadableStream<Uint8Array> | null;
  writable: WritableStream<Uint8Array> | null;
  open(options: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
}

interface Navigator {
  serial?: {
    requestPort(options?: { filters?: Array<{ usbVendorId: number; usbProductId?: number }> }): Promise<SerialPort>;
  };
}

// WebHID API types (minimal)
interface HIDDevice extends EventTarget {
  readonly opened: boolean;
  open(): Promise<void>;
  close(): Promise<void>;
  sendReport(reportId: number, data: BufferSource): Promise<void>;
}

interface HIDInputReportEvent extends Event {
  readonly data: DataView;
  readonly device: HIDDevice;
  readonly reportId: number;
}

interface HIDConnectionEvent extends Event {
  readonly device: HIDDevice;
}

interface HIDDeviceFilter {
  vendorId?: number;
  productId?: number;
  usagePage?: number;
  usage?: number;
}

interface HIDRequestDeviceOptions {
  filters?: HIDDeviceFilter[];
}

interface HID extends EventTarget {
  requestDevice(options?: HIDRequestDeviceOptions): Promise<HIDDevice[]>;
  getDevices(): Promise<HIDDevice[]>;
}

interface Navigator {
  hid?: HID;
}
