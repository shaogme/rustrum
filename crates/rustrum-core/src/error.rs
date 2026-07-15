use alloc::string::String;
use core::fmt;

/// rustrum-core 库的统一错误类型
#[derive(Debug)]
pub enum Error {
    /// 算法标识符无效
    InvalidCipherId(u8),
    /// 密钥派生失败
    KeyDerivationFailed,
    /// 数据过短，无法包含 Tag
    DataTooShort,
    /// 密钥长度不合法
    InvalidKeyLength { expected: usize, actual: usize },
    /// Nonce 长度不合法
    InvalidNonceLength { expected: usize, actual: usize },
    /// Tag 长度不合法
    InvalidTagLength { expected: usize, actual: usize },
    /// 解密失败
    DecryptionFailed,
    /// 加密失败
    EncryptionFailed,
    /// I/O 错误
    Io(String),
    /// 文件魔数不正确
    InvalidMagicNumber,
    /// 原始字节位置越界
    OutOfBounds,
    /// MIME 类型过长
    MimeTypeTooLong,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidCipherId(val) => write!(f, "Invalid Cipher ID: {val}"),
            Error::KeyDerivationFailed => write!(f, "Failed to derive key using Argon2id"),
            Error::DataTooShort => write!(f, "Data is too short to contain tag"),
            Error::InvalidKeyLength { expected, actual } => {
                write!(f, "Invalid key length: expected {expected}, got {actual}")
            }
            Error::InvalidNonceLength { expected, actual } => {
                write!(f, "Invalid nonce length: expected {expected}, got {actual}")
            }
            Error::InvalidTagLength { expected, actual } => {
                write!(f, "Invalid tag length: expected {expected}, got {actual}")
            }
            Error::DecryptionFailed => write!(f, "Decryption failed"),
            Error::EncryptionFailed => write!(f, "Encryption failed"),
            Error::Io(msg) => write!(f, "I/O error: {msg}"),
            Error::InvalidMagicNumber => write!(f, "Invalid file magic number"),
            Error::OutOfBounds => write!(f, "Byte position out of video bounds"),
            Error::MimeTypeTooLong => write!(f, "MIME type is too long (max 255 bytes)"),
        }
    }
}

impl core::error::Error for Error {}
