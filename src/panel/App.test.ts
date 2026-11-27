// @vitest-environment jsdom
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { PRESETS } from "../lib/presets";

const order: string[] = [];
let releaseListen: () => void = () => {};
const settings = {
  port: 7878, hotkey: "Ctrl+Alt+L", mode: "widget", globalOffsetMs: 0, trackOffsetsMs: { "a|b": 300 },
  styles: { focus: PRESETS.spotlight, widget: PRESETS.glass }, showPlainLyrics: false, hideWhenIdle: false,
  startWithWindows: false, minimizeToTray: true, floatingWindow: false, spotifyClientId: null,
};
const updateSettings = vi.fn(async (patch: object) => ({ ...settings, ...patch }));
vi.mock("./api", () => ({
  api: {
    getSnapshot: vi.fn(async () => { order.push("getSnapshot"); return null; }),
    getSettings: vi.fn(async () => { order.push("getSettings"); return settings; }),
    serverStatus: vi.fn(async () => { order.push("serverStatus"); return { state: "running", port: 7878 }; }),
    systemFonts: vi.fn(async () => []),
    updateSettings: (p: object) => updateSettings(p),
    setMode: vi.fn(async () => {}),
  },
  onSnapshot: vi.fn(() => new Promise<() => void>((r) => { releaseListen = () => { order.push("listening"); r(() => {}); }; })),
  onServerStatus: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => 0) }));
(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} };
import App from "./App.svelte";

beforeEach(() => { order.length = 0; updateSettings.mockClear(); });
afterEach(() => cleanup());
const tick = (ms = 10) => new Promise((r) => setTimeout(r, ms));

test("snapshot is fetched only after the event listener is registered", async () => {
  render(App);
  await tick();
  expect(order).not.toContain("getSnapshot");
  releaseListen();
  await tick();
  expect(order.indexOf("listening")).toBeLessThan(order.indexOf("getSnapshot"));
});

test("moving a control sends only that field, never a stale full settings object", async () => {
  const { container } = render(App);
  releaseListen();
  await tick();
  await tick(50);
  const slider = container.querySelector("#global") as HTMLInputElement;
  await fireEvent.input(slider, { target: { value: "500" } });
  await tick(250);
  expect(updateSettings).toHaveBeenCalledTimes(1);
  expect(updateSettings).toHaveBeenCalledWith({ globalOffsetMs: 500 });
});
