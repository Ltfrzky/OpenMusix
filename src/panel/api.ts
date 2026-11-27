import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Mode, ServerStatus, Settings, Snapshot } from "../lib/types";

export const api = {
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  setMode: (mode: Mode) => invoke<void>("set_mode", { mode }),
  nudge: (deltaMs: number) => invoke<number>("nudge_track_offset", { deltaMs }),
  resetTrackOffset: () => invoke<void>("reset_track_offset"),
  serverStatus: () => invoke<ServerStatus>("get_server_status"),
  restartServer: () => invoke<ServerStatus>("restart_server"),
  clearCache: () => invoke<void>("clear_lyrics_cache"),
  openLyricsFolder: () => invoke<void>("open_lyrics_folder"),
  systemFonts: () => invoke<string[]>("list_system_fonts"),
};

export function onSnapshot(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("snapshot", (e) => cb(e.payload));
}
export function onServerStatus(cb: (s: ServerStatus) => void): Promise<UnlistenFn> {
  return listen<ServerStatus>("server-status", (e) => cb(e.payload));
}
