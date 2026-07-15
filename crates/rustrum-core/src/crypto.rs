use crate::error::Error;
use aes_gcm::{
    aead::AeadInOut,
    Aes128Gcm,
    Aes256Gcm,
    Key as AesKey,
    Nonce as AesNonce,
};
use alloc::vec::Vec;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChaChaKey, KeyInit, Nonce as ChaChaNonce};

/// 支持的对称加密算法标识符 (Cipher ID)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CipherId {
    ChaCha20Poly1305 = 0x01,
    Aes256Gcm = 0x02,
    Aes128Gcm = 0x03,
}

impl TryFrom<u8> for CipherId {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(CipherId::ChaCha20Poly1305),
            0x02 => Ok(CipherId::Aes256Gcm),
            0x03 => Ok(CipherId::Aes128Gcm),
            _ => Err(Error::InvalidCipherId(value)),
        }
    }
}

/// 使用 Argon2id 从密码派生 32 字节 (256-bit) 的密钥。
/// 对于 AES-128-GCM，我们仅取前 16 字节。
pub fn derive_key(password: &[u8], salt: &[u8]) -> Result<[u8; 32], Error> {
    let mut okm = [0u8; 32];
    let params = Params::default();
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password_into(password, salt, &mut okm)
        .map_err(|_| Error::KeyDerivationFailed)?;
    Ok(okm)
}

/// 执行单块的就地解密 (In-place Decryption)，返回只包含明文的子切片
pub fn decrypt_chunk_in_place<'a>(
    cipher_id: CipherId,
    key: &[u8],
    nonce: &[u8],
    data: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    if data.len() < 16 {
        return Err(Error::DataTooShort);
    }
    let tag_pos = data.len() - 16;
    let (payload, tag_bytes) = data.split_at_mut(tag_pos);

    match cipher_id {
        CipherId::ChaCha20Poly1305 => {
            if key.len() < 32 {
                return Err(Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                });
            }
            let key_ref = ChaChaKey::try_from(&key[0..32]).map_err(|_| Error::InvalidKeyLength {
                expected: 32,
                actual: key.len(),
            })?;
            let nonce_ref = ChaChaNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let tag_ref = chacha20poly1305::Tag::try_from(&*tag_bytes)
                .map_err(|_| Error::InvalidTagLength {
                    expected: 16,
                    actual: tag_bytes.len(),
                })?;
            let cipher = ChaCha20Poly1305::new(&key_ref);
            cipher
                .decrypt_inout_detached(&nonce_ref, &[], payload.into(), &tag_ref)
                .map_err(|_| Error::DecryptionFailed)?;
        }
        CipherId::Aes256Gcm => {
            if key.len() < 32 {
                return Err(Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                });
            }
            let key_ref = AesKey::<Aes256Gcm>::try_from(&key[0..32]).map_err(|_| {
                Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                }
            })?;
            let nonce_ref = AesNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let tag_ref = aes_gcm::Tag::try_from(&*tag_bytes).map_err(|_| Error::InvalidTagLength {
                expected: 16,
                actual: tag_bytes.len(),
            })?;
            let cipher = Aes256Gcm::new(&key_ref);
            cipher
                .decrypt_inout_detached(&nonce_ref, &[], payload.into(), &tag_ref)
                .map_err(|_| Error::DecryptionFailed)?;
        }
        CipherId::Aes128Gcm => {
            if key.len() < 16 {
                return Err(Error::InvalidKeyLength {
                    expected: 16,
                    actual: key.len(),
                });
            }
            let key_ref = AesKey::<Aes128Gcm>::try_from(&key[0..16]).map_err(|_| {
                Error::InvalidKeyLength {
                    expected: 16,
                    actual: key.len(),
                }
            })?;
            let nonce_ref = AesNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let tag_ref = aes_gcm::Tag::try_from(&*tag_bytes).map_err(|_| Error::InvalidTagLength {
                expected: 16,
                actual: tag_bytes.len(),
            })?;
            let cipher = Aes128Gcm::new(&key_ref);
            cipher
                .decrypt_inout_detached(&nonce_ref, &[], payload.into(), &tag_ref)
                .map_err(|_| Error::DecryptionFailed)?;
        }
    }

    Ok(&mut data[..tag_pos])
}

/// 执行单块的就地加密 (In-place Encryption)
pub fn encrypt_chunk_in_place(
    cipher_id: CipherId,
    key: &[u8],
    nonce: &[u8],
    data: &mut Vec<u8>,
) -> Result<(), Error> {
    match cipher_id {
        CipherId::ChaCha20Poly1305 => {
            if key.len() < 32 {
                return Err(Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                });
            }
            let key_ref = ChaChaKey::try_from(&key[0..32]).map_err(|_| Error::InvalidKeyLength {
                expected: 32,
                actual: key.len(),
            })?;
            let nonce_ref = ChaChaNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let cipher = ChaCha20Poly1305::new(&key_ref);
            cipher
                .encrypt_in_place(&nonce_ref, &[], data)
                .map_err(|_| Error::EncryptionFailed)?;
        }
        CipherId::Aes256Gcm => {
            if key.len() < 32 {
                return Err(Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                });
            }
            let key_ref = AesKey::<Aes256Gcm>::try_from(&key[0..32]).map_err(|_| {
                Error::InvalidKeyLength {
                    expected: 32,
                    actual: key.len(),
                }
            })?;
            let nonce_ref = AesNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let cipher = Aes256Gcm::new(&key_ref);
            cipher
                .encrypt_in_place(&nonce_ref, &[], data)
                .map_err(|_| Error::EncryptionFailed)?;
        }
        CipherId::Aes128Gcm => {
            if key.len() < 16 {
                return Err(Error::InvalidKeyLength {
                    expected: 16,
                    actual: key.len(),
                });
            }
            let key_ref = AesKey::<Aes128Gcm>::try_from(&key[0..16]).map_err(|_| {
                Error::InvalidKeyLength {
                    expected: 16,
                    actual: key.len(),
                }
            })?;
            let nonce_ref = AesNonce::try_from(nonce).map_err(|_| Error::InvalidNonceLength {
                expected: 12,
                actual: nonce.len(),
            })?;
            let cipher = Aes128Gcm::new(&key_ref);
            cipher
                .encrypt_in_place(&nonce_ref, &[], data)
                .map_err(|_| Error::EncryptionFailed)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key() {
        let password = b"password123";
        let salt = b"constantsalt1234";
        let key = derive_key(password, salt).unwrap();
        assert_eq!(key.len(), 32);

        // 验证同样的密码和盐值能派生相同的密钥
        let key2 = derive_key(password, salt).unwrap();
        assert_eq!(key, key2);
    }

    #[test]
    fn test_encrypt_decrypt_all_ciphers() {
        let password = b"mysecretpassword";
        let salt = b"salt_for_testing";
        let key = derive_key(password, salt).unwrap();
        let nonce = [0u8; 12];
        let original_data = b"Hello, Rustrum secure streaming!".to_vec();

        let ciphers = [
            CipherId::ChaCha20Poly1305,
            CipherId::Aes256Gcm,
            CipherId::Aes128Gcm,
        ];

        for &cipher in &ciphers {
            let mut data = original_data.clone();
            encrypt_chunk_in_place(cipher, &key, &nonce, &mut data).unwrap();
            assert_ne!(data, original_data);

            let decrypted = decrypt_chunk_in_place(cipher, &key, &nonce, &mut data).unwrap();
            assert_eq!(decrypted, original_data);
        }
    }
}
