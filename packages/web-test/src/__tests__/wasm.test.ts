import { describe, it, expect, beforeAll } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import {
  initWasm,
  WasmDecoder
} from 'rustrum-sdk';

// Helper to create big-endian byte arrays
function writeU16(val: number): Uint8Array {
  const buf = new Uint8Array(2);
  new DataView(buf.buffer).setUint16(0, val, false);
  return buf;
}

function writeU32(val: number): Uint8Array {
  const buf = new Uint8Array(4);
  new DataView(buf.buffer).setUint32(0, val, false);
  return buf;
}

function writeU64(val: bigint): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setBigUint64(0, val, false);
  return buf;
}

function writeF64(val: number): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setFloat64(0, val, false);
  return buf;
}

describe('rustrum-wasm Core Unit Tests with WasmDecoder', () => {
  beforeAll(async () => {
    // Load WASM file directly from filesystem for testing in Node
    const wasmPath = fileURLToPath(import.meta.resolve('rustrum-sdk/pkg/rustrum_wasm_bg.wasm'));
    const wasmBuffer = fs.readFileSync(wasmPath);
    await initWasm(wasmBuffer);
  });

  describe('WasmDecoder Lifecycle and Methods', () => {
    it('should correctly parse a valid RSTR header binary layout and decrypt', async () => {
      const magic = new Uint8Array([0x52, 0x53, 0x54, 0x52]); // "RSTR"
      const version = writeU16(1);
      const cipherId = new Uint8Array([2]); // AES-256-GCM
      const isSplit = new Uint8Array([0]);
      // MIME type: 1B length + N bytes UTF-8
      const mimeType = new TextEncoder().encode('video/mp4; codecs="avc1.64001e, mp4a.40.2"');
      const mimeLen = new Uint8Array([mimeType.length]);
      const salt = new Uint8Array(16).fill(7);
      const duration = writeF64(52.29);
      const indexCount = writeU32(1);

      // Entry 1
      const entry1Offset = writeU64(0n);
      const entry1Size = writeU64(48n); // 32 bytes plaintext + 16 bytes tag
      const entry1Nonce = new Uint8Array(12).fill(1);

      // Combine
      const headerBytes = new Uint8Array([
        ...magic,
        ...version,
        ...cipherId,
        ...isSplit,
        ...mimeLen,
        ...mimeType,
        ...salt,
        ...duration,
        ...indexCount,
        ...entry1Offset,
        ...entry1Size,
        ...entry1Nonce,
      ]);

      const decoder = new WasmDecoder(headerBytes, 'testpassword');

      expect(decoder.version).toBe(1);
      expect(decoder.cipher_id).toBe(2);
      expect(decoder.is_split).toBe(false);
      expect(decoder.duration).toBe(52.29);
      expect(decoder.key_salt).toEqual(salt);
      expect(decoder.index_count).toBe(1);

      const offset = decoder.get_entry_offset(0);
      const size = decoder.get_entry_size(0);
      expect(offset).toBe(0n);
      expect(size).toBe(48n);

      // Test locate_chunk:
      expect(decoder.locate_chunk(0n)).toBe(0);
      expect(decoder.locate_chunk(31n)).toBe(0);
      expect(decoder.locate_chunk(32n)).toBe(-1);
    });

    it('should correctly locate chunks by time in the real fmp4.rstrm file', () => {
      const rstrmPath = path.resolve(__dirname, '../../../web/public/fmp4.rstrm');
      const rstrmBuffer = fs.readFileSync(rstrmPath);
      const decoder = new WasmDecoder(rstrmBuffer, 'testpassword');
      console.log('INDEX COUNT:', decoder.index_count);
      console.log('DURATION:', decoder.duration);
      for (let t = 0; t <= 52.29; t += 5) {
        console.log(`Time: ${t}s -> Chunk: ${decoder.locate_chunk_by_time(t, 52.29)}`);
      }
    });
  });
});


