import initWasm, {
  WasmDecoder,
  type InitOutput
} from './pkg/rustrum_wasm.js';

export interface RustrumPlayerOptions {
  maxPreloadSegments?: number;
  onLog?: (message: string, type: 'info' | 'success' | 'error' | 'wasm') => void;
  onSegmentStatusChange?: (index: number, status: 'pending' | 'loading' | 'decrypted' | 'active') => void;
  onPlay?: () => void;
  onPause?: () => void;
  onTimeUpdate?: (currentTime: number, duration: number) => void;
  onVolumeChange?: (volume: number, muted: boolean) => void;
  onPlaybackRateChange?: (rate: number) => void;
  onEnded?: () => void;
  onError?: (error: any) => void;
}

export interface RustrumMetadata {
  version: number;
  cipherId: number;
  cipherName: string;
  isSplit: boolean;
  duration: number;
  saltHex: string;
  indexCount: number;
  mimeType: string;
}

type RstrSourceInternal =
  | { type: 'url'; url: string }
  | { type: 'file'; file: File };

let globalWasmInstance: InitOutput | null = null;

async function ensureWasmInitialized(): Promise<InitOutput> {
  if (!globalWasmInstance) {
    globalWasmInstance = await initWasm();
  }
  return globalWasmInstance;
}

export class RustrumPlayer {
  private videoElement: HTMLVideoElement;
  private options: RustrumPlayerOptions;
  private mediaSource: MediaSource | null = null;
  private sourceBuffer: SourceBuffer | null = null;
  private decoder: WasmDecoder | null = null;
  private wasmInstance: InitOutput | null = null;
  
  private queue: Uint8Array[] = [];
  private currentSegmentIndex = -1;
  private isRemoving = false;
  private isSeeking = false;
  private wasPlaying = false;
  private targetSegIndex = -1;
  
  private loadId = 0;
  private rstrSource: RstrSourceInternal | null = null;
  private metadata: RustrumMetadata | null = null;
  private segmentStatuses: ('pending' | 'loading' | 'decrypted' | 'active')[] = [];
  private activeSegmentIndex = -1;

  constructor(videoElement: HTMLVideoElement, options: RustrumPlayerOptions = {}) {
    this.videoElement = videoElement;
    this.options = options;
  }

  private log(message: string, type: 'info' | 'success' | 'error' | 'wasm' = 'info') {
    this.options.onLog?.(message, type);
  }

  private setSegmentStatus(index: number, status: 'pending' | 'loading' | 'decrypted' | 'active') {
    if (index >= 0 && index < this.segmentStatuses.length) {
      this.segmentStatuses[index] = status;
      this.options.onSegmentStatusChange?.(index, status);
    }
  }

  public getMetadata(): RustrumMetadata | null {
    return this.metadata;
  }

  public getSegmentStatuses(): ('pending' | 'loading' | 'decrypted' | 'active')[] {
    return [...this.segmentStatuses];
  }

  public getSegmentsInfo(): { index: number; offset: bigint; size: bigint }[] {
    if (!this.decoder) return [];
    const info = [];
    for (let i = 1; i < this.decoder.index_count; i++) {
      const offset = this.decoder.get_entry_offset(i);
      const size = this.decoder.get_entry_size(i);
      if (offset !== undefined && size !== undefined) {
        info.push({ index: i, offset, size });
      }
    }
    return info;
  }

  private getCipherName(id: number): string {
    switch (id) {
      case 1: return 'ChaCha20-Poly1305';
      case 2: return 'AES-256-GCM';
      case 3: return 'AES-128-GCM';
      default: return `未知算法 (ID: ${id})`;
    }
  }

  public async load(
    rstrmSource: ArrayBuffer | string,
    rstrSource: File | string,
    password: string
  ): Promise<RustrumMetadata> {
    const currentLoadId = ++this.loadId;
    this.destroy();
    this.log('正在初始化 WebAssembly 核心模块...', 'wasm');
    this.wasmInstance = await ensureWasmInitialized();
    if (currentLoadId !== this.loadId) {
      throw new Error("Aborted due to concurrent load");
    }
    this.log('WebAssembly 核心模块初始化成功。', 'success');

    // 处理 rstrm 元数据源
    let rstrmBuffer: ArrayBuffer;
    if (typeof rstrmSource === 'string') {
      this.log(`正在获取视频元数据描述文件: ${rstrmSource}...`, 'info');
      const res = await fetch(rstrmSource);
      if (!res.ok) {
        throw new Error(`获取元数据失败: ${res.statusText}`);
      }
      if (currentLoadId !== this.loadId) {
        throw new Error("Aborted due to concurrent load");
      }
      rstrmBuffer = await res.arrayBuffer();
      if (currentLoadId !== this.loadId) {
        throw new Error("Aborted due to concurrent load");
      }
    } else {
      rstrmBuffer = rstrmSource;
    }

    // 处理 rstr 媒体源
    if (typeof rstrSource === 'string') {
      this.rstrSource = { type: 'url', url: rstrSource };
    } else {
      this.rstrSource = { type: 'file', file: rstrSource };
    }

    // 初始化解码器
    if (this.decoder) {
      this.decoder.free();
      this.decoder = null;
    }

    this.decoder = new WasmDecoder(new Uint8Array(rstrmBuffer), password);
    const saltHex = Array.from(this.decoder.key_salt)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');

    this.metadata = {
      version: this.decoder.version,
      cipherId: this.decoder.cipher_id,
      cipherName: this.getCipherName(this.decoder.cipher_id),
      isSplit: this.decoder.is_split,
      duration: this.decoder.duration,
      saltHex,
      indexCount: this.decoder.index_count,
      mimeType: this.decoder.mime_type
    };

    this.log(`成功解析并构建 WasmDecoder [版本: ${this.metadata.version}, 算法: ${this.metadata.cipherName}, 分片数量: ${this.metadata.indexCount}]`, 'wasm');

    // 初始化分片状态列表
    this.segmentStatuses = new Array(this.metadata.indexCount).fill('pending');
    // 跳过第 0 块 Init Segment
    this.setSegmentStatus(0, 'decrypted');

    // 绑定视频事件
    this.bindVideoEvents();

    // 启动 MSE 播放器
    this.startPlayer();

    return this.metadata;
  }

  private processQueue() {
    const sb = this.sourceBuffer;
    if (!sb || sb.updating || this.queue.length === 0) return;

    const nextChunk = this.queue.shift()!;
    try {
      sb.appendBuffer(nextChunk as BufferSource);
    } catch (err: any) {
      this.log(`MSE AppendBuffer 错误: ${err.message || err}`, 'error');
    }
  }

  private async loadAndDecryptSegment(index: number) {
    if (!this.wasmInstance || !this.rstrSource || !this.decoder) return;

    const activeDecoder = this.decoder;
    const activeMediaSource = this.mediaSource;

    const offset = this.decoder.get_entry_offset(index);
    const size = this.decoder.get_entry_size(index);
    if (offset === undefined || size === undefined) return;

    this.setSegmentStatus(index, 'loading');
    this.log(`正在获取并解密第 ${index} 个媒体分片 (大小: ${size} 字节)...`, 'info');

    const start = Number(offset);
    const end = start + Number(size);
    let encryptedData: Uint8Array;

    try {
      if (this.rstrSource.type === 'url') {
        const res = await fetch(this.rstrSource.url, {
          headers: {
            'Range': `bytes=${start}-${end - 1}`
          }
        });
        if (!res.ok && res.status !== 206) {
          throw new Error(`HTTP Range 请求失败，状态码: ${res.status}`);
        }
        if (activeDecoder !== this.decoder || activeMediaSource !== this.mediaSource) return;
        encryptedData = new Uint8Array(await res.arrayBuffer());
      } else {
        const blob = this.rstrSource.file.slice(start, end);
        if (activeDecoder !== this.decoder || activeMediaSource !== this.mediaSource) return;
        encryptedData = new Uint8Array(await blob.arrayBuffer());
      }

      if (activeDecoder !== this.decoder || activeMediaSource !== this.mediaSource) return;

      const startTime = performance.now();

      // WASM 零拷贝共享内存就地解密
      const ptr = this.decoder.get_buffer_ptr();
      const wasmMem = new Uint8Array(this.wasmInstance.memory.buffer);
      wasmMem.set(encryptedData, ptr);

      const plaintextLen = this.decoder.decrypt_segment_in_place(index, encryptedData.length);
      const duration = (performance.now() - startTime).toFixed(2);

      const decrypted = new Uint8Array(this.wasmInstance.memory.buffer, ptr, plaintextLen).slice();

      this.log(`分片 ${index} 在 WASM 中零拷贝就地解密完成，用时 ${duration}ms (解密后明文大小: ${plaintextLen} 字节)`, 'wasm');

      this.queue.push(decrypted);
      if (this.sourceBuffer && !this.sourceBuffer.updating) {
        this.processQueue();
      }

      this.setSegmentStatus(index, 'decrypted');
    } catch (err: any) {
      if (activeDecoder !== this.decoder || activeMediaSource !== this.mediaSource) return;
      this.log(`解密分片 ${index} 失败: ${err.message || err}`, 'error');
      this.setSegmentStatus(index, 'pending');
      this.options.onError?.(err);
    }
  }

  private startPlayer() {
    if (!this.rstrSource || !this.decoder) return;

    this.log('正在初始化 MediaSource 扩展播放流水线...', 'info');

    if (this.mediaSource) {
      try {
        this.videoElement.src = '';
      } catch {}
    }

    const ms = new MediaSource();
    this.mediaSource = ms;
    this.videoElement.src = URL.createObjectURL(ms);

    ms.addEventListener('sourceopen', () => {
      if (ms !== this.mediaSource) return;
      this.log('MediaSource 已开启，正在创建 SourceBuffer...', 'info');

      if (this.decoder) {
        ms.duration = this.decoder.duration;
      }

      const mime = this.metadata?.mimeType || 'video/mp4; codecs="avc1.64001e, mp4a.40.2"';
      try {
        const sb = ms.addSourceBuffer(mime);
        this.sourceBuffer = sb;

        sb.addEventListener('updateend', () => {
          if (sb !== this.sourceBuffer) return;

          if (this.isRemoving) {
            this.isRemoving = false;
            this.loadAndDecryptSegment(0);
            return;
          }

          if (this.isSeeking) {
            this.isSeeking = false;
            const targetIdx = this.targetSegIndex;
            this.currentSegmentIndex = targetIdx;

            if (targetIdx > 0) {
              this.loadAndDecryptSegment(targetIdx);
            }

            if (this.wasPlaying) {
              this.videoElement.play().catch(() => {});
            }
            return;
          }

          this.processQueue();

          const nextIdx = this.currentSegmentIndex + 1;
          const totalSegs = this.decoder ? this.decoder.index_count : 0;

          if (totalSegs > 0 && nextIdx >= totalSegs && this.queue.length === 0) {
            if (ms.readyState === 'open') {
              try {
                ms.endOfStream();
                this.log('所有媒体分片加载完毕，已将 MediaSource 标记为结束 (endOfStream)。', 'success');
              } catch (e: any) {
                this.log(`标记媒体流结束失败: ${e.message || e}`, 'error');
              }
            }
          } else {
            // 在上一个分片加载后，且队列空闲时，自动预加载下一个 pending 分片
            if (nextIdx >= 1 && nextIdx < totalSegs) {
              if (this.segmentStatuses[nextIdx] === 'pending') {
                const maxPreload = this.options.maxPreloadSegments;
                if (maxPreload !== undefined && maxPreload > 0) {
                  const activeIdx = this.activeSegmentIndex >= 0 ? this.activeSegmentIndex : 1;
                  if (nextIdx - activeIdx > maxPreload) {
                    return;
                  }
                }
                this.currentSegmentIndex = nextIdx;
                this.loadAndDecryptSegment(nextIdx);
              }
            }
          }
        });

        // 加载初始化分片 (索引 0)
        this.loadAndDecryptSegment(0);
        this.currentSegmentIndex = 0;
      } catch (err: any) {
        this.log(`创建 SourceBuffer 失败 (MIME: ${mime}): ${err.message || err}`, 'error');
        this.options.onError?.(err);
      }
    });
  }

  private handleTimeUpdate = () => {
    if (!this.decoder || this.segmentStatuses.length === 0) return;

    const duration = this.videoElement.duration;
    if (!duration || duration === Infinity) return;

    let activeSegIdx = 1;
    try {
      activeSegIdx = this.decoder.locate_chunk_by_time(this.videoElement.currentTime, duration);
    } catch {
      activeSegIdx = Math.min(
        Math.floor((this.videoElement.currentTime / duration) * (this.segmentStatuses.length - 2)) + 1,
        this.segmentStatuses.length - 1
      );
    }

    // 智能预加载下一分片
    const nextIdx = activeSegIdx + 1;
    if (nextIdx < this.segmentStatuses.length && this.segmentStatuses[nextIdx] === 'pending') {
      const maxPreload = this.options.maxPreloadSegments;
      if (maxPreload === undefined || maxPreload <= 0 || (nextIdx - activeSegIdx) <= maxPreload) {
        this.currentSegmentIndex = nextIdx;
        this.loadAndDecryptSegment(nextIdx);
      }
    }

    // 高亮当前分片
    if (activeSegIdx !== this.activeSegmentIndex) {
      if (this.activeSegmentIndex >= 0) {
        this.setSegmentStatus(this.activeSegmentIndex, 'decrypted');
      }
      this.activeSegmentIndex = activeSegIdx;
      this.setSegmentStatus(activeSegIdx, 'active');
    }

    this.options.onTimeUpdate?.(this.videoElement.currentTime, duration);
  };

  private handleSeeking = () => {
    if (!this.decoder || this.segmentStatuses.length === 0) return;

    const duration = this.videoElement.duration;
    if (!duration) return;

    let targetSegIdx = 1;
    try {
      targetSegIdx = this.decoder.locate_chunk_by_time(this.videoElement.currentTime, duration);
    } catch {
      targetSegIdx = Math.min(
        Math.floor((this.videoElement.currentTime / duration) * (this.segmentStatuses.length - 2)) + 1,
        this.segmentStatuses.length - 1
      );
    }

    this.log(`用户拖动进度条，寻道至: ${this.videoElement.currentTime.toFixed(2)}s (定位至分片: ${targetSegIdx})`, 'info');

    this.wasPlaying = !this.videoElement.paused;
    this.targetSegIndex = targetSegIdx;
    this.isSeeking = true;

    const sb = this.sourceBuffer;
    if (sb) {
      try {
        this.queue = [];
        sb.abort();
        this.isRemoving = true;
        sb.remove(0, this.videoElement.duration);
      } catch (err) {
        this.isRemoving = false;
        this.isSeeking = false;
      }
    } else {
      this.loadAndDecryptSegment(0).then(() => {
        if (targetSegIdx > 0) {
          this.loadAndDecryptSegment(targetSegIdx);
        }
        this.currentSegmentIndex = targetSegIdx;
        if (this.wasPlaying) {
          this.videoElement.play().catch(() => {});
        }
      });
    }
  };

  private handlePlay = () => {
    this.options.onPlay?.();
  };

  private handlePause = () => {
    this.options.onPause?.();
  };

  private handleVolumeChange = () => {
    this.options.onVolumeChange?.(this.videoElement.volume, this.videoElement.muted);
  };

  private handleRateChange = () => {
    this.options.onPlaybackRateChange?.(this.videoElement.playbackRate);
  };

  private handleEnded = () => {
    this.options.onEnded?.();
  };

  private handleVideoError = (event: ErrorEvent) => {
    this.options.onError?.(this.videoElement.error || event);
  };

  private bindVideoEvents() {
    this.videoElement.addEventListener('timeupdate', this.handleTimeUpdate);
    this.videoElement.addEventListener('seeking', this.handleSeeking);
    this.videoElement.addEventListener('play', this.handlePlay);
    this.videoElement.addEventListener('pause', this.handlePause);
    this.videoElement.addEventListener('volumechange', this.handleVolumeChange);
    this.videoElement.addEventListener('ratechange', this.handleRateChange);
    this.videoElement.addEventListener('ended', this.handleEnded);
    this.videoElement.addEventListener('error', this.handleVideoError as EventListener);
  }

  private unbindVideoEvents() {
    this.videoElement.removeEventListener('timeupdate', this.handleTimeUpdate);
    this.videoElement.removeEventListener('seeking', this.handleSeeking);
    this.videoElement.removeEventListener('play', this.handlePlay);
    this.videoElement.removeEventListener('pause', this.handlePause);
    this.videoElement.removeEventListener('volumechange', this.handleVolumeChange);
    this.videoElement.removeEventListener('ratechange', this.handleRateChange);
    this.videoElement.removeEventListener('ended', this.handleEnded);
    this.videoElement.removeEventListener('error', this.handleVideoError as EventListener);
  }

  public play(): Promise<void> {
    return this.videoElement.play();
  }

  public pause(): void {
    this.videoElement.pause();
  }

  public togglePlay(): void {
    if (this.videoElement.paused) {
      this.play().catch(() => {});
    } else {
      this.pause();
    }
  }

  public getVolume(): number {
    return this.videoElement.volume;
  }

  public setVolume(volume: number): void {
    this.videoElement.volume = volume;
  }

  public isMuted(): boolean {
    return this.videoElement.muted;
  }

  public setMuted(muted: boolean): void {
    this.videoElement.muted = muted;
  }

  public getCurrentTime(): number {
    return this.videoElement.currentTime;
  }

  public setCurrentTime(time: number): void {
    this.videoElement.currentTime = time;
  }

  public getDuration(): number {
    return this.videoElement.duration;
  }

  public getPlaybackRate(): number {
    return this.videoElement.playbackRate;
  }

  public setPlaybackRate(rate: number): void {
    this.videoElement.playbackRate = rate;
  }

  public getBufferedTimeRanges(): { start: number; end: number }[] {
    const ranges = [];
    const buffered = this.videoElement.buffered;
    for (let i = 0; i < buffered.length; i++) {
      ranges.push({
        start: buffered.start(i),
        end: buffered.end(i)
      });
    }
    return ranges;
  }

  public destroy() {
    this.unbindVideoEvents();
    if (this.decoder) {
      this.decoder.free();
      this.decoder = null;
    }
    if (this.mediaSource) {
      if (this.mediaSource.readyState === 'open') {
        try {
          this.mediaSource.endOfStream();
        } catch {}
      }
      this.mediaSource = null;
    }
    this.sourceBuffer = null;
    this.queue = [];
    this.rstrSource = null;
    this.metadata = null;
    this.currentSegmentIndex = -1;
    this.isRemoving = false;
    this.isSeeking = false;
    this.wasPlaying = false;
    this.targetSegIndex = -1;
    this.activeSegmentIndex = -1;
    this.segmentStatuses = [];
  }
}
