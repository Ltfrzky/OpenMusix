import type { Mode } from "../lib/types";

export function obsUrl(port: number, mode?: Mode): string {
  const base = `http://127.0.0.1:${port}/overlay`;
  return mode ? `${base}?mode=${mode}` : base;
}
