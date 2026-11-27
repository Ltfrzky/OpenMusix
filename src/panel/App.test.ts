// @vitest-environment jsdom
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";

const order: string[] = [];
let releaseListen: () => void = () => {};
vi.mock("./api", () => ({
  api: {
    getSnapshot: vi.fn(async () => { order.push("getSnapshot"); return null; }),
    getSettings: vi.fn(async () => { order.push("getSettings"); return null; }),
    serverStatus: vi.fn(async () => { order.push("serverStatus"); return null; }),
  },
  onSnapshot: vi.fn(() => new Promise<() => void>((r) => { releaseListen = () => { order.push("listening"); r(() => {}); }; })),
  onServerStatus: vi.fn(async () => () => {}),
}));
import App from "./App.svelte";

afterEach(() => cleanup());

test("snapshot is fetched only after the event listener is registered", async () => {
  render(App);
  await new Promise((r) => setTimeout(r, 10));
  expect(order).not.toContain("getSnapshot");
  releaseListen();
  await new Promise((r) => setTimeout(r, 10));
  expect(order.indexOf("listening")).toBeLessThan(order.indexOf("getSnapshot"));
});
