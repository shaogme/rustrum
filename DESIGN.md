# **Rustrum: 基于 WASM 的安全视频流播放器**

## **设计文档 (DESIGN.md)**

**版本:** 1.0.0  
**作者:** Arch Architect  
**状态:** 草案

---

## **1. 执行摘要 (Executive Summary)**

**Rustrum** 是一款高性能、安全、端到端加密的视频流解决方案。它由两个核心部分组成：
1. **基于 Rust 的命令行工具 (CLI)**：在本地对视频进行分片并使用现代 AEAD 对称加密算法进行加密。
2. **基于 WebAssembly (WASM) 的轻量级网页播放器**：在浏览器中实时解密并播放视频流。

Rustrum 的核心设计目标是在不可信的静态托管平台（如 GitHub Releases、Netlify、Cloudflare Pages）上实现**零知识（Zero-Knowledge）**视频托管，同时保证流畅的、原生级别的播放体验（包括即时拖动寻道和低内存占用）。

---

## **2. 系统架构 (System Architecture)**

系统被解耦为两个主要组件：**构建期加密流水线 (CLI)** 与 **运行期解密流水线 (浏览器)**。

### **2.1 高层工作流**

1. **CLI 工具 (Rust)**：读取标准视频 file -> 确保其为碎片化 MP4 (FMP4) 格式 -> 将其分割为天然的 FMP4 视频分片（或可选的固定大小块）并使用所选的对称加密算法进行独立加密 -> 生成包含加密算法标识等信息的元数据头部并输出独立的 `.rstrm` 元数据描述文件，同时输出仅由纯密文分片拼接而成的 `.rstr` 媒体文件 -> 上传至静态托管服务。
2. **网页客户端 (JS + Rust/WASM)**：获取轻量级的 `.rstrm` 索引元数据描述文件并解析 -> 使用媒体源扩展 (MSE, Media Source Extensions) 初始化 video 元素 -> 监听拖动 (seek) 和缓冲区事件 -> 通过 HTTP Range 请求直接在 `.rstr` 文件中按需拉取特定区间（偏移从 0 开始）的加密分块 -> 将加密分块传递给 WASM 模块进行解密 -> 将解密后的片段直接追加到 MSE 的 `SourceBuffer` 中。

---

## **3. 深度剖析：解决关键技术挑战**

### **3.1 挑战 1：任意拖动播放 (即时寻道支持)**

在标准的流式加密中，密文是连续的。解密任意偏移量处的数据需要先解密其前面所有的内容，这在视频播放中进行即时拖动 (Seek) 是无法接受的。

#### **解决方案：基于索引的分片加密与多算法兼容设计**

Rustrum 支持将视频分割为对齐 FMP4 天然边界的媒体分段（Init Segment 和 Moof Fragments）。对每个分段/分片使用支持的 AEAD 算法进行独立加密，每次加密使用唯一生成的 Nonce。

Rustrum 的元数据采用独立文件存储以提高流媒体加载效能。存储元数据的 `.rstrm` 索引文件结构布局设计如下：

| 字段 | 大小 (字节) | 描述 |
| :--- | :--- | :--- |
| **魔数 (Magic Number)** | 4 | ASCII 字符 `RSTR`，用于标识文件格式。 |
| **版本号 (Version)** | 2 | 文件格式版本号（例如 `0x0001`）。 |
| **算法标识符 (Cipher ID)** | 1 | `0x01`: ChaCha20-Poly1305<br>`0x02`: AES-256-GCM<br>`0x03`: AES-128-GCM |
| **分片标记 (Is Split)** | 1 | `0x01` 表示物理分片多文件，`0x00` 表示单密文合并文件。 |
| **MIME 类型 (Mime Type)** | 1 + M | 1 字节长度加上实际的 UTF-8 编码 MIME 类型字符串。 |
| **密钥盐值 (Key Salt)** | 16 | 用于 Argon2id 从用户密码派生加密密钥的随机盐值。 |
| **视频总时长 (Duration)** | 8 | 视频总时长 (f64)。 |
| **索引表长度 (Index Table Length)** | 4 | 分块总数 (N)。 |
| **索引表 (Index Table)** | N * 28 | 每个表项包含：在密文文件中的分块相对偏移量 Offset (8B)、加密分块大小 Size (8B) 以及分块 Nonce (12B)。 |

对于密文媒体数据的存储形式，元数据中的 **分片标记 (Is Split)** 将指示：
* **单合并密文模式 (`Is Split` 为 `0x00`)**：生成单个 `.rstr` 媒体文件，直接由连续拼接的加密分片组成：`[Chunk 0][Chunk 1]...`，各分片 `offset` 在文件中单调递增。
* **物理分段多文件模式 (`Is Split` 为 `0x01`)**：将分片直接存为多个独立的文件：`video_0.rstr`, `video_1.rstr` ...，索引表中各分片的 `offset` 统一置为 `0`。

> [!NOTE]
> 对于所支持的 AEAD 对称加密算法（如 ChaCha20-Poly1305、AES-GCM），标准 Nonce 长度均为 12 字节，这使得索引表中 Nonce 的存储大小可以保持统一。

#### **寻道算法步骤：**

1. **加载头部**：JS 播放器在初始化时，请求轻量级的 `.rstrm` 描述文件来解析元数据信息、确定**算法标识符**并加载索引表。
2. **触发寻道**：当用户在播放器上拖动进度条到时间 T 时，HTMLMediaElement 触发 `seeking` 事件。
3. **计算逻辑偏移**：播放器利用视频容器的内部索引表估算所需的数据偏移量，设对应的原始未加密字节位置为 B。
4. **定位分块**：动态累加索引表中每个加密块的未加密大小（`Size - 16`），计算出每个分片在内存中的明文区间，并以此定位到包含 B 的目标分块索引。
5. **拉取数据**：查询内存中的索引表，获取该分块在 `.rstr` 纯数据流文件中的物理字节区间（无 Header 干扰，从 `Offset` 开始到 `Offset+Size-1`），并向服务器发起 Range 请求。
6. **动态解密与播放**：将拉取到的单块密文连同索引表中的 Nonce、元数据中指示的 **Cipher ID** 传递给 WASM。WASM 自动选择对应的解密算法（如 AES-GCM 或 ChaCha20-Poly1305）进行解密，解密后的明文直接送入 MSE 缓冲区。

---

### **3.2 挑战 2：浏览器 MSE 与视频格式限制**

HTML5 媒体源扩展 (MSE) 无法直接消费常规的 MP4、AVI 或 MKV 等封装格式。它严格要求视频流采用**碎片化 MP4 (Fragmented MP4, FMP4)** 或 WebM 格式，即媒体元数据（`moov` atom）必须置于实际媒体碎片（`moof`/`mdat`）之前。

#### **解决方案：集成转码预检流水线**

为了避免播放失败，Rustrum CLI 集成了一个验证与自动转码预检模块（使用 FFmpeg 库或系统安装的 FFmpeg 绑定）：

1. **格式检查**：CLI 实现了基于 Box 链流式解析的 FMP4 标准判定算法。它通过解析 `ftyp` 的兼容品牌列表（兼容 `iso5`, `iso6`, `dash`, `msdh`, `cmfc` 等特征），或检测顶层是否存在 `moof`（Movie Fragment Box）或 `styp`（Segment Type Box）盒进行精准且零误报的格式验证。同时，支持检测本地 `ffprobe` 作为辅助验证手段。
2. **流式加密**：转码生成的 `output_fragmented.mp4` 被直接送入分片加密处理器。这保证了解密后的输出流能够被浏览器的 `SourceBuffer` 完美解析。

---

### **3.3 挑战 3：内存占用与数据拷贝开销控制**

在 JavaScript 垃圾回收堆 (JS Heap) 与 WebAssembly 线性内存空间之间高频传递大容量视频字节数组，会导致严重的内存碎片、垃圾回收抖动以及极高的 CPU 拷贝开销。

#### **解决方案：零拷贝共享内存环形缓冲区 (Zero-Copy Shared Memory Arena)**

Rustrum 避免了每次解密都重新分配数组，而是在 WebAssembly 线性内存中开辟了一块预先分配好的**共享内存竞技场 (Shared Memory Arena)**。

1. **预分配内存**：网页播放器初始化时，Rust WASM 模块会在其线性内存中静态分配一块略大于两个加密块（例如 3 MB）的缓冲区，并将该内存指针暴露给 JavaScript。
2. **直接写入**：当 JavaScript 获取到加密的分块数据后，不经过任何中间转换，直接通过 `WebAssembly.Memory.buffer` 将网络 Payload 写入到指向 WASM 线性内存的 `Uint8Array` 视图中（使用 `Uint8Array.set()`）。
3. **就地解密**：Rust WASM 模块根据 Header 中的 **Cipher ID** 实例化相应的解密器，对该缓冲区的数据进行**就地 (In-place) 解密**，或者将解密出的明文写入到竞技场中专门的输出槽位。
4. **零拷贝切片传递**：JS 通过 `new Uint8Array(wasm_memory, decrypted_ptr, decrypted_len)` 获得该解密内存段的视口引用，并立即调用 `sourceBuffer.appendBuffer()`。数据直接递交给浏览器的硬件解码器，整个过程实现了零 JS 堆内存分配和零冗余拷贝。

---

## **5. 技术栈 (Technology Stack)**

* **CLI 工具 (加密端)**：
  * 开发语言：Rust
  * 密码学库：`chacha20poly1305`、`aes-gcm`、`argon2` (均来自 RustCrypto 组织)
  * 命令行交互：`clap` (命令行参数解析器)
* **网页播放器 (解密及播放端)**：
  * 前端封装：TypeScript / HTML5
  * WASM 核心：Rust (通过 `wasm-pack` / `wasm-bindgen` 编译构建)
  * 播放引擎：HTML5 `<video>` + 媒体源扩展 (MSE)

---

## **6. 安全考量 (Security Considerations)**

1. **密钥派生安全性**：加密密钥通过 Argon2id 算法从用户密码派生，并配合存储在文件头中的唯一 Salt，能有效抵御离线彩虹表与暴力破解攻击。
2. **密码学真实性 (Authenticity)**：所有支持的算法均采用 **AEAD**（ChaCha20-Poly1305 / AES-GCM）模式，可同时确保机密性与完整性。即使密文视频文件被篡改了单个比特，也会在解密时触发校验失败，从而彻底杜绝视频注入或重放攻击。
3. **密钥生命周期保护**：解密密钥严格保存在 WASM 线性内存及瞬时 JS 局部变量中。除非用户明确授权记住密码以便恢复会话，否则不会将密钥写入 `localStorage` 或任何持久化缓存中。