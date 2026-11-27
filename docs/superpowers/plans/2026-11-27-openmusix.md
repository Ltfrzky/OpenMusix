# OpenMusix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows desktop app that detects the playing song, fetches time-synced lyrics from LRCLIB, and serves them as a transparent OBS overlay with Focus and Widget layouts.

**Architecture:** A Tauri v2 Rust core reads playback from the Windows media session (GSMTC), and optionally from the Spotify Web API. It merges the two into one `NowPlaying`, resolves lyrics (local override, then disk cache, then LRCLIB) and publishes a single `Snapshot` through a `tokio::sync::watch` hub. The hub feeds two places: an axum server on `127.0.0.1:<port>` (overlay page over WebSocket) and the Svelte control panel (Tauri event). The frontend counts playback position forward locally and picks the current lyric line.

**Tech Stack:** Tauri 2, Rust (tokio, axum 0.8 with `ws`, reqwest + rustls, `windows` crate, `keyring` 3, `rust-embed` 8, `font-kit`), Svelte 5 + TypeScript + Vite (multi-page), Vitest, `cargo test` with `wiremock` + `tempfile`.

**Spec:** `docs/superpowers/specs/2026-11-27-openmusix-design.md`

## Global Constraints

- Windows 10/11 only. No cross-platform shims.
- Basic use requires no signup, key or login. Spotify is opt-in.
- Server binds `127.0.0.1` only. Default port `7878`. **The port is fixed and never auto-switched**: when it is busy, show an error with a "Change port" action.
- OBS URL: `http://127.0.0.1:<port>/overlay`, with an optional `?mode=focus|widget` pin. WebSocket: `ws://127.0.0.1:<port>/ws`.
- Spotify redirect URI: `http://127.0.0.1:<port>/spotify/callback` (never `localhost`). PKCE only, no client secret. Scopes: `user-read-currently-playing user-read-playback-state`.
- **Spotify requires Premium** (verified 2026-11-27: since Feb 2026, a development-mode app's owner must hold Premium, and each user owns their own Client ID). The UI labels Connect Spotify "requires Spotify Premium".
- LRCLIB User-Agent: `OpenMusix/<CARGO_PKG_VERSION> (+<CARGO_PKG_REPOSITORY>)`. LRCLIB `duration` is in **seconds**.
- Duration guard: reject a lyrics match whose duration differs by **more than 3 s** when the duration is known. When it is unknown, accept only an exact match of normalized title + artist. If nothing passes, show no lyrics.
- Not-found cache TTL: **7 days**. Found-result TTL: 30 days (builder's call).
- Timing: drift **> 400 ms snaps**; smaller drift is eased over **300 ms**. Global offset range **±2000 ms**, default 0. Per-track nudge is **±100 ms** per click, saved by track key.
- Default hotkey `Ctrl+Alt+L` (rebindable). Plain-lyrics display is off by default.
- Refresh token is stored in Windows Credential Manager: `keyring` service `OpenMusix`, user `spotify-refresh-token`.
- Source precedence: the PC wins. Spotify polling runs every 2 s when its data is used and every 10 s otherwise; it honours `Retry-After` on 429.
- Settings JSON lives in the Tauri `app_data_dir()`. Lyrics overrides live in `<app_data_dir>/lyrics/*.lrc`; the cache in `<app_data_dir>/cache/lyrics/`.
- No LICENSE file until the user confirms one. Unsigned NSIS installer for v1.

## Review Focus

1. **The track changes while a lyrics fetch is in flight.** The previous song's lyrics must never be shown for the new song. Pinned in Task 5 (`stale_result_is_discarded`).
2. **LRCLIB `/api/get` returns `instrumental: true` with null lyrics because the album name didn't match.** Fall through to `/api/search` before showing "Instrumental". Pinned in Task 4 (`instrumental_from_get_falls_through_to_search`).
3. **LRCLIB offline, timing out or returning 5xx.** Show nothing, and do *not* cache the failure as NotFound. Pinned in Task 4 (`server_error_is_not_not_found`) and Task 5 (`network_error_not_cached`).
4. **Several media sessions at once** (Spotify paused, a browser playing YouTube). Show the one that is playing. Pinned in Task 8 (`prefers_playing_session_over_paused_spotify`).
5. **A partial or corrupt `settings.json`** (older version, or a hand edit). The app still starts: missing fields are defaulted, and a corrupt file is backed up and replaced. Pinned in Task 6 (`missing_fields_default`, `corrupt_file_backed_up`).

## Inputs needed before release (not blockers for coding)

- The GitHub repository URL. Set it as `repository` in `src-tauri/Cargo.toml`; it feeds the User-Agent and the README.
- The license choice.

---

## File Map

```
package.json, vite.config.ts, svelte.config.js, tsconfig.json, vitest.config.ts
index.html                      control panel entry
overlay.html                    overlay entry (served by axum + used by floating window)
src/lib/types.ts                TS mirrors of Rust Snapshot/Settings/Style
src/lib/timing.ts               PlaybackClock (extrapolation + drift easing)
src/lib/lyricsIndex.ts          currentLineIndex()
src/lib/ws.ts                   reconnecting snapshot socket
src/lib/presets.ts              built-in style presets
src/overlay/main.ts, Overlay.svelte, FocusView.svelte, WidgetView.svelte, styleVars.ts
src/panel/main.ts, App.svelte, NowPlayingCard.svelte, ModeToggle.svelte, OffsetControls.svelte,
          ObsUrl.svelte, StyleEditor.svelte, SettingsView.svelte, SpotifySetup.svelte, PortBanner.svelte
src-tauri/src/main.rs, lib.rs   entry + setup/wiring
src-tauri/src/model.rs          Track, NowPlaying, Source, LyricLine, LyricsResult, Snapshot
src-tauri/src/normalize.rs      clean_title/clean_artist/match_key/track_key
src-tauri/src/lrc.rs            LRC parser
src-tauri/src/lyrics/{mod,lrclib,matching,cache,local}.rs
src-tauri/src/playback/{mod,merger,windows,spotify}.rs
src-tauri/src/spotify_auth.rs   PKCE + token store
src-tauri/src/settings.rs
src-tauri/src/hub.rs            watch<Snapshot> + art store
src-tauri/src/server.rs         axum routes
src-tauri/src/commands.rs       Tauri IPC
src-tauri/src/shell.rs          tray, hotkey, autostart, floating window
.github/workflows/ci.yml, release.yml; README.md
```

---

### Task 1: Project scaffold

**Files:**
- Create: everything at the root listed above as config, `index.html`, `overlay.html`, `src/panel/main.ts`, `src/overlay/main.ts`, `src-tauri/**` (from template), `.gitignore`
- Test: `src/lib/smoke.test.ts`, `src-tauri/src/lib.rs` (`#[cfg(test)] mod smoke`)

**Interfaces:**
- Produces: `npm run tauri dev`, `npm test` (Vitest), `cargo test --manifest-path src-tauri/Cargo.toml`; a Vite multi-page build that emits `dist/index.html` and `dist/overlay.html`.

- [ ] **Step 1:** `git init`. Scaffold with `npm create tauri-app@latest . -- --template svelte-ts --manager npm --identifier com.openmusix.app`. Set `productName` "OpenMusix" and the window title "OpenMusix" (1100×720, min 900×600).
- [ ] **Step 2:** Configure Vite `build.rollupOptions.input = { index: 'index.html', overlay: 'overlay.html' }`, with each HTML file mounting its own `main.ts`. Add `vitest` and the script `"test": "vitest run"`.
- [ ] **Step 3:** Add the Rust deps listed in Tech Stack, plus `tauri-plugin-single-instance`, `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`, `tauri-plugin-opener`, `serde`/`serde_json`, `thiserror`, `tracing`, `regex`, `unicode-normalization`, `sha2`, `base64`, `rand`, `url`. Dev deps: `tempfile`, `wiremock`, `tokio` (macros, rt-multi-thread). Register single-instance first in the builder (a second launch focuses the existing window, which also prevents a self-inflicted port conflict). Set `Cargo.toml` `repository = ""` with a `# TODO(user): repo URL` comment; this is the only allowed TODO.
- [ ] **Step 4:** Write the smoke tests (`expect(1+1).toBe(2)`; `assert_eq!(2, 1+1)`). Run `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml`. Expected: both PASS. Run `npm run tauri dev`. Expected: an empty "OpenMusix" window.
- [ ] **Step 5:** Commit `chore: scaffold Tauri v2 + Svelte app`.

---

### Task 2: Core model and title normalization

**Files:**
- Create: `src-tauri/src/model.rs`, `src-tauri/src/normalize.rs`
- Test: inline `#[cfg(test)]` in `normalize.rs`

**Interfaces:**
- Produces (all `serde` with `rename_all = "camelCase"`):
  - `enum Source { Windows, Spotify }`
  - `struct Track { title: String, artist: String, album: Option<String>, duration_ms: Option<u64>, art_url: Option<String> }`
  - `struct NowPlaying { track: Track, position_ms: u64, sampled_at: i64 /* unix ms */, is_playing: bool, source: Source }`
  - `struct LyricLine { time_ms: u64, text: String }`
  - `enum LyricsResult { Synced(Vec<LyricLine>), PlainOnly(String), Instrumental, NotFound }`
  - `fn clean_title(raw_title: &str, raw_artist: &str) -> String`: removes noise and a leading `"<artist> - "`, keeping case.
  - `fn clean_artist(raw: &str) -> String`: strips `VEVO` suffix and ` - Topic`, keeping case.
  - `fn match_key(s: &str) -> String`: NFKC, lowercase, curly→straight quotes, collapse whitespace, trim.
  - `fn track_key(t: &Track) -> String` = `format!("{}|{}", match_key(&clean_artist(a)), match_key(&clean_title(t, a)))`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn strips_remaster()  { assert_eq!(clean_title("Song - Remastered 2011", "A"), "Song"); }
#[test] fn strips_feat()      { assert_eq!(clean_title("Song (feat. X & Y)", "A"), "Song"); assert_eq!(clean_title("Song [ft. X]", "A"), "Song"); }
#[test] fn strips_live()      { assert_eq!(clean_title("Song - Live at Wembley", "A"), "Song"); }
#[test] fn strips_youtube()   { assert_eq!(clean_title("Artist - Song (Official Music Video)", "ArtistVEVO"), "Song");
                                assert_eq!(clean_title("Song [Lyrics]", "A"), "Song");
                                assert_eq!(clean_title("Song (Official Audio)", "A"), "Song"); }
#[test] fn keeps_real_parens_only_when_not_noise() { assert_eq!(clean_title("Song (Radio Edit) - 2004 Remaster", "A"), "Song"); }
#[test] fn artist_noise()     { assert_eq!(clean_artist("ArtistVEVO"), "Artist"); assert_eq!(clean_artist("Artist - Topic"), "Artist"); }
#[test] fn key_is_case_and_space_insensitive() { assert_eq!(match_key("  Don’t  Stop "), "don't stop"); }
#[test] fn unicode_survives() { assert_eq!(match_key("夜に駆ける"), "夜に駆ける"); }
```

Spec rule: all `(...)`/`[...]` tags and ` - <suffix>` segments are stripped (Remaster(ed), Live, Mono, Stereo, Edit, Version, Mix, Official…, Lyrics, Audio, Video, HD, 4K). A title that would become empty is returned unstripped.

- [ ] **Step 2:** Run `cargo test normalize`. Expected: FAIL (unresolved items).
- [ ] **Step 3:** Implement with `regex` (compiled once via `std::sync::LazyLock`) and `unicode-normalization`.
- [ ] **Step 4:** Run `cargo test normalize`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): track model and title normalization`.

---

### Task 3: LRC parser

**Files:**
- Create: `src-tauri/src/lrc.rs`

**Interfaces:**
- Consumes: `LyricLine`
- Produces: `fn parse_lrc(src: &str) -> Vec<LyricLine>`, sorted ascending by `time_ms`. Empty-text lines are kept as `""` (instrumental breaks). Metadata tags are dropped. Returns empty when there are no timestamps.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn basic() { let l = parse_lrc("[00:12.34]Hello\n[01:02.5]World");
  assert_eq!(l, vec![LyricLine{time_ms:12340,text:"Hello".into()}, LyricLine{time_ms:62500,text:"World".into()}]); }
#[test] fn three_digit_ms_and_no_fraction() { assert_eq!(parse_lrc("[00:01.234]a\n[00:02]b")[0].time_ms, 1234); assert_eq!(parse_lrc("[00:02]b")[0].time_ms, 2000); }
#[test] fn multi_timestamps() { let l = parse_lrc("[00:10.00][00:30.00]Chorus\n[00:20.00]Verse");
  assert_eq!(l.iter().map(|x| x.time_ms).collect::<Vec<_>>(), vec![10000,20000,30000]); assert_eq!(l[2].text, "Chorus"); }
#[test] fn offset_tag_shifts_earlier() { let l = parse_lrc("[offset:+500]\n[00:10.00]x"); assert_eq!(l[0].time_ms, 9500); }
#[test] fn negative_offset_and_clamp() { assert_eq!(parse_lrc("[offset:-250]\n[00:01.00]x")[0].time_ms, 1250);
  assert_eq!(parse_lrc("[offset:+5000]\n[00:01.00]x")[0].time_ms, 0); }
#[test] fn metadata_ignored_blank_kept() { let l = parse_lrc("[ar:Someone]\n[ti:T]\n[00:05.00]\n[00:06.00]Go");
  assert_eq!(l.len(), 2); assert_eq!(l[0].text, ""); }
#[test] fn crlf_and_bom() { assert_eq!(parse_lrc("\u{feff}[00:01.00]a\r\n[00:02.00]b\r\n").len(), 2); }
```

- [ ] **Step 2:** Run `cargo test lrc`. Expected: FAIL.
- [ ] **Step 3:** Implement. A positive `[offset:]` means lyrics appear earlier (`t - offset`), saturating at 0.
- [ ] **Step 4:** Run `cargo test lrc`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(lyrics): LRC parser`.

---

### Task 4: LRCLIB client and wrong-lyrics guard

**Files:**
- Create: `src-tauri/src/lyrics/matching.rs`, `src-tauri/src/lyrics/lrclib.rs`, `src-tauri/src/lyrics/mod.rs` (module decls only for now)
- Test: inline in both files; `lrclib.rs` tests use `wiremock`

**Interfaces:**
- Consumes: `Track`, `clean_title`, `clean_artist`, `match_key`, `parse_lrc`, `LyricsResult`
- Produces:
  - `struct LrclibRecord { id: i64, track_name: String, artist_name: String, album_name: Option<String>, duration: f64 /* seconds */, instrumental: bool, plain_lyrics: Option<String>, synced_lyrics: Option<String> }` (serde camelCase)
  - `fn passes_guard(q: &Track, r: &LrclibRecord, from_search: bool) -> bool`
  - `fn to_result(r: &LrclibRecord) -> LyricsResult`: synced if non-empty `synced_lyrics`, else `PlainOnly`, else `Instrumental` when `instrumental`, else `NotFound`.
  - `struct LrclibClient { base: Url, http: reqwest::Client }`; `LrclibClient::new(base: Url) -> Self` (sets the User-Agent from Global Constraints, 8 s timeout)
  - `async fn LrclibClient::lookup(&self, q: &Track) -> Result<LyricsResult, LyricsError>`
  - `enum LyricsError { Network(String) }`: any transport error, timeout or 5xx. A 404 is not an error.

Guard rules (`passes_guard`):
- Duration known: `|r.duration*1000 - q.duration_ms| <= 3000`. When `from_search`, also require `match_key(clean_title(r))` == `match_key(clean_title(q))` and that one cleaned artist key contains the other.
- Duration unknown: require exact equality of both cleaned title key and cleaned artist key (for get and search alike).

Lookup algorithm:
1. `GET /api/get?track_name&artist_name&album_name&duration` using cleaned title and artist, album only if present, and duration in whole seconds only if known. (Without a duration, skip `/api/get` and go to step 2.)
2. If step 1 gave a record that passes the guard and whose `to_result` is `Synced` or `PlainOnly`, return it.
3. Otherwise `GET /api/search?track_name&artist_name` and keep records that pass the guard (`from_search = true`). Prefer synced, then plain, then the smallest duration delta. Return the first.
4. If no record with lyrics passed but a passing record (from either step) is `instrumental`, return `Instrumental`. Otherwise `NotFound`.

- [ ] **Step 1: Write the failing tests**

matching.rs:
```rust
#[test] fn rejects_4s_off()  { assert!(!passes_guard(&trk("Song","A",Some(200_000)), &rec("Song","A",204.5), false)); }
#[test] fn accepts_3s_off()  { assert!( passes_guard(&trk("Song","A",Some(200_000)), &rec("Song","A",203.0), false)); }
#[test] fn unknown_duration_needs_exact_names() {
  assert!( passes_guard(&trk("Song (Official Video)","ArtistVEVO",None), &rec("Song","Artist",999.0), false));
  assert!(!passes_guard(&trk("Song","Artist",None), &rec("Song 2","Artist",200.0), false)); }
#[test] fn search_requires_title_match() { assert!(!passes_guard(&trk("Song","A",Some(200_000)), &rec("Other","A",200.0), true)); }
```
lrclib.rs (wiremock server, `LrclibClient::new(server.uri())`):
```rust
#[tokio::test] async fn get_hit_returns_synced()                 // /api/get 200 with syncedLyrics → Synced(len>0)
#[tokio::test] async fn get_404_then_search_hit()                // /api/get 404, /api/search [rec] → Synced
#[tokio::test] async fn instrumental_from_get_falls_through_to_search() // get: instrumental:true, nulls; search: synced rec → Synced
#[tokio::test] async fn wrong_duration_everywhere_is_not_found() // get & search return 230s for a 200s query → NotFound
#[tokio::test] async fn server_error_is_not_not_found()          // get 503 → Err(LyricsError::Network(_))
#[tokio::test] async fn sends_user_agent_and_seconds()           // matcher: header user-agent starts "OpenMusix/", query duration=200
```

- [ ] **Step 2:** Run `cargo test lyrics::`. Expected: FAIL.
- [ ] **Step 3:** Implement per the rules above.
- [ ] **Step 4:** Run `cargo test lyrics::`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(lyrics): LRCLIB client with duration guard`.

---

### Task 5: Lyrics cache, local overrides and LyricsService

**Files:**
- Create: `src-tauri/src/lyrics/cache.rs`, `src-tauri/src/lyrics/local.rs`
- Modify: `src-tauri/src/lyrics/mod.rs`

**Interfaces:**
- Consumes: `LrclibClient::lookup`, `parse_lrc`, `track_key`, `LyricsResult`, `LyricsError`
- Produces:
  - `struct LyricsCache { dir: PathBuf }`: `get(&self, key: &str, now: SystemTime) -> Option<LyricsResult>`, `put(&self, key: &str, r: &LyricsResult, now: SystemTime)`, `clear(&self) -> io::Result<()>`. One file per key, named `sha256(key)` hex + `.json`, holding `{storedAt, result}`. TTL: NotFound 7 d, everything else 30 d.
  - `fn find_local(dir: &Path, t: &Track) -> Option<LyricsResult>`: scans `*.lrc`, splits the file stem on the first `" - "` into artist and title, and compares `match_key(clean_…)` of both parts. A file that parses to zero timed lines is `PlainOnly(contents)`.
  - `struct LyricsService { local_dir, cache, client }` with `async fn resolve(&self, t: &Track) -> Result<LyricsResult, LyricsError>`, order local → cache → client. Results are cached on `Ok` only.
  - `enum LyricsStatus { Loading, Ready(LyricsResult), Error }` (serde tag `"status"`) is what the hub publishes. `Error` renders like NotFound.
  - `async fn LyricsService::resolve_for(&self, t: Track, current_key: impl Fn() -> String) -> Option<LyricsStatus>`: returns `None` when `track_key(&t) != current_key()` after resolving (the stale-result guard).

- [ ] **Step 1: Write the failing tests** (`tempfile::tempdir()`)

```rust
#[test] fn not_found_expires_after_7_days()  // put NotFound at t0; get at t0+6d → Some; t0+7d+1s → None
#[test] fn synced_survives_7_days()          // put Synced at t0; get at t0+8d → Some
#[test] fn local_override_matches_loosely()  // file "Artist - Song.lrc"; Track{title:"Song - Remastered", artist:"artist"} → Synced
#[test] fn local_override_wins_over_cache()  // both present → local content returned
#[tokio::test] async fn network_error_not_cached()   // wiremock 503 → Err; cache dir stays empty
#[tokio::test] async fn stale_result_is_discarded()  // resolve_for(track A, || "b|other".into()) → None
```

- [ ] **Step 2:** Run `cargo test lyrics::`. Expected: new tests FAIL.
- [ ] **Step 3:** Implement. Cache writes go to a temp file and are then renamed (no torn files). An unreadable cache file is treated as a miss.
- [ ] **Step 4:** Run `cargo test lyrics::`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(lyrics): cache, local overrides, service`.

---

### Task 6: Settings store

**Files:**
- Create: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces (serde camelCase, `#[serde(default)]` on every struct):
  - `enum Mode { Focus, Widget }`; `enum TextEffect { None, Outline, Shadow }`; `enum Align { Left, Center, Right }`; `enum Background { Transparent, Panel }`
  - `struct Style { preset: String, font_family: String, font_size_px: u16, text_color: String, highlight_color: String, dim_color: String, effect: TextEffect, align: Align, line_count: u8, show_art: bool, background: Background }`
  - `struct Styles { focus: Style, widget: Style }`
  - `struct Settings { port: u16, hotkey: String, mode: Mode, global_offset_ms: i32, track_offsets_ms: HashMap<String, i32>, styles: Styles, show_plain_lyrics: bool, hide_when_idle: bool, start_with_windows: bool, minimize_to_tray: bool, floating_window: bool, spotify_client_id: Option<String> }`
  - Defaults: port 7878, hotkey `"Ctrl+Alt+L"`, mode Focus, offset 0, show_plain false, hide_when_idle false, start_with_windows false, minimize_to_tray true, floating false. Focus style = preset `"spotlight"` (font "Outfit", 44 px, center, 7 lines, shadow, transparent, art off). Widget style = preset `"glass"` (font "Inter", 22 px, left, 3 lines, no effect, panel, art on). These must equal `src/lib/presets.ts` (Task 11).
  - `struct SettingsStore { path: PathBuf, current: RwLock<Settings> }`: `load(path) -> Self`, `get() -> Settings`, `update(f: impl FnOnce(&mut Settings)) -> io::Result<Settings>` (clamps `global_offset_ms` to ±2000, `line_count` to 1..=9, `font_size_px` to 10..=160, then saves atomically).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn missing_file_gives_defaults()  // load(nonexistent).get().port == 7878
#[test] fn missing_fields_default()       // file {"port":9000} → port 9000, hotkey "Ctrl+Alt+L", styles.focus.line_count 7
#[test] fn corrupt_file_backed_up()       // file "{not json" → defaults; "settings.json.bak" exists with the old bytes
#[test] fn offset_clamped()               // update(|s| s.global_offset_ms = 5000) → 2000
#[test] fn roundtrip()                    // update then load again → equal
```

- [ ] **Step 2:** Run `cargo test settings`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test settings`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): settings store`.

---

### Task 7: Playback merger and state hub

**Files:**
- Create: `src-tauri/src/playback/mod.rs`, `src-tauri/src/playback/merger.rs`, `src-tauri/src/hub.rs`

**Interfaces:**
- Consumes: `Track`, `NowPlaying`, `Source`, `track_key`, `Settings`, `LyricsStatus`
- Produces:
  - `struct SourceSnapshot { track: Track, position_ms: u64, sampled_at: i64, is_playing: bool }`
  - `enum PlaybackEvent { Windows(Option<SourceSnapshot>), Spotify(Option<SourceSnapshot>) }`. Sources send these on `tokio::sync::mpsc::Sender<PlaybackEvent>`.
  - `struct Merged { now: Option<NowPlaying>, spotify_used: bool }`
  - `fn merge(win: Option<&SourceSnapshot>, sp: Option<&SourceSnapshot>) -> Merged`
  - `struct Snapshot { now_playing: Option<NowPlaying>, lyrics: LyricsStatus, track_key: Option<String>, mode: Mode, styles: Styles, global_offset_ms: i32, track_offset_ms: i32, show_plain_lyrics: bool, hide_when_idle: bool }` (serde camelCase)
  - `struct Hub { tx: watch::Sender<Snapshot>, art: RwLock<Option<Art>> }`, where `struct Art { key: String, mime: String, bytes: Vec<u8> }`; `Hub::subscribe() -> watch::Receiver<Snapshot>`, `Hub::modify(f: impl FnOnce(&mut Snapshot))` (sends only if changed), `Hub::set_art(Option<Art>)`, `Hub::art(key) -> Option<Art>`.

Merge rules (spec "Source conflict"):
1. `win` is playing → base = win, `source = Windows`. If `sp` is playing and `track_key` is equal, take Spotify's position, sampled_at, duration and art_url, set `source = Spotify` and `spotify_used = true`.
2. Otherwise, if `sp` is playing → Spotify, `spotify_used = true`.
3. Otherwise `win` paused → win. Otherwise `sp` paused → sp. Otherwise None.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn windows_only()                 // merge(Some(win playing), None) → source Windows, spotify_used false
#[test] fn spotify_refines_same_track()   // same key, sp pos 61000 → position 61000, source Spotify, duration from sp
#[test] fn pc_wins_on_different_track()   // win playing "A", sp playing "B" → title "A", source Windows, spotify_used false
#[test] fn spotify_when_pc_paused()       // win paused "A", sp playing "B" → "B", Spotify
#[test] fn nothing()                      // (None, None) → now None
#[test] fn hub_ignores_identical_modify() // subscribe; modify with no change → has_changed() false
```

- [ ] **Step 2:** Run `cargo test playback hub`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(core): playback merger and snapshot hub`.

---

### Task 8: Windows media session source

**Files:**
- Create: `src-tauri/src/playback/windows.rs`

**Interfaces:**
- Consumes: `SourceSnapshot`, `PlaybackEvent`, `Hub::set_art`, `Art`
- Produces:
  - `fn ticks_to_unix_ms(universal_time: i64) -> i64` = `(t - 116_444_736_000_000_000) / 10_000`
  - `fn timespan_to_ms(duration_100ns: i64) -> u64`
  - `struct SessionInfo { app_id: String, is_playing: bool }`; `fn pick_session(sessions: &[SessionInfo], current: Option<usize>) -> Option<usize>`: Spotify (case-insensitive `"spotify"` in `app_id`) and playing → any playing → Spotify → `current` → first.
  - `fn spawn_windows_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>)`: a dedicated thread.

Behaviour: get `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()`. Subscribe to `SessionsChanged` and `CurrentSessionChanged`, and on the chosen session to `MediaPropertiesChanged`, `PlaybackInfoChanged` and `TimelinePropertiesChanged`. Each handler pushes `()` into a std channel; the thread debounces 50 ms, re-reads, and sends `PlaybackEvent::Windows(..)`. When the chosen session changes, it re-subscribes (removing old tokens). Snapshot: `position_ms = timespan_to_ms(Position)`, `sampled_at = ticks_to_unix_ms(LastUpdatedTime)` (if 0, use now), `duration_ms = EndTime - StartTime` when > 0, otherwise `None`. Thumbnail: read the stream to bytes. Art key = first 16 hex chars of sha256(bytes), `art_url = "/art/<key>"`, stored via `hub.set_art`. Empty title → `None`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn epoch_conversion() { assert_eq!(ticks_to_unix_ms(116_444_736_000_000_000), 0); assert_eq!(ticks_to_unix_ms(116_444_736_000_000_000 + 10_000), 1); }
#[test] fn timespan() { assert_eq!(timespan_to_ms(12_340_000), 1234); }
#[test] fn prefers_playing_session_over_paused_spotify() {
  let s = [si("Spotify.exe", false), si("chrome", true)]; assert_eq!(pick_session(&s, Some(0)), Some(1)); }
#[test] fn prefers_playing_spotify() { let s = [si("chrome", true), si("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", true)]; assert_eq!(pick_session(&s, None), Some(1)); }
#[test] fn falls_back_to_current() { let s = [si("a", false), si("b", false)]; assert_eq!(pick_session(&s, Some(1)), Some(1)); }
```

- [ ] **Step 2:** Run `cargo test playback::windows`. Expected: FAIL.
- [ ] **Step 3:** Implement. `windows` crate features: `Media_Control`, `Foundation`, `Foundation_Collections`, `Storage_Streams`.
- [ ] **Step 4:** Run the tests. Expected: PASS. Manual: a temporary `cargo run --example gsmtc_probe` that prints events while Spotify plays, seeks and pauses. Expected: a line per change. Delete the example before committing.
- [ ] **Step 5:** Commit `feat(playback): Windows media session source`.

---

### Task 9: Local HTTP + WebSocket server

**Files:**
- Create: `src-tauri/src/server.rs`

**Interfaces:**
- Consumes: `Hub::subscribe`, `Hub::art`, `Snapshot`
- Produces:
  - `#[derive(RustEmbed)] #[folder = "../dist"] struct Assets;` (in debug builds rust-embed reads from disk, so `vite build --watch` updates live)
  - `enum ServerStatus { Running { port: u16 }, PortInUse { port: u16 }, Failed { port: u16, message: String } }` (serde tag `"state"`)
  - `fn router(hub: Arc<Hub>, oauth: Arc<dyn OAuthCallback>) -> axum::Router`
  - `async fn start_server(port: u16, hub: Arc<Hub>, oauth: Arc<dyn OAuthCallback>) -> ServerStatus`: binds `127.0.0.1:port` and spawns serve. `AddrInUse` → `PortInUse`.
  - `trait OAuthCallback: Send + Sync { fn handle(&self, code: Option<String>, state: Option<String>, error: Option<String>) -> String /* html */; }` (Task 14 implements it; until then a stub that returns "Spotify not configured").

Routes: `GET /overlay` → `overlay.html`; `GET /assets/*` → embedded assets with the correct mime; `GET /ws` → on upgrade, send the current snapshot as JSON `{"type":"snapshot","data":<Snapshot>}`, then one message per `changed()`; `GET /art/{key}` → bytes or 404, `Cache-Control: max-age=86400`; `GET /spotify/callback` → `oauth.handle(..)` HTML; `GET /` → redirect to `/overlay`. Responses carry no CORS headers (same-origin only).

- [ ] **Step 1: Write the failing tests** (`tokio::test`, bind port 0 via a variant `start_on(listener)`, `tokio-tungstenite` dev dep)

```rust
async fn ws_sends_initial_snapshot()  // connect → first text msg parses, type == "snapshot"
async fn ws_pushes_on_change()        // hub.modify(mode=Widget) → next msg data.mode == "widget"
async fn art_404_then_200()           // /art/x 404; set_art(key "x") → 200 with bytes
async fn port_in_use_reported()       // occupy a port with a TcpListener → start_server returns PortInUse
async fn overlay_served()             // /overlay 200, contains "<div id=\"app\""   (requires `npm run build` first; test notes this)
```

- [ ] **Step 2:** Run `npm run build` and then `cargo test server`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(server): local overlay HTTP and WebSocket server`.

---

### Task 10: Frontend timing core

**Files:**
- Create: `src/lib/types.ts`, `src/lib/timing.ts`, `src/lib/lyricsIndex.ts`, `src/lib/ws.ts`
- Test: `src/lib/timing.test.ts`, `src/lib/lyricsIndex.test.ts`

**Interfaces:**
- Consumes: the JSON shapes of `Snapshot`, `NowPlaying`, `LyricsStatus` and `Style` (Tasks 2/5/6/7). `types.ts` mirrors them exactly in camelCase.
- Produces:
  - `class PlaybackClock { update(s: { positionMs: number; sampledAt: number; isPlaying: boolean; trackKey: string | null }, now: number): void; positionAt(now: number): number }`
  - `function currentLineIndex(lines: { timeMs: number }[], posMs: number): number` (−1 before the first line; binary search)
  - `function effectivePosition(clock: PlaybackClock, now: number, globalOffsetMs: number, trackOffsetMs: number): number` = `positionAt(now) + global + track`
  - `function connectSnapshots(url: string, onSnapshot: (s: Snapshot) => void, onStatus?: (connected: boolean) => void): () => void`: reconnects with backoff of 500 ms, doubling, capped at 5 s, reset on success. It returns a dispose function.

Clock rules: target(now) = `positionMs + (isPlaying ? now - sampledAt : 0)`. On `update`: if the trackKey changed or this is the first sample → snap. Otherwise `drift = target(now) - previousDisplay(now)`. If `|drift| > 400` → snap. Otherwise store `correction = -drift` at `t0 = now`. `positionAt(now) = target(now) + correction * max(0, 1 - (now - t0)/300)`. Paused → constant.

- [ ] **Step 1: Write the failing tests**

```ts
test('extrapolates while playing', () => { const c = new PlaybackClock(); c.update({positionMs:1000,sampledAt:0,isPlaying:true,trackKey:'a'},0); expect(c.positionAt(250)).toBe(1250); });
test('frozen while paused', () => { const c = new PlaybackClock(); c.update({positionMs:1000,sampledAt:0,isPlaying:false,trackKey:'a'},0); expect(c.positionAt(5000)).toBe(1000); });
test('snaps on drift > 400', () => { const c = clockAt(1000); c.update({positionMs:1450,sampledAt:0,isPlaying:true,trackKey:'a'},0); expect(c.positionAt(0)).toBe(1450); });
test('eases drift <= 400 over 300ms', () => { const c = clockAt(1000); c.update({positionMs:1200,sampledAt:0,isPlaying:true,trackKey:'a'},0);
  expect(c.positionAt(0)).toBe(1000); expect(c.positionAt(150)).toBe(1250); expect(c.positionAt(300)).toBe(1500); });
test('seek backwards snaps', () => { const c = clockAt(60000); c.update({positionMs:5000,sampledAt:0,isPlaying:true,trackKey:'a'},0); expect(c.positionAt(0)).toBe(5000); });
test('track change snaps', () => { const c = clockAt(1000); c.update({positionMs:1100,sampledAt:0,isPlaying:true,trackKey:'b'},0); expect(c.positionAt(0)).toBe(1100); });
test('line index', () => { const L=[{timeMs:1000},{timeMs:2000},{timeMs:3000}];
  expect(currentLineIndex(L,999)).toBe(-1); expect(currentLineIndex(L,1000)).toBe(0); expect(currentLineIndex(L,2999)).toBe(1); expect(currentLineIndex(L,99999)).toBe(2); expect(currentLineIndex([],5)).toBe(-1); });
```
(`clockAt(p)` = a fresh clock with `update({positionMs:p,sampledAt:0,isPlaying:true,trackKey:'a'},0)`.)

- [ ] **Step 2:** Run `npm test`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `npm test`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(ui): playback clock and line index`.

---

### Task 11: Overlay page (Focus + Widget)

**Files:**
- Create: `src/overlay/Overlay.svelte`, `FocusView.svelte`, `WidgetView.svelte`, `styleVars.ts`, `src/lib/presets.ts`; modify `src/overlay/main.ts`, `overlay.html`
- Test: `src/overlay/styleVars.test.ts`, `src/overlay/Overlay.test.ts` (`@testing-library/svelte` + jsdom)

**Interfaces:**
- Consumes: `connectSnapshots`, `PlaybackClock`, `currentLineIndex`, `effectivePosition`, `Snapshot`, `Style`
- Produces:
  - `Overlay.svelte` props: `{ snapshot: Snapshot | null; pinnedMode?: 'focus' | 'widget'; preview?: boolean }`. `main.ts` reads `?mode=` into `pinnedMode` and `?float=1` (adds `data-tauri-drag-region` on the root). It connects to `ws://${location.host}/ws` when served over HTTP. For the floating window (`tauri:` origin) it connects to the port passed as `?port=`.
  - `function styleVars(s: Style): Record<string, string>`: CSS custom properties `--om-font`, `--om-size`, `--om-text`, `--om-hl`, `--om-dim`, `--om-align`, `--om-shadow`.
  - `presets.ts`: `export const PRESETS: Record<'spotlight'|'glass'|'neon', Style>`. `spotlight` and `glass` equal the Rust defaults in Task 6. `neon` = Outfit 40 px, highlight `#7CF7D4`, outline.
  - Bundled fonts via `@fontsource-variable/inter`, `@fontsource-variable/outfit`, `@fontsource-variable/lora`, imported in `overlay/main.ts` (and the panel, for preview).

Behaviour: `html, body` have a transparent background and `overflow: hidden`. One rAF loop computes the effective position and line index. **Focus:** shows `lineCount` lines centred on the current one via `transform: translateY` with a 350 ms ease-out transition. Scale and opacity fall off with distance (current 1.0/1.0, ±1 0.82/0.55, ±2 0.7/0.35, further 0.6/0.2). An empty-text line renders as `♪`. **Widget:** a card with art (when `showArt`), title, artist and `lineCount` lines starting at the current one. States: `Loading` → song info only. `PlainOnly` → song info + "No synced lyrics" (+ plain text when `showPlainLyrics`). `Instrumental` → "♪ Instrumental". `NotFound`/`Error` → song info only. Paused → freeze + `opacity: .45` over 400 ms. Nothing playing → fade out over 600 ms; with `hideWhenIdle`, render nothing.

- [ ] **Step 1: Write the failing tests**

```ts
test('styleVars maps colours', () => expect(styleVars(PRESETS.spotlight)['--om-hl']).toBe(PRESETS.spotlight.highlightColor));
test('pinned mode overrides snapshot mode', ...)   // snapshot.mode 'focus', pinnedMode 'widget' → .om-widget present
test('instrumental label', ...)                    // lyrics Ready(Instrumental) → text "♪ Instrumental"
test('plain hidden by default', ...)               // PlainOnly + showPlainLyrics false → "No synced lyrics", plain text absent
test('idle hidden when hideWhenIdle', ...)         // nowPlaying null + hideWhenIdle → container empty
```

- [ ] **Step 2:** Run `npm test`. Expected: FAIL.
- [ ] **Step 3:** Implement. Use the `make-interfaces-feel-better` skill for the motion and typography polish (anti-goal: amateur-looking defaults).
- [ ] **Step 4:** Run `npm test`. Expected: PASS. Run `npm run build`. Expected: `dist/overlay.html` emitted.
- [ ] **Step 5:** Commit `feat(overlay): focus and widget layouts`.

---

### Task 12: App wiring: pipeline, IPC, tray, hotkey, floating window

**Files:**
- Create: `src-tauri/src/commands.rs`, `src-tauri/src/shell.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: everything from Tasks 2–9
- Produces:
  - `struct AppCtx { settings: SettingsStore, hub: Arc<Hub>, lyrics: Arc<LyricsService>, server: RwLock<ServerStatus>, playback_tx: mpsc::Sender<PlaybackEvent> }`, managed by Tauri.
  - Tauri event `"snapshot"` (payload `Snapshot`), emitted on every hub change; event `"server-status"` (`ServerStatus`).
  - Commands: `get_snapshot() -> Snapshot`, `get_settings() -> Settings`, `update_settings(settings: Settings) -> Result<Settings, String>`, `set_mode(mode: Mode)`, `toggle_mode()`, `nudge_track_offset(delta_ms: i32) -> i32`, `reset_track_offset()`, `get_server_status() -> ServerStatus`, `restart_server()`, `clear_lyrics_cache()`, `open_lyrics_folder()`, `list_system_fonts() -> Vec<String>` (font-kit, deduped, sorted, cached), `set_floating_window(on: bool)`.
  - `shell::setup_tray(app)`: menu with Show/Hide, Switch mode, Quit. Left click toggles the window.
  - `shell::register_hotkey(app, accel: &str) -> Result<(), String>`: unregisters the previous one. An invalid or taken accelerator returns `Err` and the settings UI shows it.
  - `shell::floating_window(app, on: bool, port: u16)`: label `"float"`, `WebviewUrl::App("overlay.html?float=1&port=<port>")`, transparent, no decorations, always on top, skip taskbar, 520×260.

Pipeline (spawned in `setup`):
1. A playback task keeps the latest `win`/`sp` snapshots. On each event: `merge` → if `track_key` changed, set `lyrics = Loading` and spawn `lyrics.resolve_for(track, current_key)`, then put `Ready(r)` / `Error` into the hub only if the result is still current. Always update `now_playing` and `track_offset_ms` from `settings.track_offsets_ms[key]`. Report `spotify_used` to the Spotify poller (Task 14) through a `watch<bool>`.
2. Settings changes re-project `mode`, `styles`, offsets and flags into the hub. Changing the port restarts the server and emits `server-status`. Changing the hotkey re-registers it. `start_with_windows` toggles autostart.
3. Close-to-tray when `minimize_to_tray`.

CSP in `tauri.conf.json`: `default-src 'self'; img-src 'self' data: http://127.0.0.1:* https://i.scdn.co; connect-src 'self' ipc: http://ipc.localhost ws://127.0.0.1:*; style-src 'self' 'unsafe-inline'; font-src 'self' data:`. Capabilities: core defaults, `global-shortcut:allow-*`, `autostart:allow-*`, `opener:allow-open-url`, `opener:allow-open-path`. The window labels `main` and `float` share them.

- [ ] **Step 1: Write the failing test** (`commands.rs`, pure helper)

```rust
#[test] fn nudge_accumulates_and_clamps() // apply_nudge(map, "k", +100) twice → 200; from 1950 +100 → 2000 (clamp ±2000)
#[test] fn zero_offset_removes_key()      // apply_nudge to 0 → key absent (keeps settings.json small)
```
`fn apply_nudge(map: &mut HashMap<String,i32>, key: &str, delta: i32) -> i32`

- [ ] **Step 2:** Run `cargo test commands`. Expected: FAIL.
- [ ] **Step 3:** Implement `apply_nudge`, the commands, the pipeline and the shell.
- [ ] **Step 4:** Run `cargo test`. Expected: all PASS. Manual: `npm run tauri dev` while Spotify plays. Opening `http://127.0.0.1:7878/overlay` in Chrome shows synced lyrics; pressing `Ctrl+Alt+L` switches the layout within 1 s; the tray "Quit" exits.
- [ ] **Step 5:** Commit `feat(app): pipeline, IPC, tray, hotkey, floating window`.

---

### Task 13: Control panel UI

**Files:**
- Create: `src/panel/App.svelte`, `NowPlayingCard.svelte`, `ModeToggle.svelte`, `OffsetControls.svelte`, `ObsUrl.svelte`, `StyleEditor.svelte`, `SettingsView.svelte`, `PortBanner.svelte`; modify `src/panel/main.ts`
- Test: `src/panel/ObsUrl.test.ts`, `src/panel/OffsetControls.test.ts`

**Interfaces:**
- Consumes: the Task 12 commands and events, `Overlay.svelte` (preview, rendered directly with the panel's snapshot over a checkerboard backdrop), `PRESETS`
- Produces: `function obsUrl(port: number, mode?: 'focus'|'widget'): string` in `ObsUrl.svelte`'s module script.

Layout: two tabs, **Live** and **Settings**.
- **Live:** NowPlayingCard (art, title, artist, source badge `Windows`/`Spotify`, lyrics status chip: Synced / Plain only / Instrumental / Not found / Searching…). Live preview (aspect 16:9, mode follows the toggle). ModeToggle (Focus | Widget, showing the hotkey hint). OffsetControls (global slider −2000…+2000 step 50 with a ms readout; per-track `−100` / `+100` / reset showing the current track offset, disabled when no track). ObsUrl: three copy buttons (auto / focus-pinned / widget-pinned) with size hints "Focus: 1920×1080 · Widget: 600×200", using `navigator.clipboard.writeText` and a "Copied" toast. StyleEditor for the active mode: preset picker, font (bundled first, then `list_system_fonts`), size, three colours, effect, alignment, line count, art toggle, background, all applied live via `update_settings` (debounced 150 ms).
- **Settings:** port (number input + Apply, with a reminder: "Update your OBS Browser Source URL and Spotify redirect URI after changing the port"). Hotkey (record-a-shortcut input, showing the error from `register_hotkey`). Start with Windows, minimize to tray, floating read-along window, show plain lyrics, hide overlay when idle. Clear cache. Open lyrics folder (with the hint "Name files `Artist - Title.lrc`"). The Spotify section is Task 14.
- **PortBanner:** shown on `PortInUse`/`Failed`: "Port 7878 is in use by another program. Overlay is offline." with a [Change port] button that jumps to Settings → port.

- [ ] **Step 1: Write the failing tests**

```ts
test('obs url', () => { expect(obsUrl(7878)).toBe('http://127.0.0.1:7878/overlay'); expect(obsUrl(9000,'widget')).toBe('http://127.0.0.1:9000/overlay?mode=widget'); });
test('nudge buttons call command with ±100', ...)   // mock invoke; click "+100" → invoke('nudge_track_offset', {deltaMs: 100})
```

- [ ] **Step 2:** Run `npm test`. Expected: FAIL.
- [ ] **Step 3:** Implement. Use the `interface-design` skill for panel craft (dark theme, clear hierarchy).
- [ ] **Step 4:** Run `npm test`. Expected: PASS. Manual: every control changes the open browser overlay live; the port banner appears when `python -m http.server 7878` is started before the app.
- [ ] **Step 5:** Commit `feat(panel): control panel and settings`.

---

### Task 14: Optional Spotify connection

**Files:**
- Create: `src-tauri/src/spotify_auth.rs`, `src-tauri/src/playback/spotify.rs`, `src/panel/SpotifySetup.svelte`
- Modify: `src-tauri/src/lib.rs` (replace the OAuth stub, spawn the poller), `src-tauri/src/commands.rs`, `src/panel/SettingsView.svelte`, `NowPlayingCard.svelte`

**Interfaces:**
- Consumes: `OAuthCallback`, `PlaybackEvent::Spotify`, `SourceSnapshot`, `SettingsStore`, the `spotify_used` `watch<bool>`
- Produces:
  - `fn pkce_challenge(verifier: &str) -> String` (base64url-nopad of sha256); `fn new_verifier() -> String` (64 chars of `[A-Za-z0-9-._~]`)
  - `fn authorize_url(client_id: &str, port: u16, challenge: &str, state: &str) -> Url`
  - `struct SpotifyAuth`: `begin(client_id, port) -> Url` (stores verifier + state, 10 min expiry), implements `OAuthCallback` (checks state, exchanges code at `https://accounts.spotify.com/api/token`, saves the refresh token to keyring, returns a "Connected, you can close this tab" HTML page), `access_token() -> Result<String, AuthError>` (cached, refreshes 60 s before expiry, persists a rotated refresh token), `disconnect()` (deletes the keyring entry), `is_connected() -> bool`.
  - `fn parse_currently_playing(status: u16, body: &str, sampled_at: i64) -> Option<SourceSnapshot>`: 204 → None; `item.type != "track"` (ads, episodes) → None; artist = the item's artists' names joined with ", "; art = the largest `album.images[0].url`.
  - `fn next_poll_delay(spotify_used: bool, retry_after_s: Option<u64>) -> Duration`: retry_after wins, then 2 s when used, else 10 s.
  - Commands: `spotify_connect() -> Result<(), String>` (opens the authorize URL in the default browser), `spotify_disconnect()`, `spotify_status() -> { connected: bool, clientIdSet: bool }`.

Poller: loop while connected. `GET https://api.spotify.com/v1/me/player/currently-playing`. `sampled_at` = local midpoint of the request (not Spotify's `timestamp`). 401 → refresh once, then retry. A refresh failing with `invalid_grant` → disconnect and send `Spotify(None)`. A `PlaybackEvent::Windows` change wakes the poller immediately (`Notify`).

- [ ] **Step 1: Check the endpoint is still allowed.** Read `https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide` and confirm `GET /me/player/currently-playing` is available to development-mode apps. If it isn't, stop and report to the user before writing code.
- [ ] **Step 2: Write the failing tests**

```rust
#[test] fn rfc7636_vector() { assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeKWYDVPgvX-GuKg-c"); }
#[test] fn authorize_url_uses_loopback_ip() { let u = authorize_url("cid", 7878, "ch", "st");
  assert!(u.as_str().contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7878%2Fspotify%2Fcallback")); assert!(u.as_str().contains("code_challenge_method=S256")); }
#[test] fn parses_track()        // fixture JSON → title, "A, B", duration_ms, position from progress_ms, art_url
#[test] fn ad_is_none()          // currently_playing_type "ad" / item null → None
#[test] fn no_content_is_none()  // status 204 → None
#[test] fn poll_delays() { assert_eq!(next_poll_delay(true,None), Duration::from_secs(2)); assert_eq!(next_poll_delay(false,None), Duration::from_secs(10)); assert_eq!(next_poll_delay(true,Some(30)), Duration::from_secs(30)); }
```

- [ ] **Step 3:** Run `cargo test spotify`. Expected: FAIL.
- [ ] **Step 4:** Implement the backend. `SpotifySetup.svelte`: a "Connect Spotify (optional)" heading with the badge "Requires Spotify Premium" and a 4-step guide: (1) open developer.spotify.com/dashboard → Create app; (2) set the Redirect URI to the exact `http://127.0.0.1:<port>/spotify/callback` with a copy button; (3) tick "Web API"; (4) paste the Client ID. Then Connect / Disconnect, and the status. The NowPlayingCard badge shows "Spotify" when `source == Spotify`.
- [ ] **Step 5:** Run `cargo test`. Expected: PASS. Manual (needs a Premium account): connect, then confirm the badge switches to Spotify and the overlay stays in sync after a seek. Disconnect, then confirm the badge returns to Windows and the keyring entry is gone (`cmdkey /list | findstr OpenMusix` shows nothing).
- [ ] **Step 6:** Commit `feat(spotify): optional PKCE connection and poller`.

---

### Task 15: Packaging, CI and end-to-end verification

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `src-tauri/icons/*` (from one 1024² source PNG via `npm run tauri icon`)
- Modify: `src-tauri/tauri.conf.json` (`bundle.targets = ["nsis"]`, `bundle.windows.nsis.installMode = "currentUser"`, WebView2 `downloadBootstrapper`)

**Interfaces:**
- Consumes: the whole app
- Produces: `OpenMusix_<version>_x64-setup.exe`; CI on push/PR; a release on `v*` tags via `tauri-apps/tauri-action` (draft GitHub Release).

- [ ] **Step 1:** `ci.yml` (windows-latest): `npm ci`, `npm test`, `npm run build`, `cargo test --manifest-path src-tauri/Cargo.toml`, `cargo clippy -- -D warnings`. `release.yml`: on tag `v*`, tauri-action with `releaseDraft: true`.
- [ ] **Step 2:** README: what it does, install, the SmartScreen note ("More info → Run anyway", because the build is unsigned), OBS setup (Browser Source URL; sizes 1920×1080 Focus / 600×200 Widget; untick "Shutdown source when not visible"), hotkey, local `.lrc` overrides, the optional Spotify step with the Premium note, and an LRCLIB credit.
- [ ] **Step 3:** Run `npm run tauri build`. Expected: the NSIS installer is under `src-tauri/target/release/bundle/nsis/`, around 10 MB.
- [ ] **Step 4: Manual E2E checklist** (record pass/fail in the PR description)
  - Fresh Windows user account: install, launch, play a Spotify desktop song → lyrics within 1 minute, no configuration.
  - Sync: the highlighted line changes within ~300 ms of the vocal. Seek, pause and skip each stay correct. If sync misses 300 ms, apply the spec fallback (re-read the timeline on every timeline event + Spotify-assisted correction) before release.
  - OBS: Browser Source `http://127.0.0.1:7878/overlay` is transparent with no scrollbars; the hotkey switches the layout live; two sources pinned with `?mode=focus` and `?mode=widget` each stay fixed.
  - A niche song → no lyrics shown. An instrumental → "♪ Instrumental". YouTube Music in Chrome → correct lyrics or none, never wrong ones.
  - Port 7878 occupied → banner shows; changing the port works.
  - Floating window: always on top, transparent, draggable.
- [ ] **Step 5:** Commit `chore: NSIS packaging, CI, README`.

---

## Self-review notes

- Spec coverage: every Goal, the timing, lyrics, overlay states, styling, panel, settings, tray, floating window, distribution and verification items map to Tasks 2–15. "Auto-picks another port" is superseded by the fixed-port decision (Task 9/13). Font choices, presets, tray art, CI and module layout are the builder's calls, made above.
- The LRCLIB album-mismatch → `instrumental: true` quirk was found during fact-checking and added to Task 4 and the Review Focus.
