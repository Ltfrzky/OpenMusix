// @vitest-environment jsdom
import { afterEach, expect, test } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
afterEach(() => cleanup());
import Overlay from "./Overlay.svelte";
import { PRESETS } from "../lib/presets";
import type { LyricsStatus, Snapshot } from "../lib/types";

function snap(lyrics: LyricsStatus, over: Partial<Snapshot> = {}): Snapshot {
  return {
    nowPlaying: { track: { title: "Song", artist: "Artist", album: null, durationMs: 200000, artUrl: null }, positionMs: 0, sampledAt: Date.now(), isPlaying: true, source: "windows" },
    lyrics, trackKey: "artist|song", mode: "focus", styles: { focus: PRESETS.spotlight, widget: PRESETS.glass },
    globalOffsetMs: 0, trackOffsetMs: 0, showPlainLyrics: false, hideWhenIdle: false,
    romanize: { enabled: false, display: "below" }, roman: null, romanMissing: [], ...over,
  };
}
const synced: LyricsStatus = { status: "ready", result: { kind: "synced", value: [{ timeMs: 0, text: "First line" }, { timeMs: 99999, text: "Later" }] } };

test("focus renders synced lines", () => {
  const { container, getByText } = render(Overlay, { snapshot: snap(synced) });
  expect(container.querySelector(".om-focus")).not.toBeNull();
  expect(getByText("First line")).toBeTruthy();
});
test("pinned mode overrides snapshot mode", () => {
  const { container } = render(Overlay, { snapshot: snap(synced), pinnedMode: "widget" });
  expect(container.querySelector(".om-widget")).not.toBeNull();
  expect(container.querySelector(".om-focus")).toBeNull();
});
test("instrumental label", () => {
  const { getByText } = render(Overlay, { snapshot: snap({ status: "ready", result: { kind: "instrumental" } }) });
  expect(getByText("♪ Instrumental")).toBeTruthy();
});
test("plain hidden by default", () => {
  const { getByText, queryByText } = render(Overlay, { snapshot: snap({ status: "ready", result: { kind: "plainOnly", value: "plain words" } }) });
  expect(getByText("No synced lyrics")).toBeTruthy();
  expect(queryByText("plain words")).toBeNull();
});
test("plain shown when enabled", () => {
  const { getByText } = render(Overlay, { snapshot: snap({ status: "ready", result: { kind: "plainOnly", value: "plain words" } }, { showPlainLyrics: true }) });
  expect(getByText("plain words")).toBeTruthy();
});
test("error shows no lyrics text", () => {
  const { queryByText, container } = render(Overlay, { snapshot: snap({ status: "error" }) });
  expect(queryByText("First line")).toBeNull();
  expect(container.querySelector(".om-line")).toBeNull();
});
test("idle hidden when hideWhenIdle", () => {
  const { container } = render(Overlay, { snapshot: snap(synced, { nowPlaying: null, hideWhenIdle: true }) });
  expect(container.querySelector(".om-root")).toBeNull();
});
