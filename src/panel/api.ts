import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Mode, PackInfo, RomanLang, ServerStatus, Settings, Snapshot } from "../lib/types";

export const api = {
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("update_settings", { patch }),
  setMode: (mode: Mode) => invoke<void>("set_mode", { mode }),
  nudge: (deltaMs: number) => invoke<number>("nudge_track_offset", { deltaMs }),
  resetTrackOffset: () => invoke<void>("reset_track_offset"),
  serverStatus: () => invoke<ServerStatus>("get_server_status"),
  restartServer: () => invoke<ServerStatus>("restart_server"),
  clearCache: () => invoke<void>("clear_lyrics_cache"),
  openLyricsFolder: () => invoke<void>("open_lyrics_folder"),
  systemFonts: () => invoke<string[]>("list_system_fonts"),
  spotifyStatus: () => invoke<SpotifyStatus>("spotify_status"),
  spotifyConnect: () => invoke<void>("spotify_connect"),
  spotifyDisconnect: () => invoke<void>("spotify_disconnect"),
  listPacks: () => invoke<PackInfo[]>("list_packs"),
  installPack: (id: RomanLang) => invoke<void>("install_pack", { id }),
  removePack: (id: RomanLang) => invoke<void>("remove_pack", { id }),
  cancelPackInstall: (id: RomanLang) => invoke<void>("cancel_pack_install", { id }),
};

export function onPackProgress(cb: (p: { id: RomanLang; pct: number }) => void): Promise<UnlistenFn> {
  return listen<{ id: RomanLang; pct: number }>("pack-progress", (e) => cb(e.payload));
}
export function onPacksChanged(cb: (p: PackInfo[]) => void): Promise<UnlistenFn> {
  return listen<PackInfo[]>("packs-changed", (e) => cb(e.payload));
}

export interface SpotifyStatus {
  connected: boolean;
  clientIdSet: boolean;
  redirectUri: string;
}

export function onSpotifyStatus(cb: (connected: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("spotify-status", (e) => cb(e.payload));
}

export function onSnapshot(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("snapshot", (e) => cb(e.payload));
}
export function onServerStatus(cb: (s: ServerStatus) => void): Promise<UnlistenFn> {
  return listen<ServerStatus>("server-status", (e) => cb(e.payload));
}
