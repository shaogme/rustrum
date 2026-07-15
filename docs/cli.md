# Rustrum CLI 命令行工具文档

`rustrum-cli` 是 Rustrum 安全视频流播放体系的加密与解密工具。它负责在本地将视频（必须为 Fragmented MP4 格式）进行分片并使用 AEAD 对称加密算法加密，同时输出用于解密的索引描述文件（`.rstrm`）。它也支持将加密后的视频文件解密还原。

## 目录结构与模块说明

项目 CLI 代码位于 [crates/rustrum-cli/src](../crates/rustrum-cli/src) 目录下：

- [main.rs](../crates/rustrum-cli/src/main.rs): CLI 入口，解析命令行参数并分发命令。
- [encrypt.rs](../crates/rustrum-cli/src/encrypt.rs): 实现视频的 FMP4 预检、密钥派生、分片加密以及输出写入逻辑（支持单合并文件模式与多物理分片模式）。
- [decrypt.rs](../crates/rustrum-cli/src/decrypt.rs): 实现解密逻辑，读取 `.rstrm` 元数据，派生密钥，对密文进行并发解密并重构输出。
- [ffmpeg.rs](../crates/rustrum-cli/src/ffmpeg.rs): 实现了底层的 FMP4 格式判定算法（通过解析 ISO Base Media File Format 盒结构，如 `moof`、`styp`、`ftyp` 兼容品牌）以及获取媒体分片边界；同时集成了通过外部 `ffprobe` 命令行工具解析媒体文件高精度时长与带 codecs 参数的 `mime_type` 的逻辑。
- [io_util.rs](../crates/rustrum-cli/src/io_util.rs): 提供内存段多范围分配器 `SliceRangeAllocator`，以及连接 `std::io` 和 `rustrum-core` 的 I/O 适配器。

---

## 获取与编译安装

您可以通过以下几种方式获取并使用 `rustrum-cli`：

### 1. 从源码编译

如果您在本地开发环境中，可以直接使用 Rust 编译器和 Cargo 编译并运行 `rustrum-cli`：

```bash
# 编译并运行
cargo run --package rustrum-cli -- --help

# 编译生成 Release 版本可执行文件
cargo build --release --bin rustrum-cli

# 编译后的二进制文件位于：
# ./target/release/rustrum-cli (Linux/macOS) 或 ./target/release/rustrum-cli.exe (Windows)
```

### 2. 使用 GitHub Releases 预编译包

在项目发布新版本时，CI/CD 工作流会自动为主流操作系统编译二进制文件，并上传至 GitHub Releases。您可以直接在 Releases 页面下载对应系统的压缩包：

* **Linux**: `rustrum-cli-linux.tar.gz` (x86_64)
* **macOS**: 
  * `rustrum-cli-macos-x64.tar.gz` (Intel)
  * `rustrum-cli-macos-arm64.tar.gz` (Apple Silicon M系列)
* **Windows**: `rustrum-cli-windows.zip` (x86_64)

下载后解压，即可直接在终端中运行。

### 3. 使用 Docker 镜像

如果您不想在本地安装 Rust 环境，也可以直接使用工作流自动打包发布在 GitHub Container Registry (GHCR) ：

```bash
# 运行容器中的加密工具（将当前目录挂载到容器中）
docker run --rm -v $(pwd):/workspace -w /workspace ghcr.io/shaogme/rustrum:latest encrypt -i data/fmp4.mp4 -o web/public -p yourpassword
```

---

## 命令行使用说明

CLI 支持两个子命令：`encrypt`（加密）和 `decrypt`（解密）。

### 1. 加密视频文件 (encrypt)

将原始视频文件加密并生成 `.rstr`（密文数据）和 `.rstrm`（元数据描述文件）。

```bash
cargo run --bin rustrum-cli -- encrypt [OPTIONS] --input <INPUT> --output <OUTPUT>
```

#### 参数说明
- `-i, --input <INPUT>`: 输入的视频文件路径（必须为 FMP4 格式）。
- `-o, --output <OUTPUT>`: 输出目录路径。
- `-n, --name <NAME>`: 输出的文件名（可选，默认使用输入文件名主名）。
- `-p, --password <PASSWORD>`: 加密密码。如果未指定，程序会在终端安全提示用户输入。
- `-c, --cipher <CIPHER>`: 对称加密算法。可选值：
  - `chacha20-poly1305`（默认值）
  - `aes256-gcm`
  - `aes128-gcm`
- `--split`: 是否启用独立物理分片存储。
  - 若启用，则在输出目录生成多个独立的密文分片文件 `[name]_[index].rstr`。
  - 若未启用，则生成单个连续的密文数据文件 `[name].rstr`。
- `-t, --threads <THREADS>`: 限制并发加密时的线程数（默认使用 Rayon 自动管理的物理 CPU 核心数）。

---

### 2. 解密视频文件 (decrypt)

将加密后的文件解密并还原为原始格式视频。

```bash
cargo run --bin rustrum-cli -- decrypt [OPTIONS] --input <INPUT> --output <OUTPUT>
```

#### 参数说明
- `-i, --input <INPUT>`: 输入的密文文件路径（若为单文件模式则指向 `.rstr`，若为分片模式则指向 `[name].rstr`，程序会自动在同目录下寻找同名的 `.rstrm` 描述文件）。
- `-o, --output <OUTPUT>`: 解密后的原始视频输出路径。
- `-p, --password <PASSWORD>`: 解密密码。如果未指定，程序会提示输入。
- `-t, --threads <THREADS>`: 限制并发解密时的线程数。

---

## 核心技术实现

### 1. FMP4 格式预检与分片划分 (ffmpeg.rs)

在加密前，CLI 必须保证视频为 Fragmented MP4 (FMP4) 格式以支持流式解密。

- **格式判断**：`Fmp4Detector` 遍历读取 MP4 的顶层 Box 结构。如果检测到 `moof` 盒或 `styp` 盒，或者 `ftyp` 的兼容品牌列表包含 `iso5`, `iso6`, `dash`, `msdh`, `cmfc` 等，则判定为合格的 FMP4 格式。
- **边界划分**：利用 `moof`（Movie Fragment Box）在文件中的起始偏移量来确定加密分块的边界。第一个分片通常是 Init Segment（从文件开头到第一个 `moof` 的位置），后续每个分片由一个 `moof` 以及紧随其后的 `mdat` 构成。

### 2. 利用 ffprobe 提取媒体信息 (ffmpeg.rs)

为了让前端播放器在解密时能够精确配置 MSE 的解码管道，CLI 会在加密时调用系统中的 `ffprobe` 获取高精度时长与编解码参数：

- **调用逻辑**：通过启动外部 `ffprobe` 进程，执行 `ffprobe -v error -show_format -show_streams -of json <input>` 以 JSON 格式输出媒体流和容器元数据。
- **参数解析与组装**：
  - 从 `format.duration` 提取视频的总播放时长（秒）。
  - 遍历视频流和音频流，识别具体编码类型（如 `h264`, `hevc`, `aac` 等），并自动转换拼接成 MSE 支持的 codecs 规范字符串（如 `video/mp4; codecs="avc1.64001e, mp4a.40.2"`）。
  - 若 `ffprobe` 工具执行失败或未安装，系统将直接报错并终止加密流程，强制要求环境配置正确以保证输出的播放描述文件质量。

### 2. 高效 I/O 与并发加解密

为了在加密与解密大文件时实现极高性能，CLI 做了以下优化：

- **内存映射 (mmap)**：使用 `memmap2` 库将输入文件和输出文件直接映射 to 内存空间，减少由于用户态和内核态切换以及大缓冲区读写引发的额外拷贝。
- **并发加速**：基于 `rayon` 库，采用工作窃取 (Work-stealing) 模型对分片进行并发加密/解密。每个分片独立分配 Nonce 并由不同的线程并行计算。
- **内存安全切片分配 (SliceRangeAllocator)**：在内存映射文件中，通过 `SliceRangeAllocator` 将整个输出映射区域切割为多个互不重叠的 `&mut [u8]` 切片，并发线程可安全地就地写入数据，避免了动态内存分配和多线程锁竞争。
