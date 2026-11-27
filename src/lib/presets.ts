import type { Style } from "./types";

/** Built-in looks. `spotlight` and `glass` must equal the Rust defaults in settings.rs. */
export const PRESETS: Record<"spotlight" | "glass" | "neon", Style> = {
  spotlight: {
    preset: "spotlight", fontFamily: "Outfit", fontSizePx: 44, textColor: "#FFFFFF", highlightColor: "#FFFFFF",
    dimColor: "rgba(255,255,255,0.45)", effect: "shadow", align: "center", lineCount: 7, showArt: false, background: "transparent",
  },
  glass: {
    preset: "glass", fontFamily: "Inter", fontSizePx: 22, textColor: "#F4F4F5", highlightColor: "#FFFFFF",
    dimColor: "rgba(244,244,245,0.55)", effect: "none", align: "left", lineCount: 3, showArt: true, background: "panel",
  },
  neon: {
    preset: "neon", fontFamily: "Outfit", fontSizePx: 40, textColor: "#E6FFF8", highlightColor: "#7CF7D4",
    dimColor: "rgba(230,255,248,0.4)", effect: "outline", align: "center", lineCount: 5, showArt: false, background: "transparent",
  },
};

export const BUNDLED_FONTS = ["Inter", "Outfit", "Lora"];
