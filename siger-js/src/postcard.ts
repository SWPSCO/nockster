/**
 * Simplified Postcard-compatible serialization
 *
 * Postcard format:
 * - Varint encoding for lengths
 * - Little-endian for multi-byte integers
 * - Enum variants as varint discriminant + fields
 */

export class PostcardWriter {
  private bytes: number[] = [];

  writeU8(value: number): void {
    this.bytes.push(value & 0xFF);
  }

  writeU16(value: number): void {
    this.bytes.push(value & 0xFF);
    this.bytes.push((value >> 8) & 0xFF);
  }

  writeU32(value: number): void {
    this.bytes.push(value & 0xFF);
    this.bytes.push((value >> 8) & 0xFF);
    this.bytes.push((value >> 16) & 0xFF);
    this.bytes.push((value >> 24) & 0xFF);
  }

  writeU64Array(values: bigint[]): void {
    for (const value of values) {
      for (let i = 0; i < 8; i++) {
        this.bytes.push(Number((value >> BigInt(i * 8)) & 0xFFn));
      }
    }
  }

  writeVarint(value: number): void {
    while (value >= 0x80) {
      this.bytes.push((value & 0x7F) | 0x80);
      value >>>= 7;
    }
    this.bytes.push(value & 0x7F);
  }

  writeU64Varint(value: bigint): void {
    while (value >= 0x80n) {
      this.bytes.push(Number(value & 0x7Fn) | 0x80);
      value >>= 7n;
    }
    this.bytes.push(Number(value & 0x7Fn));
  }

  writeBytes(data: Uint8Array): void {
    this.writeVarint(data.length);
    for (const byte of data) {
      this.bytes.push(byte);
    }
  }

  writeFixedBytes(data: Uint8Array): void {
    for (const byte of data) {
      this.bytes.push(byte);
    }
  }

  writeString(str: string): void {
    const encoded = new TextEncoder().encode(str);
    this.writeBytes(encoded);
  }

  writeBool(value: boolean): void {
    this.bytes.push(value ? 1 : 0);
  }

  toBytes(): Uint8Array {
    return new Uint8Array(this.bytes);
  }
}

export class PostcardReader {
  private offset = 0;

  constructor(private data: Uint8Array) {}

  readU8(): number {
    return this.data[this.offset++];
  }

  readU16(): number {
    const low = this.data[this.offset++];
    const high = this.data[this.offset++];
    return low | (high << 8);
  }

  readU32(): number {
    const b0 = this.data[this.offset++];
    const b1 = this.data[this.offset++];
    const b2 = this.data[this.offset++];
    const b3 = this.data[this.offset++];
    return b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
  }

  readU64Array(count: number): bigint[] {
    const result: bigint[] = [];
    for (let i = 0; i < count; i++) {
      // postcard encodes u64 as varint!
      let value = 0n;
      let shift = 0;
      while (true) {
        const byte = this.data[this.offset++];
        value |= BigInt(byte & 0x7F) << BigInt(shift);
        if ((byte & 0x80) === 0) break;
        shift += 7;
      }
      result.push(value);
    }
    return result;
  }

  readVarint(): number {
    let value = 0;
    let shift = 0;
    while (true) {
      const byte = this.data[this.offset++];
      value |= (byte & 0x7F) << shift;
      if ((byte & 0x80) === 0) break;
      shift += 7;
    }
    return value;
  }

  readBytes(): Uint8Array {
    const len = this.readVarint();
    const result = this.data.slice(this.offset, this.offset + len);
    this.offset += len;
    return result;
  }

  readFixedBytes(len: number): Uint8Array {
    const result = this.data.slice(this.offset, this.offset + len);
    this.offset += len;
    return result;
  }

  readString(): string {
    const bytes = this.readBytes();
    return new TextDecoder().decode(bytes);
  }

  readBool(): boolean {
    return this.data[this.offset++] !== 0;
  }

  hasMore(): boolean {
    return this.offset < this.data.length;
  }
}
