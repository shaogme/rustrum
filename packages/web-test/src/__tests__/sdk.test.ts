import { describe, it, expect, vi, beforeAll } from 'vitest';
import { RustrumPlayer } from 'rustrum-sdk';

// Mock global fetch
global.fetch = vi.fn();

describe('RustrumPlayer SDK Unit Tests', () => {
  let videoElement: HTMLVideoElement;

  beforeAll(() => {
    videoElement = document.createElement('video');
  });

  it('should successfully instantiate the player with options', () => {
    const onLog = vi.fn();
    const onSegmentStatusChange = vi.fn();
    
    const player = new RustrumPlayer(videoElement, {
      onLog,
      onSegmentStatusChange,
    });

    expect(player).toBeDefined();
    expect(player.load).toBeInstanceOf(Function);
    expect(player.destroy).toBeInstanceOf(Function);
    expect(player.getMetadata()).toBeNull();
    expect(player.getSegmentStatuses()).toEqual([]);
  });

  it('should clean up event listeners on destroy', () => {
    const player = new RustrumPlayer(videoElement);
    const removeEventListenerSpy = vi.spyOn(videoElement, 'removeEventListener');

    player.destroy();

    expect(removeEventListenerSpy).toBeDefined();
  });
});
