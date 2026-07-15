import React, { useState, useEffect, useRef } from 'react';
import {
  Play,
  Key,
  FileVideo,
  FolderOpen,
  Terminal,
  RefreshCw,
  Cpu,
  Layers
} from 'lucide-react';
import { RustrumPlayer, type RustrumMetadata } from 'rustrum-sdk';

interface ConsoleLog {
  time: string;
  message: string;
  type: 'info' | 'success' | 'error' | 'wasm';
}

interface SegmentState {
  index: number;
  offset: bigint;
  size: bigint;
  status: 'pending' | 'loading' | 'decrypted' | 'active';
}

export default function App() {
  // 状态
  const [password, setPassword] = useState('testpassword');
  const [isPlaying, setIsPlaying] = useState(false);
  const [logs, setLogs] = useState<ConsoleLog[]>([]);
  const [segments, setSegments] = useState<SegmentState[]>([]);
  const [headerInfo, setHeaderInfo] = useState<RustrumMetadata | null>(null);
  const [wasmReady, setWasmReady] = useState(false);

  type RstrSource =
    | { type: 'url'; url: string }
    | { type: 'file'; file: File };

  // 文件加载状态
  const [rstrSource, setRstrSource] = useState<RstrSource | null>(null);
  const [rstrmData, setRstrmData] = useState<ArrayBuffer | null>(null);
  const [isLoadingFiles, setIsLoadingFiles] = useState(false);

  // 引用 (Ref)
  const videoRef = useRef<HTMLVideoElement>(null);
  const playerRef = useRef<RustrumPlayer | null>(null);

  // 添加日志记录
  const addLog = (message: string, type: ConsoleLog['type'] = 'info') => {
    const time = new Date().toLocaleTimeString();
    setLogs((prev) => [{ time, message, type }, ...prev].slice(0, 100));
  };

  // 初始化 RustrumPlayer
  useEffect(() => {
    if (!videoRef.current) return;

    const player = new RustrumPlayer(videoRef.current, {
      onLog: (msg, type) => {
        addLog(msg, type);
        if (msg.includes('WebAssembly 核心模块初始化成功')) {
          setWasmReady(true);
        }
      },
      onSegmentStatusChange: (index, status) => {
        setSegments((prev) =>
          prev.map((s) => (s.index === index ? { ...s, status } : s))
        );
      }
    });

    playerRef.current = player;

    return () => {
      player.destroy();
    };
  }, []);

  // 辅助函数：根据密码和加载的数据启动或重新加载播放器
  const handleLoadAndPlay = async (rstrm: ArrayBuffer | string, rstr: File | string, pass: string) => {
    if (!playerRef.current) return;
    try {
      const meta = await playerRef.current.load(rstrm, rstr, pass);
      setHeaderInfo(meta);
      
      // 获取分片结构并显示在 UI 监控上
      const info = playerRef.current.getSegmentsInfo();
      const segs: SegmentState[] = info.map((item) => ({
        index: item.index,
        offset: item.offset,
        size: item.size,
        status: 'pending'
      }));
      setSegments(segs);
    } catch (err: any) {
      if (err.message !== "Aborted due to concurrent load") {
        addLog(`播放器加载失败: ${err.message || err}`, 'error');
      }
    }
  };

  // 加载默认示例文件
  const loadDefaultFiles = async () => {
    setIsLoadingFiles(true);
    const baseUrl = import.meta.env.BASE_URL || '/';
    const cleanBaseUrl = baseUrl.endsWith('/') ? baseUrl : baseUrl + '/';
    const rstrmUrl = `${cleanBaseUrl}fmp4.rstrm`;
    const rstrUrl = `${cleanBaseUrl}fmp4.rstr`;

    addLog(`正在加载默认视频元数据 (${rstrmUrl})...`, 'info');
    try {
      const rstrmRes = await fetch(rstrmUrl);
      if (!rstrmRes.ok) {
        throw new Error('未找到预置的加密视频描述文件，请先使用 CLI 工具进行加密生成。');
      }
      const rstrmBuf = await rstrmRes.arrayBuffer();

      setRstrSource({ type: 'url', url: rstrUrl });
      setRstrmData(rstrmBuf);
      addLog(`加载默认元数据文件成功 (rstrm: ${rstrmBuf.byteLength} 字节)，视频源将按需发起 Range 请求。`, 'success');

      await handleLoadAndPlay(rstrmBuf, rstrUrl, password);
    } catch (err: any) {
      addLog(`加载默认文件失败: ${err.message || err}`, 'error');
    } finally {
      setIsLoadingFiles(false);
    }
  };

  // 全局错误处理
  useEffect(() => {
    const handleGlobalError = (event: ErrorEvent) => {
      addLog(`全局 JS 错误: ${event.message} 在 ${event.filename}:${event.lineno}`, 'error');
    };
    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      addLog(`全局 Promise 错误: ${event.reason}`, 'error');
    };
    window.addEventListener('error', handleGlobalError);
    window.addEventListener('unhandledrejection', handleUnhandledRejection);
    return () => {
      window.removeEventListener('error', handleGlobalError);
      window.removeEventListener('unhandledrejection', handleUnhandledRejection);
    };
  }, []);

  // 组件挂载后，如果 player 准备好了，则加载默认示例
  useEffect(() => {
    if (playerRef.current) {
      loadDefaultFiles();
    }
  }, []);

  // 密码改变时，重新加载播放器
  useEffect(() => {
    if (!rstrmData || !rstrSource) return;
    const sourcePath = rstrSource.type === 'url' ? rstrSource.url : rstrSource.file;
    handleLoadAndPlay(rstrmData, sourcePath, password);
  }, [password]);

  // 自定义文件上传处理程序
  const handleRstrUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setRstrSource({ type: 'file', file: file });
    addLog(`自定义密文视频已注册: ${file.name} (${(file.size / 1024 / 1024).toFixed(2)} MB)，将进行本地切片读取。`, 'success');
    
    if (rstrmData) {
      handleLoadAndPlay(rstrmData, file, password);
    }
  };

  const handleRstrmUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const buf = reader.result as ArrayBuffer;
      setRstrmData(buf);
      addLog(`自定义元数据描述文件已加载: ${file.name} (${file.size} 字节)`, 'success');
      
      if (rstrSource) {
        const sourcePath = rstrSource.type === 'url' ? rstrSource.url : rstrSource.file;
        handleLoadAndPlay(buf, sourcePath, password);
      }
    };
    reader.readAsArrayBuffer(file);
  };

  const startPlayer = () => {
    if (!playerRef.current || !rstrSource || !rstrmData) {
      addLog('无法开始播放，请先加载视频和元数据描述文件！', 'error');
      return;
    }
    const sourcePath = rstrSource.type === 'url' ? rstrSource.url : rstrSource.file;
    handleLoadAndPlay(rstrmData, sourcePath, password);
  };

  return (
    <div className="min-h-screen flex flex-col p-6 max-w-7xl mx-auto space-y-6">
      {/* 头部 */}
      <header className="flex flex-col md:flex-row md:items-center justify-between pb-6 border-b border-border-dark">
        <div>
          <h1 className="text-3xl font-extrabold tracking-tight bg-gradient-to-r from-brand-primary to-brand-secondary bg-clip-text text-transparent">
            Rustrum Secure Player
          </h1>
          <p className="text-sm text-gray-400 mt-1">
            基于 WebAssembly 与零拷贝共享内存的端到端安全流媒体解密播放系统
          </p>
        </div>
        <div className="flex items-center space-x-3 mt-4 md:mt-0">
          <div className={`px-3 py-1 text-xs rounded-full flex items-center gap-1.5 border ${
            wasmReady 
              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30' 
              : 'bg-amber-500/10 text-amber-400 border-amber-500/30'
          }`}>
            <Cpu className="w-3.5 h-3.5" />
            WASM 核心: {wasmReady ? '已就绪' : '加载中'}
          </div>
        </div>
      </header>

      {/* Main Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
        {/* 左侧栏：播放器与密钥管理 */}
        <div className="lg:col-span-7 space-y-6">
          {/* 播放器面板 */}
          <div className="bg-panel-dark backdrop-blur-md border border-border-dark rounded-2xl p-5 overflow-hidden">
            <div className="relative aspect-video rounded-xl bg-black border border-border-dark overflow-hidden flex items-center justify-center group">
              <video
                ref={videoRef}
                onPlay={() => setIsPlaying(true)}
                onPause={() => setIsPlaying(false)}
                onCanPlay={() => {
                  if (videoRef.current && !videoRef.current.paused) {
                    videoRef.current.play().catch(() => {});
                  }
                }}
                controls
                className="w-full h-full object-contain"
              />
              {!isPlaying && !rstrSource && (
                <div className="absolute inset-0 bg-black/60 flex flex-col items-center justify-center space-y-4">
                  <FileVideo className="w-16 h-16 text-brand-primary animate-pulse" />
                  <p className="text-gray-300 font-medium">请先加载演示视频</p>
                </div>
              )}
            </div>

            {/* 快捷播放控制 */}
            <div className="mt-4 flex items-center justify-between">
              <div className="flex items-center gap-2">
                <button
                  disabled={!rstrSource}
                  onClick={startPlayer}
                  className={`px-4 py-2 rounded-lg text-white font-medium flex items-center gap-2 transition-all shadow-md hover-scale ${
                    rstrSource
                      ? 'bg-gradient-to-r from-brand-primary to-brand-secondary hover:from-brand-primary/90 hover:to-brand-secondary/90 cursor-pointer'
                      : 'bg-gray-800 text-gray-500 border border-gray-700/50 cursor-not-allowed'
                  }`}
                >
                  <Play className="w-4 h-4 fill-white" /> 一键启动解密播放
                </button>
              </div>
              <p className="text-xs text-gray-400">
                数据将经由 WASM 共享内存进行就地 (In-place) 解密并输入 MSE
              </p>
            </div>
          </div>

          {/* 密钥管理 */}
          <div className="bg-panel-dark backdrop-blur-md border border-border-dark rounded-2xl p-5 space-y-4">
            <div className="flex items-center gap-2 font-semibold text-gray-200">
              <Key className="w-4 h-4 text-brand-primary" />
              密码配置
            </div>
            <div>
              <label className="block text-xs text-gray-400 mb-1">解密密码</label>
              <input
                type="text"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="请输入解密密码"
                className="w-full bg-black/50 border border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-brand-primary transition-colors text-white"
              />
            </div>
          </div>
        </div>

        {/* 右侧栏：文件与元数据 */}
        <div className="lg:col-span-5 space-y-6">
          {/* 文件加载器 */}
          <div className="bg-panel-dark backdrop-blur-md border border-border-dark rounded-2xl p-5 space-y-4">
            <div className="flex items-center gap-2 font-semibold text-gray-200">
              <FolderOpen className="w-4 h-4 text-brand-primary" />
              媒体源加载
            </div>
            
            <button
              disabled={isLoadingFiles}
              onClick={loadDefaultFiles}
              className="w-full py-2.5 bg-brand-primary/10 hover:bg-brand-primary/20 text-brand-primary border border-brand-primary/20 rounded-xl font-medium flex items-center justify-center gap-2 transition-all hover-scale"
            >
              {isLoadingFiles ? (
                <RefreshCw className="w-4 h-4 animate-spin" />
              ) : (
                <FileVideo className="w-4 h-4" />
              )}
              加载本地默认演示视频 (data/fmp4)
            </button>

            <div className="relative flex py-2 items-center">
              <div className="flex-grow border-t border-border-dark"></div>
              <span className="flex-shrink mx-4 text-gray-500 text-xs uppercase">或者上传自定义流</span>
              <div className="flex-grow border-t border-border-dark"></div>
            </div>

            <div className="space-y-3">
              <div>
                <label className="block text-xs text-gray-400 mb-1">加密媒体流 (.rstr)</label>
                <input
                  type="file"
                  accept=".rstr"
                  onChange={handleRstrUpload}
                  className="w-full text-xs text-gray-400 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:text-xs file:font-semibold file:bg-brand-primary/10 file:text-brand-primary hover:file:bg-brand-primary/20"
                />
              </div>
              <div>
                <label className="block text-xs text-gray-400 mb-1">索引描述文件 (.rstrm)</label>
                <input
                  type="file"
                  accept=".rstrm"
                  onChange={handleRstrmUpload}
                  className="w-full text-xs text-gray-400 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:text-xs file:font-semibold file:bg-brand-primary/10 file:text-brand-primary hover:file:bg-brand-primary/20"
                />
              </div>
            </div>
          </div>

          {/* 元数据 */}
          {headerInfo && (
            <div className="bg-panel-dark backdrop-blur-md border border-border-dark rounded-2xl p-5 space-y-3">
              <div className="flex items-center gap-2 font-semibold text-gray-200 border-b border-border-dark pb-2">
                <Layers className="w-4 h-4 text-brand-primary" />
                流元数据摘要 (.rstrm)
              </div>
              <div className="grid grid-cols-2 gap-y-2.5 text-sm">
                <div className="text-gray-400">视频流版本</div>
                <div className="font-semibold text-right">{headerInfo.version}</div>

                <div className="text-gray-400">加密算法</div>
                <div className="font-semibold text-brand-secondary text-right">{headerInfo.cipherName}</div>

                <div className="text-gray-400">物理分片文件</div>
                <div className="font-semibold text-right">{headerInfo.isSplit ? '是' : '否 (单合并文件)'}</div>

                <div className="text-gray-400">视频总时长</div>
                <div className="font-semibold text-brand-primary text-right">{headerInfo.duration.toFixed(2)}s</div>

                <div className="text-gray-400">MIME 类型</div>
                <div className="font-semibold text-right truncate pl-4 text-xs" title={headerInfo.mimeType}>
                  {headerInfo.mimeType}
                </div>

                <div className="text-gray-400">分片总数</div>
                <div className="font-semibold text-right">{headerInfo.indexCount}</div>

                <div className="text-gray-400">Argon2 密钥盐值</div>
                <div className="font-mono text-xs text-gray-400 text-right truncate pl-4" title={headerInfo.saltHex}>
                  {headerInfo.saltHex}
                </div>
              </div>
            </div>
          )}

          {/* 分片状态可视化监测 */}
          {segments.length > 0 && (
            <div className="bg-panel-dark backdrop-blur-md border border-border-dark rounded-2xl p-5 space-y-3">
              <div className="font-semibold text-gray-200 border-b border-border-dark pb-2">
                媒体分片网格监控
              </div>
              <div className="grid grid-cols-6 sm:grid-cols-8 gap-2">
                {segments.map((seg) => {
                  let colorClass = 'bg-gray-800 text-gray-400 border-gray-700/50';
                  if (seg.status === 'loading') colorClass = 'bg-brand-primary/20 text-brand-primary border-brand-primary animate-pulse';
                  if (seg.status === 'decrypted') colorClass = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
                  if (seg.status === 'active') colorClass = 'bg-brand-secondary/20 text-brand-secondary border-brand-secondary scale-105 shadow-sm';

                  return (
                    <div
                      key={seg.index}
                      className={`text-center py-2 text-xs font-mono font-bold rounded-lg border transition-all ${colorClass}`}
                      title={`偏移: ${seg.offset}, 大小: ${seg.size}`}
                    >
                      {seg.index}
                    </div>
                  );
                })}
              </div>
              <div className="flex gap-4 justify-center text-xs text-gray-400 pt-2">
                <span className="flex items-center gap-1"><span className="w-2.5 h-2.5 bg-gray-800 border border-gray-700 rounded-sm"></span>未载入</span>
                <span className="flex items-center gap-1"><span className="w-2.5 h-2.5 bg-emerald-500/20 border border-emerald-500/30 rounded-sm"></span>已解密</span>
                <span className="flex items-center gap-1"><span className="w-2.5 h-2.5 bg-brand-secondary/20 border border-brand-secondary rounded-sm"></span>正在播放</span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* 日志控制台 */}
      <div className="bg-black/80 border border-border-dark rounded-2xl p-5 font-mono text-xs flex flex-col h-64 overflow-hidden">
        <div className="flex items-center justify-between pb-3 border-b border-border-dark text-gray-400">
          <span className="flex items-center gap-2 text-brand-primary">
            <Terminal className="w-4 h-4" /> 解密流实时运行控制台
          </span>
          <button
            onClick={() => setLogs([])}
            className="text-[10px] uppercase tracking-wider px-2 py-0.5 hover:bg-gray-800 rounded transition-colors"
          >
            清空日志
          </button>
        </div>
        <div className="flex-1 overflow-y-auto space-y-1.5 py-3 pr-2 scrollbar-thin">
          {logs.map((log, i) => {
            let color = 'text-gray-400';
            if (log.type === 'success') color = 'text-emerald-400';
            if (log.type === 'error') color = 'text-red-400';
            if (log.type === 'wasm') color = 'text-purple-400';

            return (
              <div key={i} className="leading-5">
                <span className="text-gray-600 mr-2">[{log.time}]</span>
                <span className={color}>{log.message}</span>
              </div>
            );
          })}
          {logs.length === 0 && (
            <div className="text-gray-600 italic text-center py-10">暂无日志运行数据...</div>
          )}
        </div>
      </div>
    </div>
  );
}
