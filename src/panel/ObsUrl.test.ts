import { expect, test } from "vitest";
import { obsUrl } from "./obsUrl";

test("obs url", () => {
  expect(obsUrl(7878)).toBe("http://127.0.0.1:7878/overlay");
  expect(obsUrl(9000, "widget")).toBe("http://127.0.0.1:9000/overlay?mode=widget");
  expect(obsUrl(7878, "focus")).toBe("http://127.0.0.1:7878/overlay?mode=focus");
});
