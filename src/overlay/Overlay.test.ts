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

const zh: LyricsStatus = { status: "ready", result: { kind: "synced", value: [{ timeMs: 0, text: "你好" }] } };
const on = (display: "below" | "replace") => ({ romanize: { enabled: true, display }, roman: ["nǐ hǎo"] as (string | null)[] });

test("renders roman under the line", () => {
  const { getByText } = render(Overlay, { snapshot: snap(zh, on("below")) });
  expect(getByText("你好")).toBeTruthy();
  expect(getByText("nǐ hǎo").classList.contains("om-roman")).toBe(true);
});
test("replace shows only roman", () => {
  const { getByText, queryByText } = render(Overlay, { snapshot: snap(zh, on("replace")) });
  expect(queryByText("你好")).toBeNull();
  expect(getByText("nǐ hǎo")).toBeTruthy();
});
test("replace falls back to original for null", () => {
  const { getByText } = render(Overlay, { snapshot: snap(zh, { romanize: { enabled: true, display: "replace" }, roman: [null] }) });
  expect(getByText("你好")).toBeTruthy();
});
test("null roman renders as today", () => {
  const { container } = render(Overlay, { snapshot: snap(zh) });
  expect(container.querySelector(".om-roman")).toBeNull();
});
test("plain lyrics romanized per line", () => {
  const plain: LyricsStatus = { status: "ready", result: { kind: "plainOnly", value: "你好\n再见" } };
  const { container } = render(Overlay, { snapshot: snap(plain, { showPlainLyrics: true, romanize: { enabled: true, display: "below" }, roman: ["nǐ hǎo", null] }) });
  expect(container.querySelectorAll(".plain-line").length).toBe(2);
  expect(container.querySelectorAll(".om-roman").length).toBe(1);
});
test("widget view also renders roman", () => {
  const { container } = render(Overlay, { snapshot: snap(zh, on("below")), pinnedMode: "widget" });
  expect(container.querySelector(".om-roman")?.textContent).toBe("nǐ hǎo");
});
