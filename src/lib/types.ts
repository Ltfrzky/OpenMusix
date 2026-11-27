// Mirrors of the Rust JSON shapes (camelCase). Keep in sync with src-tauri/src/{model,settings,hub,lyrics}.rs.
export type Mode = "focus" | "widget";
export type Source = "windows" | "spotify";
export type TextEffect = "none" | "outline" | "shadow";
export type Align = "left" | "center" | "right";
export type Background = "transparent" | "panel";

export interface Track {
  title: string;
  artist: string;
  album: string | null;
  durationMs: number | null;
  artUrl: string | null;
}

export interface NowPlaying {
  track: Track;
  positionMs: number;
  sampledAt: number;
  isPlaying: boolean;
  source: Source;
}

export interface LyricLine {
  timeMs: number;
  text: string;
}

export type LyricsResult =
  | { kind: "synced"; value: LyricLine[] }
  | { kind: "plainOnly"; value: string }
  | { kind: "instrumental" }
  | { kind: "notFound" };

export type LyricsStatus = { status: "loading" } | { status: "error" } | { status: "ready"; result: LyricsResult };

export interface Style {
  preset: string;
  fontFamily: string;
  fontSizePx: number;
  textColor: string;
  highlightColor: string;
  dimColor: string;
  effect: TextEffect;
  align: Align;
  lineCount: number;
  showArt: boolean;
  background: Background;
}

export interface Styles {
  focus: Style;
  widget: Style;
}

export interface Snapshot {
  nowPlaying: NowPlaying | null;
  lyrics: LyricsStatus;
  trackKey: string | null;
  mode: Mode;
  styles: Styles;
  globalOffsetMs: number;
  trackOffsetMs: number;
  showPlainLyrics: boolean;
  hideWhenIdle: boolean;
}

export interface Settings {
  port: number;
  hotkey: string;
  mode: Mode;
  globalOffsetMs: number;
  trackOffsetsMs: Record<string, number>;
  styles: Styles;
  showPlainLyrics: boolean;
  hideWhenIdle: boolean;
  startWithWindows: boolean;
  minimizeToTray: boolean;
  floatingWindow: boolean;
  spotifyClientId: string | null;
}

export type ServerStatus =
  | { state: "running"; port: number }
  | { state: "portInUse"; port: number }
  | { state: "failed"; port: number; message: string };
