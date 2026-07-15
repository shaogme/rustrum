# Rustrum Core 开发者文档

`rustrum-core` 是 Rustrum 安全视频流播放体系的底层核心库。它采用 `no_std` 设计，专注于高性能的加密算法封装、文件格式序列化/反序列化以及分片寻道定位逻辑，为 CLI 工具与 WASM 模块提供统一的底层支持。

## 目录结构与模块说明

项目核心代码位于 [crates/rustrum-core/src](../crates/rustrum-core/src) 目录下：

- [lib.rs](../crates/rustrum-core/src/lib.rs): 库入口，定义 `no_std` 属性，并导出各子模块。
- [crypto.rs](../crates/rustrum-core/src/crypto.rs): 封装密钥派生与基于 AEAD 的就地加解密算法。
- [header.rs](../crates/rustrum-core/src/header.rs): 定义 `.rstrm` 元数据头结构、索引表项以及序列化/反序列化逻辑，同时实现寻道分块定位算法。
- [io.rs](../crates/rustrum-core/src/io.rs): 定义抽象的 `Read` 与 `Write` 特性，适配 `no_std` 下的内存和流式操作。
- [error.rs](../crates/rustrum-core/src/error.rs): 统一定义库的错误类型 `Error`。

---

## 核心模块详解

### 1. 加密与密钥派生 (crypto)

该模块提供基于 Argon2id 的密钥派生以及三种主流 AEAD 对称加密算法的就地 (In-place) 加解密支持。

#### 算法标识符 (CipherId)
定义在 [crypto.rs](../crates/rustrum-core/src/crypto.rs) 中：
- `ChaCha20Poly1305` (0x01)
- `Aes256Gcm` (0x02)
- `Aes128Gcm` (0x03)

#### 主要 API
- **`derive_key(password: &[u8], salt: &[u8]) -> Result<[u8; 32], Error>`**
  使用 Argon2id 算法，从用户密码与 16 字节盐值中派生 32 字节的强对称密钥。
- **`decrypt_chunk_in_place<'a>(cipher_id: CipherId, key: &[u8], nonce: &[u8], data: &'a mut [u8]) -> Result<&'a mut [u8], Error>`**
  对传入的密文切片进行就地解密。该方法会验证 AEAD 的 Tag（数据的最后 16 字节），并在解密成功后返回指向明文数据的子切片，避免额外的内存分配。
- **`encrypt_chunk_in_place(cipher_id: CipherId, key: &[u8], nonce: &[u8], data: &mut Vec<u8>) -> Result<(), Error>`**
  对数据进行就地加密，并将生成的 AEAD Tag 追加至数据末尾。

---

### 2. 格式与索引表 (header)

该模块负责 `.rstrm` 索引描述文件的读写及逻辑定位。

#### 索引项 (IndexEntry)
表示视频流中单个分片（包含 Init Segment 和各个 Media Segment）的物理及加密信息：
```rust
pub struct IndexEntry {
    pub offset: u64,     // 在密文流文件中的物理起始偏移量（单文件模式递增，分片模式为 0）
    pub size: u64,       // 加密后该分块的大小（包含 16 字节的 AEAD Tag）
    pub nonce: [u8; 12], // 该分块专属的随机 Nonce
}
```

#### 文件头 (RstrHeader)
```rust
pub struct RstrHeader {
    pub version: u16,                // 协议版本号，当前为 0x0001
    pub cipher_id: CipherId,         // 加密算法标识
    pub is_split: bool,              // 是否为物理分片多文件模式
    pub mime_type: String,           // 视频 MIME 类型与 Codec 信息
    pub duration: f64,               // 视频总时长（秒）
    pub key_salt: [u8; 16],          // 用于 Argon2id 密钥派生的盐值
    pub index_table: Vec<IndexEntry>,// 分片索引表
}
```

#### 主要 API
- **`serialize<W: Write>(&self, mut writer: W) -> Result<(), Error>`**
  将头部及索引表序列化为二进制字节流。
- **`deserialize<R: Read>(mut reader: R) -> Result<Self, Error>`**
  从二进制读取器中解析并重构 `RstrHeader` 实例。
- **`locate_chunk(&self, byte_position: u64) -> Result<(usize, &IndexEntry), Error>`**
  **寻道核心算法**：根据未加密视频的原始字节偏移量 `byte_position`，遍历索引表累加计算每个分片的明文大小（`size - 16`），从而定位该字节属于哪一个加密分块（返回索引与对应的 `IndexEntry`）。

---

### 3. 输入输出抽象 (io)

由于底层库需要在 `no_std` 环境下运行，无法使用 `std::io::Read` 和 `std::io::Write`。因此在 [io.rs](../crates/rustrum-core/src/io.rs) 中定义了轻量级的抽象：

- **`Write` 特性**：包含 `write_all(&mut self, buf: &[u8])` 方法，并为 `&mut W` 和 `Vec<u8>` 实现了该特性。
- **`Read` 特性**：包含 `read_exact(&mut self, buf: &mut [u8])` 方法，并为 `&mut R` 和 `&[u8]` 实现了该特性。

---

### 4. 错误处理 (error)

统一错误类型定义在 [error.rs](../crates/rustrum-core/src/error.rs) 的 `Error` 枚举中。主要错误包括：
- `InvalidCipherId(u8)`: 无效的算法 ID。
- `KeyDerivationFailed`: Argon2id 密钥派生失败。
- `DataTooShort`: 数据长度不足以包含 16 字节的 AEAD Tag。
- `DecryptionFailed` / `EncryptionFailed`: 加解密库执行失败。
- `InvalidMagicNumber`: 文件魔数不匹配（标准为 `RSTR`）。
- `OutOfBounds`: 寻道位置超出了索引表的视频范围。

## 关联参考

- [系统总体设计说明](../DESIGN.md)
- [WASM 模块设计文档](./wasm.md)
