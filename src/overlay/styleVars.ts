import type { Style } from "../lib/types";
import { BUNDLED_FONTS } from "../lib/presets";

const SHADOW = "0 2px 14px rgba(0,0,0,0.55), 0 1px 3px rgba(0,0,0,0.65)";
const OUTLINE = "-1.5px -1.5px 0 #000, 1.5px -1.5px 0 #000, -1.5px 1.5px 0 #000, 1.5px 1.5px 0 #000, 0 0 10px rgba(0,0,0,0.45)";

export function fontStack(family: string): string {
  const f = family.replace(/"/g, "");
  const primary = BUNDLED_FONTS.includes(f) ? `"${f} Variable", "${f}"` : `"${f}"`;
  return `${primary}, "Segoe UI", system-ui, sans-serif`;
}

/** CSS custom properties consumed by the overlay views. `fit` caps text to the window height (floating window). */
export function styleVars(s: Style, opts: { fit?: boolean } = {}): Record<string, string> {
  return {
    "--om-font": fontStack(s.fontFamily),
    "--om-size": opts.fit ? `min(${s.fontSizePx}px, 11vh)` : `${s.fontSizePx}px`,
    "--om-text": s.textColor,
    "--om-hl": s.highlightColor,
    "--om-dim": s.dimColor,
    "--om-align": s.align,
    "--om-origin": s.align === "left" ? "left center" : s.align === "right" ? "right center" : "center center",
    "--om-shadow": s.effect === "shadow" ? SHADOW : s.effect === "outline" ? OUTLINE : "none",
  };
}

export function cssText(vars: Record<string, string>): string {
  return Object.entries(vars).map(([k, v]) => `${k}: ${v}`).join("; ");
}
