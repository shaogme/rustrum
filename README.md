# Rustrum: 基于 WebAssembly 的端到端安全视频流播放系统

Rustrum 是一款高性能、安全、端到端加密的视频流解决方案。它由基于 Rust 的命令行加密/解密工具 (CLI) 和基于 WebAssembly (WASM) + 媒体源扩展 (MSE) 的轻量级网页播放器 SDK 组成。

Rustrum 的核心设计目标是在不可信的静态托管平台（如 GitHub Pages、Cloudflare Pages）上实现零知识 (Zero-Knowledge) 视频托管，同时保证流畅的、原生级别的播放体验（包括即时拖动寻道和低内存占用）。

您可以通过 [GitHub Pages 在线演示页面](https://shaogme.github.io/rustrum/) 直接体验。该页面由 [.github/workflows/deploy-pages.yml](.github/workflows/deploy-pages.yml) 持续集成自动构建并部署。

---

## 目录

- [核心架构](#核心架构)
- [核心设计与技术亮点](#核心设计与技术亮点)
- [项目目录结构与模块说明](#项目目录结构与模块说明)
- [快速上手](#快速上手)
  - [构建与安装 CLI 工具](#构建与安装-cli-工具)
  - [使用 Docker 运行 CLI](#使用-docker-运行-cli)
  - [使用 CLI 加密视频](#使用-cli-加密视频)
  - [使用 CLI 解密视频](#使用-cli-解密视频)
  - [本地 Web 演示部署](#本地-web-演示部署)
- [前端 SDK 接入指南](#前端-sdk-接入指南)
  - [安装与引入](#安装与引入)
  - [React Hooks 示例](#react-hooks-示例)
  - [智能预加载与寻道](#智能预加载与寻道)
- [安全性与优化建议](#安全性与优化建议)
- [许可证](#许可证)

---

## 核心架构

Rustrum 将流程解耦为构建期加密流水线与运行期解密播放管线：

1. **构建期 (CLI 加密端)**：
   - 提取输入视频文件，验证其是否为碎片化 MP4 (FMP4) 格式。如果未安装 `ffprobe` 或不是 FMP4，则强制要求环境适配或抛出错误。
   - 通过 Argon2id 算法，将用户输入的密码与 16 字节随机盐值派生出强对称密钥。
   - 对 FMP4 的媒体分片（基于 `moof` 盒物理边界）进行并发 AEAD 加密，为每个分片分配独立 Nonce。
   - 输出 `.rstrm` 索引元数据文件，同时生成合并的密文媒体文件 `.rstr`（或独立的物理分片文件）。

2. **运行期 (浏览器解密播放端)**：
   - 网页端先获取轻量级的 `.rstrm` 描述文件并解析出元数据与索引表。
   - 当播放器初始化或用户进行拖动寻道 (Seek) 时，根据明文视频的偏移量计算对应的加密分片索引。
   - 播放器发起带有 HTTP Range 请求的按需拉取，直接将密文分片流加载至 WASM 共享内存缓冲区。
   - WASM 模块采用就地 (In-place) 零拷贝解密，将明文字节直接以视口引面的形式递交给媒体源扩展 (MSE) 播放管线。

---

## 核心设计与技术亮点

### 1. 任意拖动播放 (即时寻道)
在流式加密中，密文通常是连续的。解密任意偏移处的数据通常需要先解密前面的所有密文，这对于视频播放是不现实的。
Rustrum 通过引入独立分片加密与独立 Nonce，结合自定义索引描述文件 `.rstrm`，在播放时实现了精确的字节寻道。当播放器触发 Seek 时，可通过内部计算将时间或明文偏移量换算为分片索引，并利用 HTTP Range 请求下载特定的加密片段，实现毫秒级的即时寻道响应。

### 2. 零拷贝共享内存 (Zero-Copy Shared Memory Arena)
为避免在 JavaScript 垃圾回收堆与 WebAssembly 线性内存空间之间传递大体积字节数组时产生的严重内存碎片与 CPU 拷贝开销，本系统设计了共享内存区。
- **预分配内存**：WASM 模块在初始化时，会在其线性内存中静态分配一块略大于最大加密分片大小的缓冲区。
- **直接写入**：JavaScript 将网络获取的密文 Payload 直接通过 `WebAssembly.Memory.buffer` 写入 WASM 内部该指针地址。
- **就地解密**：WASM 根据指定的加密算法在同一缓冲区内就地 (In-place) 解密，明文直接覆盖密文。
- **视口引用**：JS 通过指针建立对明文段的只读视图引用并直接通过 `appendBuffer()` 送入 MSE 管道。

### 3. FMP4 格式预检与 MSE 播放管线
HTML5 媒体源扩展 (MSE) 无法直接播放常规 MP4 视频，它要求视频文件采用碎片化 MP4 (Fragmented MP4) 格式。CLI 工具集成了基于 Box 链流式解析的 FMP4 预检机制（通过检测 `moof`、`styp` 盒或包含特定特征的 `ftyp` 兼容品牌列表），并且利用系统安装的 `ffprobe` 自动提取时长与精确的 MIME 编解码器信息（例如 `video/mp4; codecs="avc1.64001e,mp4a.40.2"`），保证了播放侧的高兼容性。

---

## 项目目录结构与模块说明

项目采用了基于 Cargo 和 pnpm 的 Monorepo 目录结构：

- [crates/rustrum-core](crates/rustrum-core): 核心库。`no_std` 设计，实现 AEAD 加密（ChaCha20Poly1305, AES-GCM）、Argon2id 密钥派生、`.rstrm` 文件头的序列化与反序列化，以及定位寻道的底层算法。
- [crates/rustrum-cli](crates/rustrum-cli): 命令行工具。封装文件 I/O 适配器、FFmpeg 判定与 `ffprobe` 调用、并发 Rayon 就地加密与解密处理逻辑。
- [crates/rustrum-wasm](crates/rustrum-wasm): WebAssembly 包装层。通过 `wasm-bindgen` 向前端导出 API，实现基于共享内存的零拷贝解密。
- [packages/sdk](packages/sdk): 前端 SDK 封装包。实现 `RustrumPlayer` 类、智能流式加载预分配、解密重构、MSE 绑定以及专用的 React Hooks。
- [packages/web](packages/web): 演示网页应用。使用 React、Lucide React 和 Tailwind 驱动，提供直观的视频播放、密钥配置、解密日志控制台以及可视化的媒体分片网格监测。
- [docs](docs): 模块说明文档，包括：
  - [docs/core.md](docs/core.md): Core 核心底层设计文档
  - [docs/cli.md](docs/cli.md): CLI 工具的使用与技术文档
  - [docs/wasm.md](docs/wasm.md): WASM 接口与零拷贝共享内存工作流文档
  - [docs/sdk.md](docs/sdk.md): 前端 SDK API 及组件开发指南

---

## 快速上手

### 构建与安装 CLI 工具

在本地开发环境中，需要安装好 Rust 编译器工具链。

```bash
# 从源码编译 Release 版本的命令行二进制文件
cargo build --release --bin rustrum-cli

# 编译生成的二进制文件位于：
# ./target/release/rustrum-cli (Linux/macOS) 或 ./target/release/rustrum-cli.exe (Windows)
```

您也可以在发布新版本时从 GitHub Releases 直接下载对应操作系统的压缩包：
- Linux: `rustrum-cli-linux.tar.gz` (x86_64)
- macOS (Intel): `rustrum-cli-macos-x64.tar.gz`
- macOS (Apple Silicon): `rustrum-cli-macos-arm64.tar.gz`
- Windows: `rustrum-cli-windows.zip` (x86_64)

### 使用 Docker 运行 CLI

若本地没有配置 Rust 环境，可以直接使用自动构建发布的镜像：

```bash
docker run --rm -v $(pwd):/workspace -w /workspace ghcr.io/shaogme/rustrum:latest encrypt -i data/fmp4.mp4 -o web/public -p yourpassword
```

### 使用 CLI 加密视频

```bash
# 将原始视频加密并生成 .rstr 和 .rstrm 文件
cargo run --bin rustrum-cli -- encrypt \
  --input data/fmp4.mp4 \
  --output data/out \
  --password yourpassword \
  --cipher chacha20-poly1305
```

可选参数：
- `-i, --input`: 输入视频路径（必须为 FMP4 格式）。
- `-o, --output`: 输出的目标目录。
- `-p, --password`: 加密密码（如缺省，会在终端交互式输入）。
- `-c, --cipher`: 加密算法（支持 `chacha20-poly1305`、`aes256-gcm`、`aes128-gcm`，默认为 `chacha20-poly1305`）。
- `--split`: 是否启用独立物理分片存储。开启后会生成多个 `[name]_[index].rstr` 文件；默认不开启，生成单个密文数据文件 `[name].rstr`。
- `-t, --threads`: 限制加密线程数。

#### 视频源文件格式要求 (FMP4)

Rustrum 要求输入的源视频文件必须为 **Fragmented MP4 (FMP4)** 格式。普通的 MP4 文件无法直接用于分片加密和流式播放。

如果您的源视频文件是普通 MP4，请先使用 FFmpeg 转换为 FMP4。

1. **仅转换容器封装（不进行重编码，速度极快，适用于源视频本身已是 H.264/AAC 编码）**：
   ```bash
   ffmpeg -i input.mp4 -c copy -movflags empty_moov+omit_tfhd_offset+frag_keyframe+default_base_moof output.mp4
   ```

2. **重新编码为兼容性最佳的 H.264 + AAC 编码 FMP4（推荐，能保证在大部分浏览器下正常解码播放）**：
   ```bash
   ffmpeg -i input.mp4 -c:v libx264 -c:a aac -movflags empty_moov+omit_tfhd_offset+frag_keyframe+default_base_moof output.mp4
   ```

### 使用 CLI 解密视频

可以使用解密子命令还原出原始视频以验证正确性：

```bash
cargo run --bin rustrum-cli -- decrypt \
  --input data/out/fmp4.rstr \
  --output data/out/decrypted.mp4 \
  --password yourpassword
```

### 本地 Web 演示部署

本地需要安装 Node.js (v23+) 和 pnpm，通过以下命令启动 React 开发演示服务：

```bash
# 安装依赖
pnpm install

# 构建 WASM 与前端 SDK
pnpm --filter rustrum-sdk build

# 启动 Web 演示应用
pnpm --filter web run dev
```

浏览器打开输出的本地地址（通常是 http://localhost:5173 ）即可进入演示面板进行播放测试。

---

## 前端 SDK 接入指南

### 安装与引入

在您的 monorepo 应用中直接引入：

```bash
pnpm add rustrum-sdk --filter <your-app-name>
```

如果是外部独立工程，也可以通过在 GitHub Releases 下载打包的 `rustrum-sdk-<version>.tgz` 进行本地离线安装：

```bash
pnpm add ./rustrum-sdk-<version>.tgz
```

### React Hooks 示例

SDK 提供了 `useRustrumPlayer` Hook。它可以自动接管播放器的生命周期与状态更新：

```tsx
import React, { useRef } from 'react';
import { useRustrumPlayer } from 'rustrum-sdk';

export default function PlayerComponent() {
  const videoRef = useRef<HTMLVideoElement>(null);
  
  const {
    isPlaying,
    currentTime,
    duration,
    metadata,
    load,
    togglePlay
  } = useRustrumPlayer(videoRef);

  const handleStart = async () => {
    // 载入索引描述、加密视频流与密码
    await load(
      '/media/video.rstrm', // 支持下载 URL 或已经下载 of ArrayBuffer
      '/media/video.rstr',   // 支持下载 URL 或本地 File 对象
      'yourpassword'         // 解密密码
    );
  };

  return (
    <div>
      <video ref={videoRef} className="video-player" />
      <button onClick={handleStart}>初始化并加载视频</button>
      <button onClick={togglePlay}>{isPlaying ? '暂停' : '播放'}</button>
      {metadata && (
        <p>正在播放：{metadata.duration.toFixed(2)} 秒的 {metadata.cipherName} 加密流</p>
      )}
    </div>
  );
}
```

### 智能预加载与寻道

- **HTTP Range Requests**：为发挥最佳的分片寻道性能，承载 `.rstr` 视频文件的静态 HTTP 服务器必须支持 **HTTP Range Requests** (响应 `206 Partial Content`)。如果服务器不支持该请求，SDK 仍然可以运行，但将被迫退化为加载和解密整个密文文件。
- **预加载缓冲区限制**：您可以在实例化 `RustrumPlayer` 时通过指定 `maxPreloadSegments` 参数控制内存占用的上限（限制在客户端预加载的解码分片数量）。

---

## 安全性与优化建议

1. **强密钥派生**：通过 Argon2id 对密码和独有的随机盐进行派生，保障了应对暴力破译或彩虹表攻击的韧性。
2. **完整性校验**：所用的三种算法均为 **AEAD** 加密算法。一旦有人恶意篡改密文流或更改分片顺序，AEAD 的解密校验将直接引发报错终止播放，防止注入重放攻击。
3. **安全内存生命周期**：解密密钥和解密缓冲数据严格保留在 JS 局部变量与 WebAssembly 的独立线性内存段中，在组件/播放器销毁 (`player.destroy()`) 时立刻会被覆盖、清空和释放，避免内存泄漏与密钥持久落盘。

---

## 许可证

本项目基于双重授权协议发布，您可以选择以下任一许可证进行使用和分发：

- [Apache License, Version 2.0 (LICENSE-APACHE)](LICENSE-APACHE)
- [MIT License (LICENSE-MIT)](LICENSE-MIT)
