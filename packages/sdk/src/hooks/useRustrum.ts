import { useState, useCallback, useEffect, useRef } from 'react';
import type { RefObject } from 'react';
import initWasm, {
  derive_key,
  parse_header,
  decrypt_chunk,
  WasmDecoder,
  type InitOutput,
  type WasmRstrHeader
} from '../pkg/rustrum_wasm.js';
import { RustrumPlayer, type RustrumPlayerOptions, type RustrumMetadata } from '../player.ts';

export interface UseRustrumReturn {
  isLoading: boolean;
  error: string | null;
  wasmInstance: InitOutput | null;
  initialize: (wasmInput?: ArrayBuffer | WebAssembly.Module) => Promise<InitOutput>;
  deriveKey: (password: string, salt: Uint8Array) => Uint8Array;
  parseHeader: (headerBytes: Uint8Array) => WasmRstrHeader;
  decryptChunk: (
    cipherId: number,
    key: Uint8Array,
    nonce: Uint8Array,
    encryptedData: Uint8Array
  ) => Uint8Array;
  createDecoder: (headerBytes: Uint8Array, password: string) => WasmDecoder;
}

export function useRustrum(): UseRustrumReturn {
  const [wasmInstance, setWasmInstance] = useState<InitOutput | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const initialize = useCallback(async (wasmInput?: ArrayBuffer | WebAssembly.Module) => {
    setIsLoading(true);
    setError(null);
    try {
      const instance = await initWasm(wasmInput);
      setWasmInstance(instance);
      setIsLoading(false);
      return instance;
    } catch (err: any) {
      const errMsg = err?.message || String(err);
      setError(errMsg);
      setIsLoading(false);
      throw err;
    }
  }, []);

  const deriveKey = useCallback((password: string, salt: Uint8Array): Uint8Array => {
    if (!wasmInstance) throw new Error('WASM not initialized');
    return derive_key(password, salt);
  }, [wasmInstance]);

  const parseHeader = useCallback((headerBytes: Uint8Array): WasmRstrHeader => {
    if (!wasmInstance) throw new Error('WASM not initialized');
    return parse_header(headerBytes);
  }, [wasmInstance]);

  const decryptChunk = useCallback((
    cipherId: number,
    key: Uint8Array,
    nonce: Uint8Array,
    encryptedData: Uint8Array
  ): Uint8Array => {
    if (!wasmInstance) throw new Error('WASM not initialized');
    return decrypt_chunk(cipherId, key, nonce, encryptedData);
  }, [wasmInstance]);

  const createDecoder = useCallback((headerBytes: Uint8Array, password: string): WasmDecoder => {
    if (!wasmInstance) throw new Error('WASM not initialized');
    return new WasmDecoder(headerBytes, password);
  }, [wasmInstance]);

  return {
    isLoading,
    error,
    wasmInstance,
    initialize,
    deriveKey,
    parseHeader,
    decryptChunk,
    createDecoder
  };
}

export interface UseRustrumPlayerReturn {
  player: RustrumPlayer | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
  playbackRate: number;
  metadata: RustrumMetadata | null;
  segmentStatuses: ('pending' | 'loading' | 'decrypted' | 'active')[];
  load: (rstrmSource: ArrayBuffer | string, rstrSource: File | string, password: string) => Promise<RustrumMetadata>;
  play: () => Promise<void>;
  pause: () => void;
  togglePlay: () => void;
  setVolume: (volume: number) => void;
  setMuted: (muted: boolean) => void;
  setCurrentTime: (time: number) => void;
  setPlaybackRate: (rate: number) => void;
}

export function useRustrumPlayer(
  videoRef: RefObject<HTMLVideoElement | null>,
  options: RustrumPlayerOptions = {}
): UseRustrumPlayerReturn {
  const [player, setPlayer] = useState<RustrumPlayer | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTimeState] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(1);
  const [isMuted, setIsMutedState] = useState(false);
  const [playbackRate, setPlaybackRateState] = useState(1);
  const [metadata, setMetadata] = useState<RustrumMetadata | null>(null);
  const [segmentStatuses, setSegmentStatuses] = useState<('pending' | 'loading' | 'decrypted' | 'active')[]>([]);

  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) {
      setPlayer(null);
      return;
    }

    const playerOptions: RustrumPlayerOptions = {
      ...optionsRef.current,
      onLog: (msg, type) => {
        optionsRef.current.onLog?.(msg, type);
      },
      onSegmentStatusChange: (idx, status) => {
        optionsRef.current.onSegmentStatusChange?.(idx, status);
        if (playerInstance) {
          setSegmentStatuses(playerInstance.getSegmentStatuses());
        }
      },
      onPlay: () => {
        optionsRef.current.onPlay?.();
        setIsPlaying(true);
      },
      onPause: () => {
        optionsRef.current.onPause?.();
        setIsPlaying(false);
      },
      onTimeUpdate: (curTime, dur) => {
        optionsRef.current.onTimeUpdate?.(curTime, dur);
        setCurrentTimeState(curTime);
        setDuration(dur);
      },
      onVolumeChange: (vol, muted) => {
        optionsRef.current.onVolumeChange?.(vol, muted);
        setVolumeState(vol);
        setIsMutedState(muted);
      },
      onPlaybackRateChange: (rate) => {
        optionsRef.current.onPlaybackRateChange?.(rate);
        setPlaybackRateState(rate);
      },
      onEnded: () => {
        optionsRef.current.onEnded?.();
        setIsPlaying(false);
      },
      onError: (err) => {
        optionsRef.current.onError?.(err);
      }
    };

    const playerInstance = new RustrumPlayer(video, playerOptions);
    setPlayer(playerInstance);

    setVolumeState(video.volume);
    setIsMutedState(video.muted);
    setPlaybackRateState(video.playbackRate);

    return () => {
      playerInstance.destroy();
      setPlayer(null);
      setMetadata(null);
      setSegmentStatuses([]);
      setIsPlaying(false);
    };
  }, [videoRef]);

  const load = useCallback(async (
    rstrmSource: ArrayBuffer | string,
    rstrSource: File | string,
    password: string
  ) => {
    if (!player) throw new Error('Player not initialized');
    const meta = await player.load(rstrmSource, rstrSource, password);
    setMetadata(meta);
    setSegmentStatuses(player.getSegmentStatuses());
    return meta;
  }, [player]);

  const play = useCallback(async () => {
    if (!player) throw new Error('Player not initialized');
    await player.play();
  }, [player]);

  const pause = useCallback(() => {
    player?.pause();
  }, [player]);

  const togglePlay = useCallback(() => {
    player?.togglePlay();
  }, [player]);

  const setVolume = useCallback((vol: number) => {
    player?.setVolume(vol);
  }, [player]);

  const setMuted = useCallback((muted: boolean) => {
    player?.setMuted(muted);
  }, [player]);

  const setCurrentTime = useCallback((time: number) => {
    player?.setCurrentTime(time);
  }, [player]);

  const setPlaybackRate = useCallback((rate: number) => {
    player?.setPlaybackRate(rate);
  }, [player]);

  return {
    player,
    isPlaying,
    currentTime,
    duration,
    volume,
    isMuted,
    playbackRate,
    metadata,
    segmentStatuses,
    load,
    play,
    pause,
    togglePlay,
    setVolume,
    setMuted,
    setCurrentTime,
    setPlaybackRate
  };
}
