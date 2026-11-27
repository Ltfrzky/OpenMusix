// @vitest-environment jsdom
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

const invoke = vi.fn(async () => 0);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [])) }));
import OffsetControls from "./OffsetControls.svelte";

afterEach(() => { cleanup(); invoke.mockClear(); });

test("nudge buttons call command with ±100", async () => {
  const { getByRole } = render(OffsetControls, { globalOffsetMs: 0, trackOffsetMs: 0, hasTrack: true, onGlobal: () => {} });
  await fireEvent.click(getByRole("button", { name: "+100 ms" }));
  expect(invoke).toHaveBeenCalledWith("nudge_track_offset", { deltaMs: 100 });
  await fireEvent.click(getByRole("button", { name: "−100 ms" }));
  expect(invoke).toHaveBeenCalledWith("nudge_track_offset", { deltaMs: -100 });
});
test("nudge disabled without a track", () => {
  const { getByRole } = render(OffsetControls, { globalOffsetMs: 0, trackOffsetMs: 0, hasTrack: false, onGlobal: () => {} });
  expect((getByRole("button", { name: "+100 ms" }) as HTMLButtonElement).disabled).toBe(true);
});
