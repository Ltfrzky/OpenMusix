// @vitest-environment jsdom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { PackInfo, Settings, Snapshot } from "../lib/types";
import { PRESETS } from "../lib/presets";

let packs: PackInfo[] = [];
const installPack = vi.fn(async (_id: string) => {});
const cancelPackInstall = vi.fn(async (_id: string) => {});
vi.mock("./api", () => ({
  api: {
    listPacks: vi.fn(async () => packs),
    installPack: (id: string) => installPack(id),
    removePack: vi.fn(async () => {}),
    cancelPackInstall: (id: string) => cancelPackInstall(id),
  },
  onPackProgress: vi.fn(async () => () => {}),
  onPacksChanged: vi.fn(async () => () => {}),
}));
import RomanizationSection from "./RomanizationSection.svelte";
import NowPlayingCard from "./NowPlayingCard.svelte";

const settings: Settings = {
  port: 7878, hotkey: "Ctrl+Alt+L", mode: "focus", globalOffsetMs: 0, trackOffsetsMs: {},
  styles: { focus: PRESETS.spotlight, widget: PRESETS.glass }, showPlainLyrics: false, hideWhenIdle: false,
  startWithWindows: false, minimizeToTray: true, floatingWindow: false, spotifyClientId: null,
  romanize: { enabled: false, display: "below" },
};
const tick = (ms = 10) => new Promise((r) => setTimeout(r, ms));

beforeEach(() => {
  installPack.mockClear();
  cancelPackInstall.mockClear();
  packs = [
    { id: "zh", label: "Chinese (pinyin)", size: 300_000, state: "notInstalled" },
    { id: "ja", label: "Japanese (romaji)", size: 15_000_000, state: "installed" },
    { id: "ko", label: "Korean", size: 0, state: "builtIn" },
  ];
});
afterEach(() => cleanup());

test("switch saves the whole romanize object", async () => {
  const onSave = vi.fn(async () => null);
  const { getByLabelText } = render(RomanizationSection, { settings, onSave });
  await fireEvent.click(getByLabelText("Romanize Chinese, Japanese and Korean lyrics"));
  expect(onSave).toHaveBeenCalledWith({ romanize: { enabled: true, display: "below" } });
});

test("display radio saves replace", async () => {
  const onSave = vi.fn(async () => null);
  const s = { ...settings, romanize: { enabled: true, display: "below" as const } };
  const { getByLabelText } = render(RomanizationSection, { settings: s, onSave });
  await fireEvent.click(getByLabelText("Instead of the original"));
  expect(onSave).toHaveBeenCalledWith({ romanize: { enabled: true, display: "replace" } });
});

test("pack list shows rows and download installs", async () => {
  const { getByText, getAllByRole } = render(RomanizationSection, { settings, onSave: vi.fn(async () => null) });
  await tick();
  expect(getByText("Built-in")).toBeTruthy();
  expect(getByText("Remove")).toBeTruthy();
  expect(getByText("0.3 MB")).toBeTruthy();
  const download = getAllByRole("button").find((b) => b.textContent?.includes("Download"))!;
  await fireEvent.click(download);
  expect(installPack).toHaveBeenCalledWith("zh");
});

test("installing shows percent and cancel", async () => {
  packs[0] = { ...packs[0], state: "installing", pct: 62 };
  const { getByText } = render(RomanizationSection, { settings, onSave: vi.fn(async () => null) });
  await tick();
  expect(getByText("62%")).toBeTruthy();
  await fireEvent.click(getByText("Cancel"));
  expect(cancelPackInstall).toHaveBeenCalledWith("zh");
});

test("broken pack offers reinstall", async () => {
  packs[1] = { ...packs[1], state: "broken" };
  const { getByText } = render(RomanizationSection, { settings, onSave: vi.fn(async () => null) });
  await tick();
  expect(getByText("Reinstall")).toBeTruthy();
});

function snap(over: Partial<Snapshot>): Snapshot {
  return {
    nowPlaying: { track: { title: "Song", artist: "Artist", album: null, durationMs: 1, artUrl: null }, positionMs: 0, sampledAt: 0, isPlaying: true, source: "system" },
    lyrics: { status: "loading" }, trackKey: "a|s", mode: "focus", styles: { focus: PRESETS.spotlight, widget: PRESETS.glass },
    globalOffsetMs: 0, trackOffsetMs: 0, showPlainLyrics: false, hideWhenIdle: false,
    romanize: { enabled: true, display: "below" }, roman: null, romanMissing: [], ...over,
  };
}

test("missing pack hint appears and installs", async () => {
  const { getByText } = render(NowPlayingCard, { snapshot: snap({ romanMissing: ["ja"] }), artBase: "" });
  expect(getByText("This song is in Japanese. Install the Japanese pack to romanize it.")).toBeTruthy();
  await fireEvent.click(getByText("Install"));
  expect(installPack).toHaveBeenCalledWith("ja");
});

test("install failure from the hint is shown", async () => {
  installPack.mockRejectedValueOnce("download failed: offline");
  const { getByText, findByText } = render(NowPlayingCard, { snapshot: snap({ romanMissing: ["ja"] }), artBase: "" });
  await fireEvent.click(getByText("Install"));
  expect(await findByText("download failed: offline")).toBeTruthy();
});

test("missing pack hint hidden when romanization is off", () => {
  const { queryByText } = render(NowPlayingCard, { snapshot: snap({ romanMissing: ["ja"], romanize: { enabled: false, display: "below" } }), artBase: "" });
  expect(queryByText(/Install the Japanese pack/)).toBeNull();
});
