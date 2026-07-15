#![no_std]

extern crate alloc;

use alloc::{
    boxed::Box, string::{String, ToString}, vec::Vec,
};
use rustrum_core::{
    crypto::{CipherId, decrypt_chunk_in_place, derive_key as core_derive_key},
    header::{IndexEntry, RstrHeader},
};
use wasm_bindgen::prelude::*;

#[derive(Clone)]
pub struct WasmIndexEntry {
    pub offset: u64,
    pub size: u64,
    pub nonce: [u8; 12],
}

#[wasm_bindgen]
pub struct WasmRstrHeader {
    pub version: u16,
    pub cipher_id: u8,
    pub is_split: bool,
    pub duration: f64,
    mime_type: String,
    key_salt: [u8; 16],
    index_table: Vec<WasmIndexEntry>,
}

#[wasm_bindgen]
impl WasmRstrHeader {
    #[wasm_bindgen(getter)]
    pub fn mime_type(&self) -> String {
        self.mime_type.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn key_salt(&self) -> Vec<u8> {
        self.key_salt.to_vec()
    }

    #[wasm_bindgen(getter)]
    pub fn index_count(&self) -> usize {
        self.index_table.len()
    }

    pub fn get_entry_offset(&self, index: usize) -> Option<u64> {
        self.index_table.get(index).map(|entry| entry.offset)
    }

    pub fn get_entry_size(&self, index: usize) -> Option<u64> {
        self.index_table.get(index).map(|entry| entry.size)
    }

    pub fn locate_chunk(&self, byte_position: u64) -> i32 {
        let original_entries: Vec<IndexEntry> = self
            .index_table
            .iter()
            .map(|entry| IndexEntry {
                offset: entry.offset,
                size: entry.size,
                nonce: entry.nonce,
            })
            .collect();

        let dummy_cipher = CipherId::ChaCha20Poly1305;
        let temp_header = RstrHeader {
            version: self.version,
            cipher_id: dummy_cipher,
            is_split: self.is_split,
            mime_type: self.mime_type.clone(),
            duration: self.duration,
            key_salt: self.key_salt,
            index_table: original_entries,
        };

        match temp_header.locate_chunk(byte_position) {
            Ok((idx, _)) => idx as i32,
            Err(_) => -1,
        }
    }

    pub fn locate_chunk_by_time(&self, current_time: f64, duration: f64) -> i32 {
        if self.index_table.is_empty() {
            return -1;
        }
        if self.index_table.len() == 1 {
            return 0;
        }
        if duration <= 0.0 {
            return 1;
        }

        let total_plaintext_size: u64 = self
            .index_table
            .iter()
            .map(|entry| entry.size.saturating_sub(16))
            .sum();

        let init_size = self
            .index_table
            .first()
            .map(|entry| entry.size.saturating_sub(16))
            .unwrap_or(0);

        let media_plaintext_size = total_plaintext_size.saturating_sub(init_size);
        if media_plaintext_size == 0 {
            return 1;
        }

        let media_byte_pos = (current_time / duration) * (media_plaintext_size as f64);
        let target_byte_pos = (init_size as f64) + media_byte_pos;

        let mut target_seg_idx = self.locate_chunk(target_byte_pos as u64);

        if target_seg_idx < 1 {
            target_seg_idx = 1;
        }
        let max_idx = (self.index_table.len() - 1) as i32;
        if target_seg_idx > max_idx {
            target_seg_idx = max_idx;
        }

        target_seg_idx
    }
}

#[wasm_bindgen]
pub struct WasmDecoder {
    header: WasmRstrHeader,
    key: Vec<u8>,
    decrypt_buffer: Box<[u8]>,
}

#[wasm_bindgen]
impl WasmDecoder {
    #[wasm_bindgen(constructor)]
    pub fn new(header_bytes: &[u8], password: &str) -> Result<WasmDecoder, JsValue> {
        let parsed = parse_header(header_bytes)?;
        let key = core_derive_key(password.as_bytes(), &parsed.key_salt())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let max_segment_size = parsed
            .index_table
            .iter()
            .map(|entry| entry.size as usize)
            .max()
            .ok_or_else(|| JsValue::from_str("Empty index table"))?;

        let decrypt_buffer = alloc::vec![0; max_segment_size].into_boxed_slice();

        Ok(WasmDecoder {
            header: parsed,
            key: key.to_vec(),
            decrypt_buffer,
        })
    }

    #[wasm_bindgen(getter)]
    pub fn mime_type(&self) -> String {
        self.header.mime_type()
    }

    #[wasm_bindgen(getter)]
    pub fn version(&self) -> u16 {
        self.header.version
    }

    #[wasm_bindgen(getter)]
    pub fn cipher_id(&self) -> u8 {
        self.header.cipher_id
    }

    #[wasm_bindgen(getter)]
    pub fn is_split(&self) -> bool {
        self.header.is_split
    }

    #[wasm_bindgen(getter)]
    pub fn duration(&self) -> f64 {
        self.header.duration
    }

    #[wasm_bindgen(getter)]
    pub fn key_salt(&self) -> Vec<u8> {
        self.header.key_salt()
    }

    #[wasm_bindgen(getter)]
    pub fn index_count(&self) -> usize {
        self.header.index_count()
    }

    pub fn get_entry_offset(&self, index: usize) -> Option<u64> {
        self.header.get_entry_offset(index)
    }

    pub fn get_entry_size(&self, index: usize) -> Option<u64> {
        self.header.get_entry_size(index)
    }

    pub fn locate_chunk(&self, byte_position: u64) -> i32 {
        self.header.locate_chunk(byte_position)
    }

    pub fn locate_chunk_by_time(&self, current_time: f64, duration: f64) -> i32 {
        self.header.locate_chunk_by_time(current_time, duration)
    }

    pub fn get_buffer_ptr(&mut self) -> *mut u8 {
        self.decrypt_buffer.as_mut_ptr()
    }

    pub fn decrypt_segment_in_place(
        &mut self,
        index: usize,
        encrypted_len: usize,
    ) -> Result<usize, JsValue> {
        let entry = self
            .header
            .index_table
            .get(index)
            .ok_or_else(|| JsValue::from_str("Invalid segment index"))?;

        if self.decrypt_buffer.len() < encrypted_len {
            return Err(JsValue::from_str("Buffer size mismatch"));
        }

        let data_slice = &mut self.decrypt_buffer[..encrypted_len];
        let cipher = CipherId::try_from(self.header.cipher_id)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let plaintext_slice = decrypt_chunk_in_place(cipher, &self.key, &entry.nonce, data_slice)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(plaintext_slice.len())
    }
}

#[wasm_bindgen]
pub fn init_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn parse_header(header_bytes: &[u8]) -> Result<WasmRstrHeader, JsValue> {
    let header =
        RstrHeader::deserialize(header_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let index_table = header
        .index_table
        .into_iter()
        .map(|entry| WasmIndexEntry {
            offset: entry.offset,
            size: entry.size,
            nonce: entry.nonce,
        })
        .collect();

    Ok(WasmRstrHeader {
        version: header.version,
        cipher_id: header.cipher_id as u8,
        is_split: header.is_split,
        duration: header.duration,
        mime_type: header.mime_type,
        key_salt: header.key_salt,
        index_table,
    })
}

#[wasm_bindgen]
pub fn derive_key(password: &str, salt: &[u8]) -> Result<Vec<u8>, JsValue> {
    let key = core_derive_key(password.as_bytes(), salt)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(key.to_vec())
}

#[wasm_bindgen]
pub fn decrypt_chunk(
    cipher_id: u8,
    key: &[u8],
    nonce: &[u8],
    encrypted_data: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let cipher = CipherId::try_from(cipher_id).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let mut data = encrypted_data.to_vec();
    let plaintext_slice = decrypt_chunk_in_place(cipher, key, nonce, &mut data)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(plaintext_slice.to_vec())
}
