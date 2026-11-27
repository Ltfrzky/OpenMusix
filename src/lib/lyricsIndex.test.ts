import { expect, test } from "vitest";
import { currentLineIndex } from "./lyricsIndex";

test("line index", () => {
  const L = [{ timeMs: 1000 }, { timeMs: 2000 }, { timeMs: 3000 }];
  expect(currentLineIndex(L, 999)).toBe(-1);
  expect(currentLineIndex(L, 1000)).toBe(0);
  expect(currentLineIndex(L, 2999)).toBe(1);
  expect(currentLineIndex(L, 99999)).toBe(2);
  expect(currentLineIndex([], 5)).toBe(-1);
});
test("duplicate timestamps pick the last", () => {
  expect(currentLineIndex([{ timeMs: 0 }, { timeMs: 1000 }, { timeMs: 1000 }], 1000)).toBe(2);
});
