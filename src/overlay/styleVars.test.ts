import { expect, test } from "vitest";
import { styleVars } from "./styleVars";
import { PRESETS } from "../lib/presets";

test("styleVars maps colours", () => {
  const v = styleVars(PRESETS.spotlight);
  expect(v["--om-hl"]).toBe(PRESETS.spotlight.highlightColor);
  expect(v["--om-dim"]).toBe(PRESETS.spotlight.dimColor);
  expect(v["--om-size"]).toBe("44px");
  expect(v["--om-font"]).toContain("Outfit");
});
test("outline effect yields text-stroke shadow", () => {
  expect(styleVars(PRESETS.neon)["--om-shadow"]).not.toBe("none");
  expect(styleVars({ ...PRESETS.glass, effect: "none" })["--om-shadow"]).toBe("none");
});
test("presets match rust defaults", () => {
  expect(PRESETS.spotlight).toMatchObject({ fontFamily: "Outfit", fontSizePx: 44, align: "center", lineCount: 7, effect: "shadow", background: "transparent", showArt: false });
  expect(PRESETS.glass).toMatchObject({ fontFamily: "Inter", fontSizePx: 22, align: "left", lineCount: 3, effect: "none", background: "panel", showArt: true });
});
test("fit mode caps font size to the window height", () => {
  expect(styleVars(PRESETS.spotlight, { fit: true })["--om-size"]).toBe("min(44px, 11vh)");
  expect(styleVars(PRESETS.spotlight)["--om-size"]).toBe("44px");
});
