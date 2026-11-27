import { expect, test } from "vitest";
import { obsUrl, tiktokUrl } from "./obsUrl";

test("obs url", () => {
  expect(obsUrl(7878)).toBe("http://127.0.0.1:7878/overlay");
  expect(obsUrl(9000, "widget")).toBe("http://127.0.0.1:9000/overlay?mode=widget");
  expect(obsUrl(7878, "focus")).toBe("http://127.0.0.1:7878/overlay?mode=focus");
});

test("tiktok url uses a dotted domain and drops port 80", () => {
  expect(tiktokUrl(7878)).toBe("http://localtest.me:7878/overlay");
  expect(tiktokUrl(80)).toBe("http://localtest.me/overlay");
  expect(tiktokUrl(80, "widget")).toBe("http://localtest.me/overlay?mode=widget");
});
