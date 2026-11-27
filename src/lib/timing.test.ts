import { expect, test } from "vitest";
import { PlaybackClock, effectivePosition } from "./timing";

const s = (positionMs: number, isPlaying = true, trackKey: string | null = "a") => ({ positionMs, sampledAt: 0, isPlaying, trackKey });
function clockAt(p: number) { const c = new PlaybackClock(); c.update(s(p), 0); return c; }

test("extrapolates while playing", () => { expect(clockAt(1000).positionAt(250)).toBe(1250); });
test("frozen while paused", () => { const c = new PlaybackClock(); c.update(s(1000, false), 0); expect(c.positionAt(5000)).toBe(1000); });
test("snaps on drift > 400", () => { const c = clockAt(1000); c.update(s(1450), 0); expect(c.positionAt(0)).toBe(1450); });
test("eases drift <= 400 over 300ms", () => {
  const c = clockAt(1000); c.update(s(1200), 0);
  expect(c.positionAt(0)).toBe(1000); expect(c.positionAt(150)).toBe(1250); expect(c.positionAt(300)).toBe(1500);
});
test("seek backwards snaps", () => { const c = clockAt(60000); c.update(s(5000), 0); expect(c.positionAt(0)).toBe(5000); });
test("track change snaps", () => { const c = clockAt(1000); c.update(s(1100, true, "b"), 0); expect(c.positionAt(0)).toBe(1100); });
test("pause snaps to reported position", () => { const c = clockAt(1000); c.update(s(1100, false), 0); expect(c.positionAt(500)).toBe(1100); });
test("empty clock is zero", () => { expect(new PlaybackClock().positionAt(123)).toBe(0); });
test("effective position adds offsets", () => { expect(effectivePosition(clockAt(1000), 0, 200, -50)).toBe(1150); });
