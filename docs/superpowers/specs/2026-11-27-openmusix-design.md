# OpenMusix — Design

> **Status:** Approved 2026-11-27. Implementation plan: `docs/superpowers/plans/2026-11-27-openmusix.md`.

## Context / why
Streamers want viewers to see the lyrics of whatever song is playing, scrolling in sync the way Spotify or Shazam shows them. OpenMusix is a small Windows desktop app that detects the current song, fetches time-synced lyrics and shows them as an OBS-ready overlay. It will be shared with other streamers, so first-run setup must be close to zero, and it has to look and feel like a finished product. The project folder `D:\Project\OpenMusix` is empty; this is a fresh build.

## Goals
- Detect the currently playing song and its playback position with **no account or key required** (Windows media session).
- Optionally **Connect Spotify** for more precise timing and song details.
- Fetch synced lyrics and highlight the current line in time, with smooth scrolling.
- An overlay at a local URL for an OBS Browser Source, with a transparent background.
- **Two layouts, switchable live:** *Focus* (big, centred, the main event) and *Widget* (a compact corner card).
- Desktop control panel: status, live preview, mode switch, styling, timing offset, settings.
- A Windows installer anyone can run.

## Non-goals (v1)
- Word-by-word karaoke highlighting (LRCLIB is line-level).
- macOS and Linux support.
- Translations or romanization, editing lyrics in the app, playback controls.
- Using Spotify's own (Musixmatch) lyrics through unofficial endpoints, which would break Spotify's terms.

## Anti-goals (a failure even if it technically works)
- Basic use needs any signup, key or login.
- Visible stutter, jumps or late lines on stream.
- Confidently showing **wrong** lyrics. Showing none is better than showing the wrong song's lyrics.
- Overlay styling that looks amateur by default.

## Constraints
- Windows 10/11 only (it depends on the Windows media session API).
- LRCLIB is the lyrics source: free, no key, line-level LRC, and it asks callers to send an identifying User-Agent.
- Spotify Web API: each user brings their own Client ID, because personal developer-mode apps are capped at a few users. The PKCE flow needs no secret. Redirect URIs must use `http://127.0.0.1:<port>`; `localhost` is no longer accepted.
- The Spotify Web API is polled under rate limits.

## Approaches considered
| | Approach | Trade-off |
|---|---|---|
| A | Windows media session only (Tauri) | Zero setup and works with any player, but timing is a little less precise |
| B | Spotify Web API only | Precise, but Spotify only, and every user must create a developer app |
| **C (chosen)** | **A by default, plus optional Connect Spotify** | Best of both; about 30% more initial work |

**The user picked C.** Option A alone would leave Spotify users who care about precision without an upgrade. Option B alone breaks the zero-setup goal for a shared app.

## Architecture
**Stack (my call):** Tauri v2, a Rust backend, and a TypeScript + Svelte (Vite) frontend. The installer stays around 10 MB. Rust can call the Windows media session directly through the `windows` crate. One web frontend serves both the control panel and the overlay.

```
 ┌─────────────── Rust core (Tauri) ───────────────┐
 │ PlaybackSource trait                            │
 │   ├─ WindowsMediaSource  (GSMTC, event-driven)  │
 │   └─ SpotifySource       (Web API, PKCE)        │
 │ PlaybackMerger  → NowPlaying{track, pos, t, playing}
 │ LyricsService   → LRCLIB + disk cache + local .lrc
 │ Settings store  (JSON in app data dir)          │
 │ Local HTTP+WS server 127.0.0.1:<port>           │
 └──────────┬───────────────────────────┬──────────┘
      Tauri IPC                    HTTP / WebSocket
 ┌──────────▼─────────┐      ┌──────────▼───────────────┐
 │ Control panel UI   │      │ /overlay page (OBS +     │
 │ (+ live preview)   │      │ in-app preview + floating│
 └────────────────────┘      │ window)                  │
                             └──────────────────────────┘
```

### Playback sources
- **WindowsMediaSource:** uses `GlobalSystemMediaTransportControlsSessionManager`. It prefers the Spotify session (source app ID containing `Spotify`) and otherwise falls back to the current session. It reads title, artist, album, playback status and the timeline (`Position` + `LastUpdatedTime`), and subscribes to media, playback and timeline change events rather than polling.
- **SpotifySource:** optional. The user pastes their Client ID in Settings, and the app shows a short step-by-step guide to creating one. It uses PKCE through a loopback redirect served by the app's own server, stores the refresh token in Windows Credential Manager (`keyring` crate), and refreshes tokens automatically. It polls `GET /me/player/currently-playing` about every 2 s while playing, and immediately when the Windows source reports a change. It backs off on HTTP 429.
- **Merger:** when Spotify is connected and reports the same track, its position and duration win. Otherwise the Windows source is used. Output is `NowPlaying { track{title, artist, album, duration_ms?, art_url?}, position_ms, sampled_at, is_playing, source }`. Album art comes from Spotify when connected, otherwise from the media session thumbnail, served by the local server.

### Timing
- The frontend counts forward from the latest `(position_ms, sampled_at)` with `requestAnimationFrame`. A new sample with a drift over 400 ms snaps into place; smaller drift is eased over about 300 ms so nothing visibly jumps.
- **Global offset** slider (±2 s, default +0, adjustable live). **Per-track offset** nudges are saved by track key.

### Lyrics
- Lookup order: a local override folder (`<appdata>/lyrics/*.lrc`, matched by "Artist - Title"), then the disk cache, then LRCLIB `/api/get` (track, artist, album, duration), then LRCLIB `/api/search` with a duration filter.
- **Wrong-lyrics guard:** reject a match whose duration differs by more than 3 s when the duration is known. Normalize titles by removing "- Remastered", "(feat. …)", "- Live" and similar. If nothing passes, show no lyrics.
- Results: `Synced(lines)`, `PlainOnly(text)`, `Instrumental` or `NotFound`. LRC is parsed in Rust, including multiple timestamps per line and `[offset:]` tags.
- User-Agent: `OpenMusix/<version> (+repo URL)`. Cache entries, including not-found results, live in app data with a TTL (7 days for not-found).

### Overlay (`/overlay`)
- Transparent background with no scrollbars. It connects to `ws://127.0.0.1:<port>/ws` and reconnects automatically.
- **Focus mode:** about 5–7 visible lines. The current line is large and full-opacity; the others shrink and fade with distance. Scrolling is smooth and centred.
- **Widget mode:** a compact card with album art, title, artist and 2–3 lines.
- **Mode switching:** a control-panel toggle plus a global hotkey (default `Ctrl+Alt+L`, rebindable), pushed live over WebSocket. `?mode=focus|widget` pins one mode, so a streamer can set up two sources in different scenes.
- **States:** for PlainOnly, show song info with "No synced lyrics" (static plain lyrics optional, my call: off by default). Instrumental shows "♪ Instrumental". Paused freezes and dims. Nothing playing fades out, and the overlay can optionally hide completely.
- **Styling** (per mode, saved, previewed live): font family (bundled set plus system fonts), size, text, highlight and dim colours, outline or shadow, alignment, line count, album art on/off, and background (transparent or rounded panel). Two or three built-in presets look good out of the box.

### Desktop app
- **Control panel:** now-playing card with source badge (Windows / Spotify), lyrics status, live overlay preview, mode toggle, offset slider, per-track nudge (±100 ms buttons), "Copy OBS URL" button with size hints, and a style editor.
- **Settings:** port (default 7878, auto-picks another if busy), hotkey, Connect/Disconnect Spotify with a setup guide, start with Windows, minimize to tray, clear cache, open lyrics override folder.
- **Floating lyrics window (my call):** optional, always on top, transparent and frameless, showing the overlay page. It lets a streamer who sings read along without looking at OBS.
- **System tray:** show/hide, switch mode, quit.

### Distribution
- Tauri bundler produces an NSIS installer. Builds publish to GitHub Releases. Code signing and auto-update are left to the builder for later; unsigned is acceptable for v1, with a SmartScreen note in the README.

## Decided by the user
- Both layouts, switchable (Focus and Widget).
- Shared with other streamers, so polish and zero setup matter.
- Approach C: Windows media session by default, plus optional Spotify connection.
- The overlay is reachable from the same PC only (`127.0.0.1`). Two-PC setups (OBS on a separate streaming PC) are out of scope for v1.

## My calls (correctable)
Tauri v2 + Rust + Svelte/TS; local port 7878; `Ctrl+Alt+L` hotkey; floating read-along window; plain-lyrics display off by default; 3 s duration tolerance; refresh token kept in Credential Manager; NSIS installer on GitHub Releases.

## Settled after builder review (my calls)
- **Port is fixed, not auto-switched.** The OBS URL and the Spotify redirect URI both depend on it. If 7878 is busy, show a clear error with a "change port" action. Changing the port in Settings shows a reminder to update the OBS source and the Spotify redirect URI. This replaces "auto-picks another if busy" above.
- **Unknown duration** (common for browsers): accept a lyrics match only when the normalized title and artist match exactly. Normalization also strips YouTube noise ("Official Video/Audio", "Lyrics", "VEVO"/"- Topic" in artist names, `[...]`/`(...)` tags). Otherwise show nothing.
- **Source conflict:** the PC wins. If the Windows session is actively playing, it leads, and Spotify only adds precision and details when it reports the same track. If the PC has nothing playing (or it is paused) and Spotify is playing on another device, use Spotify. Spotify polling slows to about every 10 s when its data isn't being used.
- **Sync target:** the highlighted line changes within about 300 ms of the vocal in the default (Windows-only) mode. If testing misses that, add a re-sync on each timeline event and a short Spotify-assisted correction.
- **Spotify Premium risk:** Spotify has tightened developer-mode access, and current terms may require Premium. Check this first during implementation. If it's required, keep Connect Spotify but label it "requires Spotify Premium" in the UI.

## Left to the builder
Exact visual design of presets, bundled font choices, tray icon art, internal module layout, test framework, CI setup, license file (to confirm with the user before release).

## Verification
- Unit tests: LRC parser, title normalization and duration-guard matching, timing extrapolation and drift correction, merger precedence.
- Manual end to end: play Spotify desktop with no login and confirm lyrics appear and stay in sync through seek, pause and skip. Connect Spotify and confirm the source badge switches and timing improves. Add `http://127.0.0.1:7878/overlay` as an OBS Browser Source and confirm transparency and live mode switching with the hotkey. Test a niche song (NotFound path), an instrumental, and YouTube Music in a browser (non-Spotify path).
- Installer: a clean install on a fresh Windows user account produces lyrics within a minute with no configuration.
