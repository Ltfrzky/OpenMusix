// Dev-only: renders the control panel in a normal browser with mocked Tauri IPC,
// fed by live snapshots from a running app at ws://127.0.0.1:7878/ws.
// Open http://localhost:1420/dev/panel-preview.html while `npm run tauri dev` runs.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { PRESETS } from "../src/lib/presets";
import type { Settings, Snapshot } from "../src/lib/types";

let settings: Settings = {
  port: 7878, hotkey: "Ctrl+Alt+L", mode: "focus", globalOffsetMs: 0, trackOffsetsMs: {},
  styles: { focus: PRESETS.spotlight, widget: PRESETS.glass }, showPlainLyrics: false, hideWhenIdle: false,
  startWithWindows: false, minimizeToTray: true, floatingWindow: false, spotifyClientId: null,
  romanize: { enabled: false, display: "below" },
};
let snapshot: Snapshot | null = null;

mockWindows("main");
mockIPC(
  (cmd, args) => {
    const a = args as Record<string, unknown>;
    switch (cmd) {
      case "get_settings": return settings;
      case "update_settings": settings = { ...settings, ...(a.patch as Partial<Settings>) }; return settings;
      case "set_mode": settings = { ...settings, mode: a.mode as Settings["mode"] }; return null;
      case "get_snapshot": return snapshot;
      case "get_server_status": return new URLSearchParams(location.search).has("busy") ? { state: "portInUse", port: 7878 } : { state: "running", port: 7878 };
      case "list_system_fonts": return ["Arial", "Bahnschrift", "Segoe UI"];
      case "spotify_status": return { connected: false, clientIdSet: false, redirectUri: "http://127.0.0.1:7878/spotify/callback" };
      default: return null;
    }
  },
  { shouldMockEvents: true },
);

const ws = new WebSocket("ws://127.0.0.1:7878/ws");
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.type === "snapshot") {
    snapshot = { ...m.data, mode: settings.mode, styles: settings.styles };
    emit("snapshot", snapshot);
  }
};

await import("../src/panel/main");
