/** Counts playback position forward locally between backend samples, hiding small drift. */
export interface ClockSample {
  positionMs: number;
  /** Unix epoch ms at which positionMs was true. */
  sampledAt: number;
  isPlaying: boolean;
  trackKey: string | null;
}

export const SNAP_THRESHOLD_MS = 400;
export const EASE_MS = 300;

export class PlaybackClock {
  private sample: ClockSample | null = null;
  private correction = 0;
  private t0 = 0;

  private target(now: number): number {
    const s = this.sample;
    if (!s) return 0;
    return s.positionMs + (s.isPlaying ? Math.max(0, now - s.sampledAt) : 0);
  }

  private residual(now: number): number {
    const k = Math.max(0, 1 - (now - this.t0) / EASE_MS);
    return this.correction * k;
  }

  update(s: ClockSample, now: number): void {
    const prev = this.sample;
    const shown = prev ? this.target(now) + this.residual(now) : 0;
    this.sample = s;
    if (!prev || prev.trackKey !== s.trackKey) {
      this.correction = 0;
      return;
    }
    const drift = this.target(now) - shown;
    if (Math.abs(drift) > SNAP_THRESHOLD_MS) {
      this.correction = 0;
    } else {
      this.correction = -drift;
      this.t0 = now;
    }
  }

  positionAt(now: number): number {
    return this.target(now) + this.residual(now);
  }
}

export function effectivePosition(clock: PlaybackClock, now: number, globalOffsetMs: number, trackOffsetMs: number): number {
  return clock.positionAt(now) + globalOffsetMs + trackOffsetMs;
}
