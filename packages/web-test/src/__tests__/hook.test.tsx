import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRustrum } from 'rustrum-sdk';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

describe('useRustrum React Hook Tests', () => {
  it('should initialize WASM and run helper functions', async () => {
    const { result } = renderHook(() => useRustrum());

    // Initial state
    expect(result.current.isLoading).toBe(false);
    expect(result.current.wasmInstance).toBeNull();
    expect(result.current.error).toBeNull();

    // Read WASM file
    const wasmPath = fileURLToPath(import.meta.resolve('rustrum-sdk/pkg/rustrum_wasm_bg.wasm'));
    const wasmBuffer = fs.readFileSync(wasmPath);

    // Call initialize
    let initPromise;
    await act(async () => {
      initPromise = result.current.initialize(wasmBuffer);
      await initPromise;
    });

    expect(result.current.isLoading).toBe(false);
    expect(result.current.wasmInstance).toBeDefined();
    expect(result.current.error).toBeNull();

    // Test deriveKey through hook
    const password = 'mysecretpassword';
    const salt = new Uint8Array(16).fill(9);
    const key = result.current.deriveKey(password, salt);
    expect(key).toBeInstanceOf(Uint8Array);
    expect(key.length).toBe(32);

    // Test decryptChunk through hook using Web Crypto AES-256-GCM
    const nonce = crypto.getRandomValues(new Uint8Array(12));
    const plaintext = new TextEncoder().encode('React integration test plaintext');

    const cryptoKey = await crypto.subtle.importKey(
      'raw',
      key as BufferSource,
      { name: 'AES-GCM' },
      false,
      ['encrypt']
    );
    const encryptedBuffer = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv: nonce },
      cryptoKey,
      plaintext
    );
    const encryptedData = new Uint8Array(encryptedBuffer);

    // Decrypt using hook
    let decrypted: Uint8Array;
    act(() => {
      decrypted = result.current.decryptChunk(2, key, nonce, encryptedData);
    });

    expect(decrypted!).toBeDefined();
    expect(decrypted!.length).toBe(plaintext.length);
    expect(new TextDecoder().decode(decrypted!)).toBe('React integration test plaintext');
  });
});
