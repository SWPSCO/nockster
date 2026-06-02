import { COBSEncoder, COBSFrameReader } from './cobs.js';
import {
  Frame,
  Request,
  Response,
  Msg,
  serializeMsg,
  deserializeMsg,
  PROTO_V1,
  getErrorMessage,
  UpdateBundle,
  UpdateStatus,
  SecurityStatus,
  SeedSlotLabel,
  DeviceAddressBookEntry,
  MAX_UPDATE_CHUNK_LEN,
  assertUpdateFirmwareMatchesBundle,
  assertUpdateStreamStatus,
  serializeDeviceAddressBookEntries,
  deserializeDeviceAddressBookEntries,
} from './protocol.js';

export interface SerialTransport {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  write(data: Uint8Array): Promise<void>;
  startReading(onData: (data: Uint8Array) => void): void;
  isConnected(): boolean;
}

const NOCKSTER_HID_VENDOR_ID = 0x303a;
const NOCKSTER_HID_PRODUCT_ID = 0x2001;
const NOCKSTER_HID_REPORT_ID = 1;
const NOCKSTER_HID_REPORT_DATA_LEN = 63; // excluding report ID
const NOCKSTER_HID_PAYLOAD_MAX = NOCKSTER_HID_REPORT_DATA_LEN - 1; // first byte is payload length

function hidOpenError(error: unknown): Error {
  const name = error instanceof DOMException ? error.name : error instanceof Error ? error.name : 'Error';
  const message = error instanceof Error ? error.message : String(error);
  if (name !== 'NotAllowedError') {
    return error instanceof Error ? error : new Error(message);
  }

  return new Error(
    [
      `Failed to open Nockster HID device (${name}: ${message}).`,
      'On Linux this is usually browser confinement or device access, not firmware.',
      'Use a non-Snap/non-Flatpak Chrome or Edge build, close other tabs/CLI sessions using the device, then reconnect USB and try again.',
    ].join(' ')
  );
}

export class NocksterDevice {
  private transport: SerialTransport | null = null;
  private port: SerialPort | null = null;
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null;
  private hidDevice: HIDDevice | null = null;
  private hidOnInputReport: ((event: HIDInputReportEvent) => void) | null = null;
  private frameReader = new COBSFrameReader();
  private nextMsgId = 1;
  private nextFragId = 1;
  private pendingWaiters = new Map<
    number,
    Array<{
      predicate: (response: Response) => boolean;
      resolve: (response: Response) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }>
  >();
  private responseBacklog = new Map<number, Response[]>();
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
    return !!navigator.hid || !!navigator.serial || ('__TAURI__' in window);
  }

  private rejectWaitersForMessage(msgId: number, error: Error): void {
    const waiters = this.pendingWaiters.get(msgId);
    if (!waiters) {
      return;
    }
    for (const waiter of waiters) {
      clearTimeout(waiter.timer);
      waiter.reject(error);
    }
    this.pendingWaiters.delete(msgId);
  }

  private rejectAllWaiters(error: Error): void {
    for (const [msgId] of this.pendingWaiters) {
      this.rejectWaitersForMessage(msgId, error);
    }
  }

  async connect(): Promise<void> {
    if (this.transport) {
      await this.transport.connect();
      this.transport.startReading((data) => {
        for (const frame of this.frameReader.push(data)) {
          this.handleFrame(frame);
        }
      });
      return;
    }

    if (navigator.hid) {
      const devices = await navigator.hid.requestDevice({
        filters: [{ vendorId: NOCKSTER_HID_VENDOR_ID, productId: NOCKSTER_HID_PRODUCT_ID }],
      });
      if (!devices.length) {
        throw new Error('No HID device selected');
      }
      const device = devices[0];
      await this.connectHidDevice(device);
      return;
    }

    if (!navigator.serial) {
      throw new Error('WebHID/Web Serial API not supported in this browser');
    }

    const port = await navigator.serial.requestPort({
      filters: [
        { usbVendorId: 0x303a, usbProductId: 0x1001 },
      ],
    });

    this.port = port;
    await port.open({ baudRate: 115200 });
    this.startReading();
  }

  async connectHidDevice(device: HIDDevice): Promise<void> {
    if (this.transport) {
      throw new Error('Cannot use HID device with a custom transport');
    }

    if (this.hidDevice && this.hidDevice !== device) {
      await this.disconnect();
    }

    this.hidDevice = device;
    try {
      await device.open();
    } catch (error) {
      this.hidDevice = null;
      throw hidOpenError(error);
    }
    this.startHidReading();
  }

  async disconnect(): Promise<void> {
    this.rejectAllWaiters(new Error('Device disconnected'));
    this.responseBacklog.clear();

    if (this.transport) {
      await this.transport.disconnect();
      return;
    }

    if (this.hidDevice) {
      if (this.hidOnInputReport) {
        this.hidDevice.removeEventListener('inputreport', this.hidOnInputReport as EventListener);
        this.hidOnInputReport = null;
      }
      await this.hidDevice.close();
      this.hidDevice = null;
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
    if (this.hidDevice) {
      return this.hidDevice.opened;
    }
    return this.port !== null && this.writer !== null;
  }

  private startHidReading(): void {
    if (!this.hidDevice) {
      throw new Error('HID device not connected');
    }

    this.hidOnInputReport = (event: HIDInputReportEvent) => {
      if (event.reportId !== NOCKSTER_HID_REPORT_ID) {
        return;
      }
      if (event.data.byteLength < 1) {
        return;
      }
      const claimedLen = event.data.getUint8(0);
      const maxLen = Math.min(claimedLen, NOCKSTER_HID_PAYLOAD_MAX, event.data.byteLength - 1);
      if (maxLen <= 0) {
        return;
      }
      const payload = new Uint8Array(event.data.buffer, event.data.byteOffset + 1, maxLen);
      for (const frame of this.frameReader.push(payload)) {
        this.handleFrame(frame);
      }
    };

    this.hidDevice.addEventListener('inputreport', this.hidOnInputReport as EventListener);
  }

  private async hidWrite(data: Uint8Array): Promise<void> {
    if (!this.hidDevice || !this.hidDevice.opened) {
      throw new Error('HID device not connected');
    }

    let off = 0;
    while (off < data.length) {
      const take = Math.min(NOCKSTER_HID_PAYLOAD_MAX, data.length - off);
      const report = new Uint8Array(NOCKSTER_HID_REPORT_DATA_LEN);
      report[0] = take;
      report.set(data.subarray(off, off + take), 1);
      await this.hidDevice.sendReport(NOCKSTER_HID_REPORT_ID, report);
      off += take;
    }
  }

  private async sendFrame(msgId: number, frame: Frame): Promise<void> {
    if (!this.isConnected()) {
      throw new Error('Device not connected');
    }

    const msg: Msg<Frame> = {
      v: PROTO_V1,
      id: msgId,
      msg: frame,
    };
    const serialized = serializeMsg(msg);

    if (this.debug) {
      console.log('Sending:', {
        msgId,
        frame: frame.type,
        serializedBytes: Array.from(serialized)
          .map((b) => b.toString(16).padStart(2, '0'))
          .join(' '),
        length: serialized.length,
      });
    }

    const encoded = COBSEncoder.encode(serialized);

    if (this.debug) {
      console.log('COBS:', {
        encodedBytes: Array.from(encoded)
          .map((b) => b.toString(16).padStart(2, '0'))
          .join(' '),
        length: encoded.length,
      });
    }

    if (this.transport) {
      await this.transport.write(encoded);
    } else if (this.hidDevice) {
      await this.hidWrite(encoded);
    } else {
      await this.writer!.write(encoded);
    }
  }

  private waitForResponse(
    msgId: number,
    predicate: (response: Response) => boolean,
    timeoutMs: number,
  ): Promise<Response> {
    const backlog = this.responseBacklog.get(msgId);
    if (backlog && backlog.length) {
      const idx = backlog.findIndex(predicate);
      if (idx >= 0) {
        const [found] = backlog.splice(idx, 1);
        if (backlog.length === 0) {
          this.responseBacklog.delete(msgId);
        }
        return Promise.resolve(found);
      }
    }

    return new Promise<Response>((resolve, reject) => {
      const waiter = {
        predicate,
        resolve,
        reject,
        timer: setTimeout(() => {
          const waiters = this.pendingWaiters.get(msgId);
          if (waiters) {
            const idx = waiters.indexOf(waiter);
            if (idx >= 0) {
              waiters.splice(idx, 1);
            }
            if (waiters.length === 0) {
              this.pendingWaiters.delete(msgId);
            }
          }
          reject(new Error(`Request timeout after ${timeoutMs}ms (msgId: ${msgId})`));
        }, timeoutMs),
      };

      const waiters = this.pendingWaiters.get(msgId) ?? [];
      waiters.push(waiter);
      this.pendingWaiters.set(msgId, waiters);
    });
  }

  async call(request: Request, timeoutMs: number = 30000): Promise<Response> {
    const msgId = this.nextMsgId++;
    const respP = this.waitForResponse(msgId, () => true, timeoutMs);
    try {
      await this.sendFrame(msgId, { type: 'One', request });
    } catch (error: any) {
      this.rejectWaitersForMessage(msgId, error instanceof Error ? error : new Error(String(error)));
      try {
        await respP;
      } catch {
        // The original send error is more useful than the cancelled waiter.
      }
      throw error;
    }
    return await respP;
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

  async reboot(): Promise<void> {
    const msgId = this.nextMsgId++;
    const respP = this.waitForResponse(msgId, () => true, 2000);
    let requestDelivered = false;
    try {
      await this.sendFrame(msgId, { type: 'One', request: { type: 'Reboot' } });
      requestDelivered = true;
      const resp = await respP;
      if (resp.type === 'Err') {
        throw new Error(getErrorMessage(resp.code));
      }
      if (resp.type !== 'Ok') {
        throw new Error(`unexpected response: ${resp.type}`);
      }
    } catch (error: any) {
      if (!requestDelivered) {
        this.rejectWaitersForMessage(msgId, error instanceof Error ? error : new Error(String(error)));
        try {
          await respP;
        } catch {
          // The original send error is more useful than the cancelled waiter.
        }
        throw error;
      }
      const message = error?.message ?? error?.toString() ?? '';
      if (message.includes('Request timeout') || message.includes('Device disconnected')) {
        return;
      }
      throw error;
    }
  }

  async getLockStatus() {
    const resp = await this.call({ type: 'GetLockStatus' });
    if (resp.type !== 'OkLockStatus') {
      throw new Error('Unexpected response type');
    }
    return resp;
  }

  async unlock(pin: string) {
    // Unlock takes ~5 seconds due to PBKDF2, use longer timeout
    const resp = await this.call({ type: 'Unlock', pin }, 15000);
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
    // InitializePIN runs PBKDF2 and rewrites persistent flash storage.
    const resp = await this.call({ type: 'InitializePIN', pin, seed64 }, 120000);
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    return resp;
  }

  async addSeed(seed64: Uint8Array) {
    if (seed64.length !== 64) {
      throw new Error('seed must be 64 bytes');
    }
    const resp = await this.call({ type: 'AddSeed', seed64 }, 60000);
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

  async changePinOnDevice(currentPin: string) {
    const resp = await this.call({ type: 'ChangePinOnDevice', current_pin: currentPin }, 120000);
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

  async getSeedLabels(): Promise<SeedSlotLabel[]> {
    const resp = await this.call({ type: 'GetSeedLabels' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkSeedLabels') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.labels;
  }

  async setSeedLabel(slot: number, label: string) {
    const resp = await this.call({ type: 'SetSeedLabel', slot, label });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  async getAddressBook(timeoutMs: number = 30000): Promise<DeviceAddressBookEntry[]> {
    const msgId = this.nextMsgId++;
    const respP = this.waitForResponse(msgId, () => true, timeoutMs);
    try {
      await this.sendFrame(msgId, { type: 'One', request: { type: 'GetAddressBook' } });
    } catch (error: any) {
      this.rejectWaitersForMessage(msgId, error instanceof Error ? error : new Error(String(error)));
      try {
        await respP;
      } catch {
        // Preserve original send error.
      }
      throw error;
    }

    const resp = await respP;
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type === 'OkAddressBook') {
      return resp.entries;
    }
    if (resp.type !== 'FragBegin' || resp.kind !== 'AddressBook') {
      throw new Error(`unexpected response: ${resp.type}`);
    }

    const payload = await this.receiveFragBlob(msgId, resp.id, resp.total_len, timeoutMs);
    return deserializeDeviceAddressBookEntries(payload);
  }

  async setAddressBook(entries: DeviceAddressBookEntry[], timeoutMs: number = 30000) {
    const payload = serializeDeviceAddressBookEntries(entries);
    const resp = await this.sendAddressBookPayload(payload, timeoutMs);
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

  async getUpdateTrust() {
    const resp = await this.call({ type: 'GetUpdateTrust' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkUpdateTrust') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.trust;
  }

  async getReleaseInfo() {
    const resp = await this.call({ type: 'GetReleaseInfo' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkReleaseInfo') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.info;
  }

  async getBuildInfo() {
    const resp = await this.call({ type: 'GetBuildInfo' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkBuildInfo') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.info;
  }

  async getSecurityStatus(): Promise<SecurityStatus> {
    const resp = await this.call({ type: 'GetSecurityStatus' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkSecurityStatus') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.status;
  }

  async getUpdateBootStatus() {
    const resp = await this.call({ type: 'GetUpdateBootStatus' });
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'OkUpdateBootStatus') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp.status;
  }

  async verifyUpdateBundle(bundle: UpdateBundle) {
    const resp = await this.call({
      type: 'VerifyUpdateManifest',
      manifest: bundle.manifest,
      signature64: bundle.signature64,
      signing_pubkey_sec1: bundle.signing_pubkey_sec1,
    }, 30000);
    if (resp.type === 'Err') {
      throw new Error(getErrorMessage(resp.code));
    }
    if (resp.type !== 'Ok') {
      throw new Error(`unexpected response: ${resp.type}`);
    }
    return resp;
  }

  async streamUpdateBundle(
    bundle: UpdateBundle,
    firmware: Uint8Array,
    options: {
      writeFlash?: boolean;
      chunkSize?: number;
      timeoutMs?: number;
      onProgress?: (status: UpdateStatus) => void;
      onBegin?: (status: UpdateStatus) => void;
      onChunk?: (status: UpdateStatus, expectedBytesReceived: number) => void;
    } = {},
  ): Promise<UpdateStatus> {
    const writeFlash = options.writeFlash ?? false;
    const chunkSize = options.chunkSize ?? MAX_UPDATE_CHUNK_LEN;
    const timeoutMs = options.timeoutMs ?? 120000;

    if (chunkSize <= 0 || chunkSize > MAX_UPDATE_CHUNK_LEN) {
      throw new Error(`chunkSize must be between 1 and ${MAX_UPDATE_CHUNK_LEN}`);
    }
    await assertUpdateFirmwareMatchesBundle(bundle, firmware);

    let begun = false;
    try {
      const beginResp = await this.call({
        type: 'BeginUpdate',
        manifest: bundle.manifest,
        signature64: bundle.signature64,
        signing_pubkey_sec1: bundle.signing_pubkey_sec1,
        write_flash: writeFlash,
      }, timeoutMs);
      if (beginResp.type === 'Err') {
        throw new Error(getErrorMessage(beginResp.code));
      }
      if (beginResp.type !== 'OkUpdateStatus') {
        throw new Error(`unexpected response: ${beginResp.type}`);
      }
      assertUpdateStreamStatus(beginResp.status, bundle, 'begin update stream', {
        expectedActive: true,
        expectedManifestVerified: true,
        expectedImageVerified: false,
        expectedBytesReceived: 0,
      });
      begun = true;
      options.onBegin?.(beginResp.status);
      options.onProgress?.(beginResp.status);

      let offset = 0;
      while (offset < firmware.length) {
        const end = Math.min(firmware.length, offset + chunkSize);
        const chunk = firmware.subarray(offset, end);
        const chunkResp = await this.call({ type: 'UpdateChunk', offset, chunk }, timeoutMs);
        if (chunkResp.type === 'Err') {
          throw new Error(getErrorMessage(chunkResp.code));
        }
        if (chunkResp.type !== 'OkUpdateStatus') {
          throw new Error(`unexpected response: ${chunkResp.type}`);
        }
        assertUpdateStreamStatus(chunkResp.status, bundle, 'stream update chunk', {
          expectedActive: true,
          expectedManifestVerified: true,
          expectedImageVerified: false,
          expectedBytesReceived: end,
        });
        options.onChunk?.(chunkResp.status, end);
        offset = chunkResp.status.bytes_received;
        options.onProgress?.(chunkResp.status);
      }

      const finishResp = await this.call({ type: 'FinishUpdate' }, timeoutMs);
      if (finishResp.type === 'Err') {
        throw new Error(getErrorMessage(finishResp.code));
      }
      if (finishResp.type !== 'OkUpdateStatus') {
        throw new Error(`unexpected response: ${finishResp.type}`);
      }
      assertUpdateStreamStatus(
        finishResp.status,
        bundle,
        'finish update stream',
        {
          expectedActive: false,
          expectedManifestVerified: true,
          expectedImageVerified: true,
          expectedBytesReceived: bundle.manifest.image_size,
        },
      );
      options.onProgress?.(finishResp.status);
      return finishResp.status;
    } catch (error) {
      if (begun && this.isConnected()) {
        try {
          await this.call({ type: 'CancelUpdate' }, 5000);
        } catch {
          // Preserve the original update error.
        }
      }
      throw error;
    }
  }

  async signDraft(draft: Uint8Array, timeoutMs: number = 120000): Promise<Uint8Array> {
    if (!this.isConnected()) {
      throw new Error('Device not connected');
    }
    if (!draft.length) {
      throw new Error('draft is empty');
    }

    const msgId = this.nextMsgId++;
    const fragId = (this.nextFragId++ & 0xffff) || 1;

    // 1) Begin
    const beginRespP = this.waitForResponse(msgId, () => true, timeoutMs);
    await this.sendFrame(msgId, {
      type: 'FragBegin',
      id: fragId,
      total_len: draft.length,
      kind: 'SignDraft',
    });
    const beginResp = await beginRespP;
    if (beginResp.type === 'Err') {
      throw new Error(getErrorMessage(beginResp.code));
    }
    if (beginResp.type !== 'Ok') {
      throw new Error(`unexpected response to FragBegin: ${beginResp.type}`);
    }

    // 2) Send parts; device replies Ok per part. Final part replies Ok (GUI path) or FragBegin (headless).
    let offset = 0;
    const maxChunk = 180;
    while (offset < draft.length) {
      const end = Math.min(draft.length, offset + maxChunk);
      const chunk = draft.subarray(offset, end);
      const last = end === draft.length;

      const partRespP = this.waitForResponse(msgId, () => true, timeoutMs);
      await this.sendFrame(msgId, { type: 'FragPart', id: fragId, offset, chunk, last });
      const partResp = await partRespP;

      if (partResp.type === 'Err') {
        throw new Error(getErrorMessage(partResp.code));
      }

      if (!last) {
        if (partResp.type !== 'Ok') {
          throw new Error(`unexpected response to FragPart: ${partResp.type}`);
        }
      } else if (partResp.type === 'FragBegin') {
        if (partResp.id !== fragId || partResp.kind !== 'SignDraft') {
          throw new Error('unexpected FragBegin (id/kind mismatch)');
        }
        return await this.receiveFragBlob(msgId, fragId, partResp.total_len, timeoutMs);
      } else if (partResp.type !== 'Ok') {
        throw new Error(`unexpected response to last FragPart: ${partResp.type}`);
      }

      offset = end;
    }

    // 3) GUI path: wait for approval result (Err) or outbound frag begin.
    const ready = await this.waitForResponse(
      msgId,
      (r) =>
        r.type === 'Err' ||
        (r.type === 'FragBegin' && r.id === fragId && r.kind === 'SignDraft'),
      timeoutMs,
    );
    if (ready.type === 'Err') {
      throw new Error(getErrorMessage(ready.code));
    }
    if (ready.type !== 'FragBegin') {
      throw new Error(`unexpected response while waiting for approval: ${ready.type}`);
    }
    if (ready.id !== fragId || ready.kind !== 'SignDraft') {
      throw new Error('unexpected FragBegin (id/kind mismatch)');
    }
    return await this.receiveFragBlob(msgId, fragId, ready.total_len, timeoutMs);
  }

  private async sendAddressBookPayload(payload: Uint8Array, timeoutMs: number): Promise<Response> {
    const msgId = this.nextMsgId++;
    const fragId = (this.nextFragId++ & 0xffff) || 1;

    const beginRespP = this.waitForResponse(msgId, () => true, timeoutMs);
    await this.sendFrame(msgId, {
      type: 'FragBegin',
      id: fragId,
      total_len: payload.length,
      kind: 'AddressBook',
    });
    const beginResp = await beginRespP;
    if (beginResp.type === 'Err') return beginResp;
    if (beginResp.type !== 'Ok') {
      throw new Error(`unexpected response to FragBegin: ${beginResp.type}`);
    }

    let offset = 0;
    const maxChunk = 180;
    let lastResp: Response = beginResp;
    while (offset < payload.length) {
      const end = Math.min(payload.length, offset + maxChunk);
      const chunk = payload.subarray(offset, end);
      const last = end === payload.length;

      const partRespP = this.waitForResponse(msgId, () => true, timeoutMs);
      await this.sendFrame(msgId, { type: 'FragPart', id: fragId, offset, chunk, last });
      const partResp = await partRespP;
      if (partResp.type === 'Err') return partResp;
      if (partResp.type !== 'Ok') {
        throw new Error(`unexpected response to FragPart: ${partResp.type}`);
      }

      lastResp = partResp;
      offset = end;
    }

    return lastResp;
  }

  private async receiveFragBlob(
    msgId: number,
    fragId: number,
    totalLen: number,
    timeoutMs: number,
  ): Promise<Uint8Array> {
    if (totalLen <= 0 || totalLen > 64 * 1024) {
      throw new Error(`invalid fragment total_len: ${totalLen}`);
    }

    const out = new Uint8Array(totalLen);
    let nextOff = 0;

    while (true) {
      const resp = await this.waitForResponse(
        msgId,
        (r) => r.type === 'Err' || (r.type === 'FragPart' && r.id === fragId),
        timeoutMs,
      );

      if (resp.type === 'Err') {
        throw new Error(getErrorMessage(resp.code));
      }
      if (resp.type !== 'FragPart') {
        continue;
      }
      if (resp.offset !== nextOff) {
        throw new Error(`fragment offset mismatch: got ${resp.offset}, expected ${nextOff}`);
      }
      if (nextOff + resp.chunk.length > totalLen) {
        throw new Error('fragment overflow');
      }

      out.set(resp.chunk, nextOff);
      nextOff += resp.chunk.length;

      if (resp.last) {
        if (nextOff !== totalLen) {
          throw new Error('fragment ended early');
        }
        return out;
      }
    }
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

        for (const frame of this.frameReader.push(value)) {
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
          pendingIds: Array.from(this.pendingWaiters.keys())
        });
      }

      const waiters = this.pendingWaiters.get(msg.id);
      if (waiters && waiters.length) {
        const idx = waiters.findIndex((w) => w.predicate(msg.msg));
        if (idx >= 0) {
          const [w] = waiters.splice(idx, 1);
          clearTimeout(w.timer);
          if (waiters.length === 0) {
            this.pendingWaiters.delete(msg.id);
          } else {
            this.pendingWaiters.set(msg.id, waiters);
          }
          w.resolve(msg.msg);
          return;
        }
      }

      const backlog = this.responseBacklog.get(msg.id) ?? [];
      backlog.push(msg.msg);
      this.responseBacklog.set(msg.id, backlog);
    } catch (error) {
      if (this.debug) {
        console.error('Failed to deserialize frame:', error);
        console.error('Frame bytes:', Array.from(frame).map(b => b.toString(16).padStart(2, '0')).join(' '));
      }
    }
  }
}
