# Rustrum SDK 开发者文档

Rustrum SDK 是一个高性能、安全、基于 WebAssembly (WASM) 和媒体源扩展 (MSE) 的前端视频流解密播放开发工具包。它支持对通过 Rustrum CLI 加密的视频流（.rstr 格式）及其索引描述文件（.rstrm 格式）进行即时、按需的流式解密与原生的流畅播放，具备极速的拖动寻道能力和卓越的内存性能。

## 核心设计原理

在不可信的静态托管平台上实现端到端加密视频播放，面临着拖动寻道慢、内存拷贝开销大、视频格式兼容难三大核心痛点。Rustrum SDK 通过以下方案攻克了这些技术挑战：

1. **分片独立加密与即时寻道**：视频文件在构建期被拆分为符合 Fragmented MP4 (FMP4) 标准的天然媒体分片（Init Segment 及多个 Media Segment）。每个分片使用高强度 AEAD 算法（如 ChaCha20-Poly1305、AES-GCM）进行独立加密并分配唯一 Nonce。播放器在初始化时仅需加载极小的 .rstrm 索引描述文件，当用户拖动进度条时，播放器根据索引表精确定位并拉取特定偏移量范围的密文分片，从而实现毫秒级即时寻道。
2. **零拷贝共享内存 (Zero-Copy Shared Memory)**：为避免在 JavaScript 垃圾回收堆与 WebAssembly 线性内存之间复制大体积视频字节数组，SDK 将网络拉取的密文直接写入预先分配的 WASM 线性内存缓冲区中，并在该缓冲区内完成就地 (In-place) 解密。JavaScript 通过视口引用的方式将明文切片直接追加至 MSE 的 SourceBuffer 中，实现了零 JS 堆分配和零冗余内存拷贝。
3. **媒体源扩展 (MSE) 播放管线**：解密后的明文 FMP4 数据直接喂入浏览器的 HTML5 Media Source Extensions 管道，充分利用硬件解码加速，提供与普通视频完全一致的流畅度、多音轨及分辨率兼容性。

## 安装与引入

根据您的开发场景，您可以通过以下几种方式获取并引入 Rustrum SDK。

### 1. 本地 monorepo 开发引用

如果您在 Rustrum 本地工作区开发：
* SDK 源码及构建脚本位于本地的 `packages/sdk` 目录。
* 您可以通过 monorepo 的工作区机制直接在同一项目下的应用中引入：
  ```bash
  pnpm add rustrum-sdk --filter <your-app-name>
  ```

### 2. 使用 GitHub Releases 预编译包（离线 Tarball 安装）

在生产或正式发布流程中，Rustrum 的 CI/CD 工作流（由 `.github/workflows/release.yml` 驱动）会在发布 Tag 时自动打包并上传 SDK 产物。您可以从 GitHub Releases 中下载对应版本的打包文件 `rustrum-sdk-<version>.tgz`，并执行以下命令安装：

```bash
# 使用 npm 安装本地 tgz 包
npm install ./rustrum-sdk-<version>.tgz

# 使用 pnpm 安装本地 tgz 包
pnpm add ./rustrum-sdk-<version>.tgz

# 使用 yarn 安装本地 tgz 包
yarn add link:./rustrum-sdk-<version>.tgz
```

### 3. 直接使用编译产物归档（非工程化项目/静态托管）

如果您的项目没有使用构建打包工具，或者需要以纯静态资源形式引用 SDK，您可以在 GitHub Releases 中下载 `rustrum-sdk-dist.zip` 归档文件。
* 解压后可直接获得编译生成的 JavaScript 代码、声明文件以及 WebAssembly 二进制核心模块。
* 适合将其作为静态资源托管于您的 Web 服务器上，或者通过 CDN/静态目录直接使用。

### 导入 SDK 模块

在工程化项目中，安装完成后可通过如下方式在代码中引入：

```typescript
import {
  RustrumPlayer,
  useRustrum,
  initWasm,
  WasmDecoder
} from 'rustrum-sdk';
```

## 核心 API 参考

### RustrumPlayer 类

`RustrumPlayer` 是 SDK 的核心播放器控制器，负责将 HTML5 `<video>` 元素与基于 WASM 的流式解密流水线进行绑定，并全面接管 MSE 的生命周期。

#### 构造函数

```typescript
const player = new RustrumPlayer(videoElement: HTMLVideoElement, options?: RustrumPlayerOptions);
```

* **videoElement**: 需要绑定的 HTML5 `<video>` 元素实例。
* **options**: 播放器可选配置参数，详见下文 `RustrumPlayerOptions` 结构。

#### load() 方法

加载元数据索引以及关联的密文媒体源，并进行解密初始化。

```typescript
public async load(
  rstrmSource: ArrayBuffer | string,
  rstrSource: File | string,
  password: string
): Promise<RustrumMetadata>
```

* **参数**：
  - `rstrmSource`: `.rstrm` 索引描述文件内容（可以是已经获取到的 `ArrayBuffer`，也可以是其下载 URL 字符串）。
  - `rstrSource`: 加密视频密文数据（可以是本地上传的 `File` 对象，也可以是其下载 URL 字符串）。如果传入 URL 字符串，播放器将采用 HTTP Range 请求进行按需切片加载。
  - `password`: 用户输入的解密密码，用于通过 Argon2id 派生解密密钥。
* **返回值**：返回一个 Promise，解析后得到 `RustrumMetadata` 视频元数据。
* **异常**：如果密码错误、WASM 初始化失败或加载被后续并发 load 中断，将抛出异常。

#### destroy() 方法
销毁播放器实例，释放所有资源。

```typescript
public destroy(): void
```

* **说明**：会解绑视频元素的所有事件监听器、销毁内部的 `WasmDecoder`、清空缓存队列并断开 MSE 的连接。

#### play() 方法
开始播放视频。

```typescript
public play(): Promise<void>
```

#### pause() 方法
暂停视频播放。

```typescript
public pause(): void
```

#### togglePlay() 方法
切换视频的播放/暂停状态。

```typescript
public togglePlay(): void
```

#### getVolume() / setVolume() 方法
获取/设置视频音量。

```typescript
public getVolume(): number
public setVolume(volume: number): void
```

* **参数**：`volume` 为 0.0 到 1.0 之间的数值。

#### isMuted() / setMuted() 方法
获取/设置静音状态。

```typescript
public isMuted(): boolean
public setMuted(muted: boolean): void
```

#### getCurrentTime() / setCurrentTime() 方法
获取/设置当前播放时间进度（以秒为单位）。

```typescript
public getCurrentTime(): number
public setCurrentTime(time: number): void
```

#### getDuration() 方法
获取当前视频总时长。

```typescript
public getDuration(): number
```

#### getPlaybackRate() / setPlaybackRate() 方法
获取/设置视频播放倍速。

```typescript
public getPlaybackRate(): number
public setPlaybackRate(rate: number): void
```

#### getBufferedTimeRanges() 方法
获取已下载并成功解密缓冲的视频时间范围。

```typescript
public getBufferedTimeRanges(): { start: number; end: number }[]
```

#### getMetadata() 方法
获取当前加载的视频元数据。

```typescript
public getMetadata(): RustrumMetadata | null
```

* **返回值**：`RustrumMetadata` 对象，未加载时返回 `null`。

#### getSegmentStatuses() 方法
获取所有媒体分片的当前加载与解密状态。

```typescript
public getSegmentStatuses(): ('pending' | 'loading' | 'decrypted' | 'active')[]
```

* **返回值**：表示各分片状态的数组。状态值包括：
  - `pending`: 处于等待加载状态。
  - `loading`: 正在从网络或文件读取并进行解密。
  - `decrypted`: 已解密完成并已追加至浏览器 MSE 缓冲区。
  - `active`: 当前视频正在播放该分片对应的时间区间。

#### getSegmentsInfo() 方法
获取从索引表解析出的所有媒体分片的物理偏移和大小信息。

```typescript
public getSegmentsInfo(): { index: number; offset: bigint; size: bigint }[]
```

* **返回值**：分片物理信息列表（第 0 块为 Init Segment 初始化分片，通常不列入该返回，或根据实现自第 1 块开始）。

---

### React Hooks

SDK 提供了两个自定义 Hooks，方便在 React 框架下开发：

#### 1. useRustrum Hook
针对 React 框架提供的基础 Hook，封装了底层的 WASM 初始化、密码派生及底层加解密工具函数。

```typescript
export function useRustrum(): UseRustrumReturn;
```

##### 返回值 (UseRustrumReturn) 属性说明
* **isLoading**: `boolean`，指示 WASM 核心模块是否正在初始化。
* **error**: `string | null`，初始化或解密过程中遇到的错误信息。
* **wasmInstance**: `InitOutput | null`，初始化后的 WASM 实例。
* **initialize**: `(wasmInput?: ArrayBuffer | WebAssembly.Module) => Promise<InitOutput>`，手动初始化 WASM 模块的方法。
* **deriveKey**: `(password: string, salt: Uint8Array) => Uint8Array`，底层密码派生函数（内部使用 Argon2id）。
* **parseHeader**: `(headerBytes: Uint8Array) => WasmRstrHeader`，解析 `.rstrm` 文件二进制头信息。
* **decryptChunk**: `(cipherId: number, key: Uint8Array, nonce: Uint8Array, encryptedData: Uint8Array) => Uint8Array`，单块解密函数。
* **createDecoder**: `(headerBytes: Uint8Array, password: string) => WasmDecoder`，创建底层的 WASM 解码器实例。

#### 2. useRustrumPlayer Hook
高级控制 Hook，自动管理 `RustrumPlayer` 实例的生命周期（组件卸载时自动销毁），并将播放器状态转化为响应式的 React 状态。

```typescript
export function useRustrumPlayer(
  videoRef: RefObject<HTMLVideoElement | null>,
  options?: RustrumPlayerOptions
): UseRustrumPlayerReturn;
```

##### 返回值 (UseRustrumPlayerReturn) 属性说明
* **player**: `RustrumPlayer | null`，底层的播放器实例。
* **isPlaying**: `boolean`，当前是否正在播放。
* **currentTime**: `number`，当前播放进度（秒）。
* **duration**: `number`，视频总时长（秒）。
* **volume**: `number`，当前音量大小（0.0 ~ 1.0）。
* **isMuted**: `boolean`，是否处于静音状态。
* **playbackRate**: `number`，当前播放速率。
* **metadata**: `RustrumMetadata | null`，当前加载视频的元数据信息。
* **segmentStatuses**: `('pending' | 'loading' | 'decrypted' | 'active')[]`，流式分片状态列表。
* **load**: `(rstrmSource: ArrayBuffer | string, rstrSource: File | string, password: string) => Promise<RustrumMetadata>`，加载并初始化视频源的方法。
* **play**: `() => Promise<void>`，播放。
* **pause**: `() => void`，暂停。
* **togglePlay**: `() => void`，播放/暂停切换。
* **setVolume**: `(vol: number) => void`，设置音量。
* **setMuted**: `(muted: boolean) => void`，设置静音状态。
* **setCurrentTime**: `(time: number) => void`，跳转进度。
* **setPlaybackRate**: `(rate: number) => void`，调整倍速。

---

### 数据结构定义

#### RustrumPlayerOptions

```typescript
export interface RustrumPlayerOptions {
  // 限制缓存量：指定最大预加载分片数。不设置或小于等于 0 则默认不限制
  maxPreloadSegments?: number;

  // 日志输出回调函数
  onLog?: (message: string, type: 'info' | 'success' | 'error' | 'wasm') => void;

  // 分片状态变更回调函数
  onSegmentStatusChange?: (index: number, status: 'pending' | 'loading' | 'decrypted' | 'active') => void;

  // 媒体播放状态回调
  onPlay?: () => void;
  onPause?: () => void;
  onTimeUpdate?: (currentTime: number, duration: number) => void;
  onVolumeChange?: (volume: number, muted: boolean) => void;
  onPlaybackRateChange?: (rate: number) => void;
  onEnded?: () => void;
  onError?: (error: any) => void;
}
```

#### RustrumMetadata

```typescript
export interface RustrumMetadata {
  version: number;        // 视频流加密协议版本号
  cipherId: number;       // 加密算法 ID (1: ChaCha20-Poly1305, 2: AES-256-GCM, 3: AES-128-GCM)
  cipherName: string;     // 算法名称字符串形式
  isSplit: boolean;       // 是否为物理分段多文件模式
  duration: number;       // 视频总时长 (秒)
  saltHex: string;        // 派生密钥所使用的 16 字节随机盐的十六进制字符串
  indexCount: number;     // 视频总分块数 (包含 Init Segment)
  mimeType: string;       // 视频的 MIME 格式与 Codecs 编码标识
}
```

---

## 完整应用示例 (React + TypeScript)

以下示例展示了如何在 React 组件中集成 `RustrumPlayer` 进行视频解密播放，并绑定日志记录与分片状态监测：

```tsx
import React, { useState, useEffect, useRef } from 'react';
import { RustrumPlayer, RustrumMetadata } from 'rustrum-sdk';

interface LogItem {
  time: string;
  message: string;
  type: 'info' | 'success' | 'error' | 'wasm';
}

export default function SecurePlayerApp() {
  const [password, setPassword] = useState('your-password');
  const [metadata, setMetadata] = useState<RustrumMetadata | null>(null);
  const [logs, setLogs] = useState<LogItem[]>([]);
  const [segmentStatuses, setSegmentStatuses] = useState<string[]>([]);
  
  const videoRef = useRef<HTMLVideoElement>(null);
  const playerRef = useRef<RustrumPlayer | null>(null);

  const addLog = (message: string, type: LogItem['type'] = 'info') => {
    const time = new Date().toLocaleTimeString();
    setLogs((prev) => [{ time, message, type }, ...prev]);
  };

  useEffect(() => {
    if (!videoRef.current) return;

    // 1. 实例化播放器
    const player = new RustrumPlayer(videoRef.current, {
      onLog: (msg, type) => {
        addLog(msg, type);
      },
      onSegmentStatusChange: (index, status) => {
        // 动态更新分片状态列表
        setSegmentStatuses((prev) => {
          const next = [...prev];
          next[index] = status;
          return next;
        });
      }
    });

    playerRef.current = player;

    return () => {
      // 2. 销毁播放器，释放 WASM 内存与事件绑定
      player.destroy();
    };
  }, []);

  const handleStartPlay = async () => {
    if (!playerRef.current) return;

    const rstrmUrl = '/media/video.rstrm'; // 元数据索引 URL
    const rstrUrl = '/media/video.rstr';   // 密文视频媒体文件 URL

    try {
      addLog('正在加载并初始化加密媒体流...', 'info');
      // 3. 加载解密播放器
      const meta = await playerRef.current.load(rstrmUrl, rstrUrl, password);
      setMetadata(meta);
      setSegmentStatuses(new Array(meta.indexCount).fill('pending'));
      addLog('解密播放就绪，准备播放！', 'success');
    } catch (err: any) {
      addLog(`初始化播放器失败: ${err.message || err}`, 'error');
    }
  };

  return (
    <div style={{ padding: '20px', maxWidth: '800px', margin: '0 auto' }}>
      <h2>Rustrum 安全视频解密播放器</h2>
      
      <div style={{ marginBottom: '15px' }}>
        <label>解密密码：</label>
        <input 
          type="password" 
          value={password} 
          onChange={(e) => setPassword(e.target.value)} 
          style={{ padding: '5px', width: '200px' }}
        />
        <button onClick={handleStartPlay} style={{ marginLeft: '10px', padding: '6px 12px' }}>
          加载视频
        </button>
      </div>

      <div style={{ background: '#000', borderRadius: '8px', overflow: 'hidden' }}>
        <video 
          ref={videoRef} 
          controls 
          style={{ width: '100%', display: 'block' }}
        />
      </div>

      {metadata && (
        <div style={{ marginTop: '15px', background: '#f5f5f5', padding: '10px', borderRadius: '5px' }}>
          <h4>视频信息：</h4>
          <p>加密算法: {metadata.cipherName} | 总时长: {metadata.duration.toFixed(2)}s | 分片总数: {metadata.indexCount}</p>
        </div>
      )}

      <div style={{ marginTop: '20px' }}>
        <h4>解密与播放日志：</h4>
        <div style={{ 
          height: '150px', 
          overflowY: 'auto', 
          background: '#1e1e1e', 
          color: '#ccc', 
          padding: '10px', 
          fontFamily: 'monospace',
          fontSize: '12px'
        }}>
          {logs.map((log, idx) => (
            <div key={idx} style={{ color: log.type === 'error' ? '#ff6b6b' : log.type === 'success' ? '#51cf66' : '#ccc' }}>
              [{log.time}] {log.message}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
```

## 性能优化建议

1. **HTTP 服务器配置**：为了实现平滑的即时寻道，托管密文视频（.rstr 文件）的静态服务器必须支持 **HTTP Range Requests** (响应码 206 Partial Content)。否则播放器无法按需提取视频分片，只能降级为加载全部文件。
2. **预加载下一分片**：播放器内部自带智能预加载机制，会在当前分片播放完毕或缓冲区空闲时，提前加载并解密下一个 `pending` 状态的分片。通常无需开发者在外部手动干预。
3. **MIME 匹配**：加密时会自动探测并保存视频的 MIME 类型。解密播放时，SDK 会优先从解密后的元数据中自动读取并使用该 `mimeType`，无需手动在配置项中传入。
