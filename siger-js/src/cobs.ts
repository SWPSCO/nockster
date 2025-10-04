/**
 * COBS (Consistent Overhead Byte Stuffing) encoder/decoder
 * Compatible with the Rust `cobs` crate used in siger-fw
 */

export class COBSEncoder {
  /**
   * Encode data using COBS and append 0x00 terminator
   * @param data - Input data to encode
   * @returns COBS-encoded data with 0x00 terminator
   */
  static encode(data: Uint8Array): Uint8Array {
    const maxLen = data.length + Math.ceil(data.length / 254) + 1;
    const output = new Uint8Array(maxLen + 1); // +1 for terminator

    let writeIdx = 1;
    let codeIdx = 0;
    let code = 1;

    for (let i = 0; i < data.length; i++) {
      const byte = data[i];
      if (byte === 0) {
        output[codeIdx] = code;
        codeIdx = writeIdx++;
        code = 1;
      } else {
        output[writeIdx++] = byte;
        code++;
        if (code === 0xFF) {
          output[codeIdx] = code;
          codeIdx = writeIdx++;
          code = 1;
        }
      }
    }

    output[codeIdx] = code;
    output[writeIdx] = 0; // Terminator

    return output.slice(0, writeIdx + 1);
  }

  /**
   * Decode COBS-encoded data (without terminator)
   * @param encoded - COBS-encoded data (0x00 terminator should be removed)
   * @returns Decoded data
   */
  static decode(encoded: Uint8Array): Uint8Array {
    if (encoded.length === 0) {
      return new Uint8Array(0);
    }

    const output = new Uint8Array(encoded.length);
    let writeIdx = 0;
    let readIdx = 0;

    while (readIdx < encoded.length) {
      const code = encoded[readIdx++];

      if (code === 0) {
        throw new Error('Invalid COBS encoding: unexpected zero byte');
      }

      for (let i = 1; i < code && readIdx < encoded.length; i++) {
        output[writeIdx++] = encoded[readIdx++];
      }

      if (code < 0xFF && readIdx < encoded.length) {
        output[writeIdx++] = 0;
      }
    }

    return output.slice(0, writeIdx);
  }
}

/**
 * COBS frame reader for streaming data
 * Accumulates bytes until 0x00 terminator is found
 */
export class COBSFrameReader {
  private buffer: number[] = [];

  /**
   * Add bytes to the buffer
   * @param data - Incoming data
   * @returns Decoded frame if complete, or null if still accumulating
   */
  push(data: Uint8Array): Uint8Array | null {
    for (const byte of data) {
      if (byte === 0x00) {
        // Frame complete
        if (this.buffer.length === 0) {
          continue; // Skip leading zeros
        }
        const frame = new Uint8Array(this.buffer);
        this.buffer = [];
        return COBSEncoder.decode(frame);
      } else {
        this.buffer.push(byte);
      }
    }
    return null; // Frame not yet complete
  }

  /**
   * Reset the buffer
   */
  reset(): void {
    this.buffer = [];
  }
}
