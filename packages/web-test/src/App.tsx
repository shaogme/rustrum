import { useState, useRef, useEffect } from 'react';
import { useRustrumPlayer } from 'rustrum-sdk';
import './App.css';

interface LogItem {
  id: number;
  text: string;
  type: 'info' | 'success' | 'error' | 'wasm';
}

function App() {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const logEndRef = useRef<HTMLDivElement | null>(null);

  // 默认使用公共测试资源
  const [rstrmUrl, setRstrmUrl] = useState('/test.rstrm');
  const [rstrUrl, setRstrUrl] = useState('/test.rstr');
  const [password, setPassword] = useState('password123');
  const [logs, setLogs] = useState<LogItem[]>([]);
  const logIdRef = useRef(0);

  const addLog = (text: string, type: 'info' | 'success' | 'error' | 'wasm' = 'info') => {
    setLogs((prev) => [
      ...prev.slice(-99),
      { id: ++logIdRef.current, text, type }
    ]);
  };

  const {
    isPlaying,
    currentTime,
    duration,
    volume,
    isMuted,
    playbackRate,
    metadata,
    segmentStatuses,
    load,
    togglePlay,
    setVolume,
    setMuted,
    setCurrentTime,
    setPlaybackRate
  } = useRustrumPlayer(videoRef, {
    maxPreloadSegments: 3,
    onLog: (msg, type) => {
      addLog(msg, type);
    },
    onSegmentStatusChange: (idx, status) => {
      addLog(`分片 ${idx} 状态变化 -> ${status}`, 'info');
    },
    onError: (err) => {
      addLog(`播放器错误: ${err?.message || String(err)}`, 'error');
    }
  });

  const handleLoad = async () => {
    try {
      setLogs([]);
      addLog('正在载入 Rustrum 视频流...', 'info');
      await load(rstrmUrl, rstrUrl, password);
      addLog('Rustrum 视频载入就绪。', 'success');
    } catch (err: any) {
      addLog(`载入流失败: ${err.message || err}`, 'error');
    }
  };

  useEffect(() => {
    if (logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs]);

  const formatTime = (time: number) => {
    if (isNaN(time) || time === Infinity) return '00:00';
    const mins = Math.floor(time / 60).toString().padStart(2, '0');
    const secs = Math.floor(time % 60).toString().padStart(2, '0');
    return `${mins}:${secs}`;
  };

  return (
    <div style={{ padding: '24px', maxWidth: '1000px', margin: '0 auto', textAlign: 'left', fontFamily: 'system-ui, sans-serif' }}>
      <header style={{ marginBottom: '24px', borderBottom: '1px solid var(--border)', paddingBottom: '16px' }}>
        <h1 style={{ margin: '0 0 8px 0', fontSize: '32px', color: 'var(--text-h)' }}>Rustrum 安全播放控制台</h1>
        <p style={{ margin: 0, color: 'var(--text)', fontSize: '15px' }}>测试与展示 Rustrum SDK 实时流就地零拷贝解密与底层控制功能</p>
      </header>

      {/* 配置面板 */}
      <section style={{
        background: 'var(--social-bg)',
        border: '1px solid var(--border)',
        borderRadius: '12px',
        padding: '20px',
        marginBottom: '24px',
        display: 'grid',
        gridTemplateColumns: '1fr 1fr',
        gap: '16px'
      }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <label style={{ fontSize: '14px', fontWeight: 'bold', color: 'var(--text-h)' }}>元数据文件 (.rstrm)</label>
          <input
            type="text"
            value={rstrmUrl}
            onChange={(e) => setRstrmUrl(e.target.value)}
            style={{ padding: '8px 12px', borderRadius: '6px', border: '1px solid var(--border)', background: 'var(--bg)', color: 'var(--text-h)' }}
          />
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <label style={{ fontSize: '14px', fontWeight: 'bold', color: 'var(--text-h)' }}>解密密码</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            style={{ padding: '8px 12px', borderRadius: '6px', border: '1px solid var(--border)', background: 'var(--bg)', color: 'var(--text-h)' }}
          />
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', gridColumn: 'span 2' }}>
          <label style={{ fontSize: '14px', fontWeight: 'bold', color: 'var(--text-h)' }}>加密媒体文件 (.rstr)</label>
          <div style={{ display: 'flex', gap: '12px' }}>
            <input
              type="text"
              value={rstrUrl}
              onChange={(e) => setRstrUrl(e.target.value)}
              style={{ flex: 1, padding: '8px 12px', borderRadius: '6px', border: '1px solid var(--border)', background: 'var(--bg)', color: 'var(--text-h)' }}
            />
            <button
              onClick={handleLoad}
              style={{
                padding: '8px 24px',
                borderRadius: '6px',
                background: 'var(--accent)',
                color: '#fff',
                border: 'none',
                cursor: 'pointer',
                fontWeight: 'bold',
                transition: 'opacity 0.2s'
              }}
              onMouseEnter={(e) => e.currentTarget.style.opacity = '0.9'}
              onMouseLeave={(e) => e.currentTarget.style.opacity = '1'}
            >
              载入并解析
            </button>
          </div>
        </div>
      </section>

      {/* 媒体元数据展示 */}
      {metadata && (
        <section style={{
          background: 'var(--accent-bg)',
          border: '1px solid var(--accent-border)',
          borderRadius: '12px',
          padding: '16px',
          marginBottom: '24px',
          display: 'grid',
          gridTemplateColumns: 'repeat(4, 1fr)',
          gap: '12px',
          fontSize: '13px'
        }}>
          <div><strong>加密算法:</strong> {metadata.cipherName}</div>
          <div><strong>分片总数:</strong> {metadata.indexCount} 个</div>
          <div><strong>媒体格式:</strong> {metadata.mimeType}</div>
          <div><strong>流总长度:</strong> {metadata.duration.toFixed(2)} 秒</div>
        </section>
      )}

      {/* 主播放器与状态排版 */}
      <div style={{ display: 'grid', gridTemplateColumns: '1.2fr 0.8fr', gap: '24px', marginBottom: '24px' }}>
        
        {/* 左侧：播放器和控制条 */}
        <div>
          <div style={{ position: 'relative', width: '100%', background: '#000', borderRadius: '12px', overflow: 'hidden', boxShadow: 'var(--shadow)', aspectRatio: '16/9' }}>
            <video
              ref={videoRef}
              style={{ width: '100%', height: '100%', display: 'block' }}
              onClick={togglePlay}
            />
            {!metadata && (
              <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#fff', background: 'rgba(0,0,0,0.6)' }}>
                请点击上方“载入并解析”按钮加载安全视频流
              </div>
            )}
          </div>

          {/* 常用控制组件 */}
          <div style={{
            marginTop: '12px',
            background: 'var(--social-bg)',
            border: '1px solid var(--border)',
            borderRadius: '12px',
            padding: '16px',
            display: 'flex',
            flexDirection: 'column',
            gap: '12px'
          }}>
            {/* 播放进度条 */}
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <span style={{ fontSize: '13px', minWidth: '40px' }}>{formatTime(currentTime)}</span>
              <input
                type="range"
                min={0}
                max={duration || 100}
                value={currentTime}
                disabled={!metadata}
                onChange={(e) => setCurrentTime(parseFloat(e.target.value))}
                style={{ flex: 1, cursor: 'pointer', accentColor: 'var(--accent)' }}
              />
              <span style={{ fontSize: '13px', minWidth: '40px' }}>{formatTime(duration)}</span>
            </div>

            {/* 控制器底部 */}
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: '12px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <button
                  onClick={togglePlay}
                  disabled={!metadata}
                  style={{
                    padding: '6px 16px',
                    borderRadius: '6px',
                    border: '1px solid var(--border)',
                    background: 'var(--bg)',
                    color: 'var(--text-h)',
                    cursor: 'pointer',
                    fontSize: '14px',
                    fontWeight: '500'
                  }}
                >
                  {isPlaying ? '暂停' : '播放'}
                </button>

                <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <button
                    onClick={() => setMuted(!isMuted)}
                    disabled={!metadata}
                    style={{
                      padding: '6px 12px',
                      borderRadius: '6px',
                      border: '1px solid var(--border)',
                      background: 'var(--bg)',
                      color: 'var(--text-h)',
                      cursor: 'pointer',
                      fontSize: '13px'
                    }}
                  >
                    {isMuted ? '取消静音' : '静音'}
                  </button>
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.05}
                    value={isMuted ? 0 : volume}
                    disabled={!metadata}
                    onChange={(e) => {
                      setVolume(parseFloat(e.target.value));
                      if (isMuted) setMuted(false);
                    }}
                    style={{ width: '80px', accentColor: 'var(--accent)' }}
                  />
                </div>
              </div>

              {/* 倍速控制 */}
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span style={{ fontSize: '13px', color: 'var(--text)' }}>播放速度:</span>
                <select
                  value={playbackRate}
                  disabled={!metadata}
                  onChange={(e) => setPlaybackRate(parseFloat(e.target.value))}
                  style={{ padding: '4px 8px', borderRadius: '4px', border: '1px solid var(--border)', background: 'var(--bg)', color: 'var(--text-h)' }}
                >
                  <option value={0.5}>0.5x</option>
                  <option value={1.0}>1.0x (正常)</option>
                  <option value={1.5}>1.5x</option>
                  <option value={2.0}>2.0x</option>
                </select>
              </div>
            </div>
          </div>
        </div>

        {/* 右侧：分片解密可视化状态 & 终端日志 */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', height: '100%' }}>
          
          {/* 分片状态面板 */}
          <div style={{
            background: 'var(--social-bg)',
            border: '1px solid var(--border)',
            borderRadius: '12px',
            padding: '16px',
            flex: 1
          }}>
            <h3 style={{ margin: '0 0 12px 0', fontSize: '16px', color: 'var(--text-h)' }}>流媒体分片解密图谱</h3>
            {segmentStatuses.length === 0 ? (
              <p style={{ color: 'var(--text)', fontSize: '13px' }}>尚未载入流文件</p>
            ) : (
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(40px, 1fr))', gap: '6px' }}>
                {segmentStatuses.map((status, idx) => {
                  let bgColor = '#cbd5e1';
                  let border = '1px solid transparent';
                  if (status === 'loading') bgColor = '#f97316';
                  if (status === 'decrypted') bgColor = '#3b82f6';
                  if (status === 'active') {
                    bgColor = '#10b981';
                    border = '2px solid #047857';
                  }
                  return (
                    <div
                      key={idx}
                      title={`分片 ${idx}: ${status}`}
                      style={{
                        height: '28px',
                        background: bgColor,
                        border: border,
                        borderRadius: '4px',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        color: status === 'active' || status === 'decrypted' || status === 'loading' ? '#fff' : '#475569',
                        fontSize: '11px',
                        fontWeight: 'bold',
                        transition: 'all 0.2s'
                      }}
                    >
                      {idx}
                    </div>
                  );
                })}
              </div>
            )}
            {segmentStatuses.length > 0 && (
              <div style={{ marginTop: '16px', display: 'flex', gap: '12px', flexWrap: 'wrap', fontSize: '11px' }}>
                <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  <span style={{ display: 'inline-block', width: '12px', height: '12px', background: '#cbd5e1', borderRadius: '2px' }} /> 待处理 (Pending)
                </span>
                <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  <span style={{ display: 'inline-block', width: '12px', height: '12px', background: '#f97316', borderRadius: '2px' }} /> 解密中 (Loading)
                </span>
                <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  <span style={{ display: 'inline-block', width: '12px', height: '12px', background: '#3b82f6', borderRadius: '2px' }} /> 已解密 (Decrypted)
                </span>
                <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  <span style={{ display: 'inline-block', width: '12px', height: '12px', background: '#10b981', border: '1px solid #047857', borderRadius: '2px' }} /> 播放中 (Active)
                </span>
              </div>
            )}
          </div>

          {/* 实时终端日志 */}
          <div style={{
            background: '#0f172a',
            border: '1px solid var(--border)',
            borderRadius: '12px',
            padding: '16px',
            height: '220px',
            display: 'flex',
            flexDirection: 'column'
          }}>
            <h3 style={{ margin: '0 0 8px 0', fontSize: '14px', color: '#f8fafc', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span>实时系统日志</span>
              <button
                onClick={() => setLogs([])}
                style={{ background: 'none', border: 'none', color: '#94a3b8', cursor: 'pointer', fontSize: '12px' }}
              >
                清空
              </button>
            </h3>
            <div style={{
              flex: 1,
              overflowY: 'auto',
              fontFamily: 'monospace',
              fontSize: '11px',
              color: '#e2e8f0',
              lineHeight: '1.4'
            }}>
              {logs.length === 0 ? (
                <div style={{ color: '#64748b' }}>等待载入或播放...</div>
              ) : (
                logs.map((log) => {
                  let color = '#cbd5e1';
                  if (log.type === 'error') color = '#ef4444';
                  if (log.type === 'success') color = '#10b981';
                  if (log.type === 'wasm') color = '#c084fc';
                  return (
                    <div key={log.id} style={{ color, marginBottom: '2px', wordBreak: 'break-all' }}>
                      {log.text}
                    </div>
                  );
                })
              )}
              <div ref={logEndRef} />
            </div>
          </div>

        </div>
      </div>
    </div>
  );
}

export default App;
