# OpenMusix

Synced lyrics for your stream. OpenMusix detects the song playing on your PC, fetches time-synced lyrics and shows them as a transparent overlay for OBS.

- **No account, no keys.** It reads your system's media session, so it works with Spotify, YouTube Music in a browser, and most other players.
- **Two layouts, switched live:** *Focus* (big, centred lyrics) and *Widget* (a compact card with album art). Press **Ctrl+Alt+L** or use the switch in the app.
- **Never the wrong song's lyrics.** A match must agree on title, artist and duration (within 3 seconds); otherwise nothing is shown.
- **Optional Spotify connection** for sharper timing and album art, including when Spotify plays on another device. This needs **Spotify Premium**.

Runs on Windows 10 and 11, macOS 11 or later, and Linux desktops with MPRIS (most players support it).

## Install

Download the file for your system from [Releases](https://github.com/Ltfrzky/OpenMusix/releases). None of the builds are code-signed yet, so each system warns on first launch.

**Windows:** run `OpenMusix_x.y.z_x64-setup.exe`. If SmartScreen says "Windows protected your PC", choose **More info → Run anyway**.

**macOS:** open the `.dmg` and drag OpenMusix to Applications. The first time, right-click the app and choose **Open**, then confirm. If OpenMusix falls back to asking Spotify or Music directly, macOS asks once whether it may control them. Allow it, or song detection stops for those apps.

**Linux:** install the `.deb` (`sudo apt install ./OpenMusix_x.y.z_amd64.deb`), or make the `.AppImage` executable (`chmod +x`) and run it. The tray icon needs AppIndicator support. GNOME needs the "AppIndicator and KStatusNotifierItem Support" extension.

Then play a song. The app's Live tab shows it within a few seconds.

### What each system can detect

- **Windows:** any app that shows up in the Windows media controls.
- **Linux:** any player that supports MPRIS, including Spotify, browsers, VLC and most music apps.
- **macOS:** any app in the system Now Playing widget, browsers included. If that reader fails to start, OpenMusix falls back to Spotify and Music only. Connecting Spotify (below) works on every system.

## Add it to OBS

1. In OBS: **Sources → + → Browser**.
2. URL: `http://127.0.0.1:7878/overlay`
3. Size: **1920 × 1080** for Focus, or **600 × 200** for a Widget-only source.
4. Untick **Shutdown source when not visible** so lyrics stay in sync between scenes.

The background is transparent. To keep one layout in a scene regardless of the switch, add `?mode=focus` or `?mode=widget` to the URL. The Live tab has copy buttons for all three URLs.

The overlay only listens on `127.0.0.1`, so OBS has to run on the same PC.

## Add it to TikTok LIVE Studio

LIVE Studio's **Link** source rejects `127.0.0.1` and `localhost`, so use `http://localtest.me:7878/overlay` instead. `localtest.me` is a public domain that always points back to your own PC, so the overlay still never leaves it. The Live tab has a copy button for it.

If LIVE Studio still calls the link invalid, set the port to **80** in Settings. On Linux, port 80 needs root, so use OBS there instead. The URL then becomes `http://localtest.me/overlay`, with no port. If your router's DNS blocks names that point to 127.0.0.1, add the overlay in OBS and bring it into LIVE Studio through OBS Virtual Camera instead.

## Timing

- **All songs** slider: shifts every lyric earlier or later (±2 s).
- **This song** ±100 ms buttons: a per-song correction that's remembered.

## Your own lyrics

Settings → **Open lyrics folder**, then drop in `.lrc` files named `Artist - Title.lrc`. They win over downloaded lyrics.

## Romanized lyrics

To romanize lyrics:

1. Go to Settings → **Romanization** and turn on **Romanize Chinese, Japanese and Korean lyrics**.
2. Choose where the romanized text goes: under the original line, or instead of it.

Chinese shows as pinyin, Japanese as romaji, and Korean in Revised Romanization.

Korean works out of the box. Chinese and Japanese each need a language pack: press **Download** next to it. The packs are about 0.3 MB and 15 MB. Each download is checked against a checksum built into the app before it is used. Words with several possible readings are sometimes romanized wrong.

## Connect Spotify (optional)

Settings → **Connect Spotify** walks you through it:

1. Create an app at [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard).
2. Add the redirect URI the app shows (`http://127.0.0.1:7878/spotify/callback`).
3. Tick **Web API**, then paste the app's Client ID into OpenMusix and press **Connect**.

Spotify requires the developer app's owner to have Premium. Your sign-in token is stored in the system keychain: Windows Credential Manager, macOS Keychain, or the Secret Service (GNOME Keyring, KWallet) on Linux.

If you change the port in Settings, update both the OBS URL and the Spotify redirect URI.

## Credits

Lyrics are provided by [LRCLIB](https://lrclib.net), a free, community-maintained lyrics database.

The language packs use these sources:
- Pinyin readings from [pinyin-data](https://github.com/mozillazg/pinyin-data) (MIT).
- Japanese readings from the IPADIC dictionary (its license ships inside the pack), tokenized with [Lindera](https://github.com/lindera/lindera).
- Romaji conversion by [wana_kana](https://github.com/PSeitz/wana_kana_rust).

On macOS, song detection uses [mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) (BSD-3-Clause). Its license ships inside the app.

## Development

Requirements: Node 22+ and Rust (stable), plus the system tools for your OS:

- **Windows:** the Visual Studio C++ Build Tools (MSVC).
- **macOS:** Xcode command line tools and CMake. Run `scripts/build-mediaremote-adapter.sh` once before building; it puts the adapter in `src-tauri/macos/`.
- **Linux (Debian/Ubuntu):** `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libfontconfig1-dev libfreetype6-dev libdbus-1-dev`.

```powershell
npm install
npm run tauri dev
```

- `npm test` runs the frontend tests (Vitest); `cargo test --manifest-path src-tauri/Cargo.toml` runs the Rust tests.
- `npm run tauri build` produces the installers for your OS under `src-tauri/target/release/bundle/`: NSIS on Windows, `.dmg`/`.app` on macOS, `.deb`/`.AppImage` on Linux.
- While the dev app is running, `http://localhost:1420/dev/panel-preview.html` renders the control panel in a normal browser with mocked IPC, which is handy for UI work.

Design spec and implementation plan: [`docs/superpowers`](docs/superpowers).
