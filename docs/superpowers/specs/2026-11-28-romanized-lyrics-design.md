# Romanized Lyrics — Design

> **Status:** Draft 2026-11-28, waiting for approval. Branch: `feat/romanized-lyrics` (from `origin/main` at `84ca416`).

## Context / why
Many streamers play Chinese, Japanese and Korean songs. Their viewers, and often the streamer too, can't read the original script, so they can't sing along. OpenMusix should show these lyrics in Latin letters: pinyin for Chinese, romaji for Japanese, and Revised Romanization for Korean. The v1 spec listed romanization as a non-goal. This feature lifts that.

## Goals
- A **Romanize** switch. When it is on, any line containing Chinese, Japanese or Korean script is romanized automatically. No per-song action is needed.
- A **Display** choice: show the romanization **under the original** line, or **replace** the original with it.
- **Language packs are optional downloads.** The user installs each one with a button in the panel. Nothing language-specific ships in the installer except Korean (see Decisions).
- Romanization works the same in the OBS overlay, the in-app preview and the floating window.

## Non-goals
- Translation (meaning), only pronunciation.
- Romanizing the track title or artist on the info screen.
- Per-song overrides, or hand-editing the romanized text.
- Perfect readings. Chinese characters and Japanese kanji with several readings will sometimes come out wrong.
- Languages other than Chinese, Japanese and Korean (for example Thai, Cyrillic, Arabic).

## Anti-goals (a failure even if it technically works)
- The installer grows by the size of a dictionary.
- Romanization causes lag, stutter or late lines on stream. The overlay must not do the heavy work.
- A tampered or corrupted download gets loaded.
- Turning the feature off leaves anything changed on screen.
- Lyrics show late because romanization delays them. The original must appear first, and romanization follows.

## Constraints
- Stack stays as it is: Tauri v2 + Rust backend + Svelte overlay and panel (see `2026-11-27-openmusix-design.md`).
- Every dependency and data file must use a permissive license that allows redistribution (MIT, Apache, BSD, or similar). **No GPL or other copyleft** (this rules out the `kakasi` crate). The IPADIC license (ICOT/NAIST) is permissive and allows redistribution with its notice, so it qualifies. Its license file ships inside the pack.
- Packs are hosted on this repo's GitHub releases, so the app never depends on a third-party file host at runtime.
- Windows only, like the rest of the app.

## Approaches considered
| | Approach | Trade-off |
|---|---|---|
| **A (chosen)** | **Rust romanizes. Packs contain only dictionary data, downloaded on demand.** | The overlay gets finished text and does no work. Romanization runs once per track. The Japanese tokenizer code adds about 1–2 MB to the binary even without the pack. |
| B | The overlay romanizes in the browser from downloaded JS bundles | Every OBS browser source loads a dictionary of 15 MB or more on every reload. The work repeats in each source. Slow, and against the "no stutter" anti-goal. |
| C | Downloadable helper executables, one per language | The smallest app, but it runs downloaded binaries. Antivirus false positives. Too much risk for little gain. |

**The user picked A.**

## Decisions made with the user
- Both display options are offered (under / replace), as a user setting.
- All language data is an optional package installed by a button.
- When the switch is on, romanization happens automatically. Off by default.
- **Packs are published on this branch, before merge.** The branch adds `packs.yml`. The user runs it, which publishes the public `packs-v1` release, and then the real URLs, SHA-256 hashes and sizes are committed to the manifest. The feature merges only with working downloads. Publishing the release is the user's action, and the plan pauses for it.

## Decisions I made (open to correction)
- **Korean needs no download.** Hangul romanizes by a fixed rule in a few KB of code. The pack list shows it as "Built-in". A download that fetches nothing would be pointless.
- **Telling the scripts apart, per run of characters:** Hangul is always romanized as Korean. Kana is always Japanese. Han characters follow the song: if any line contains kana (U+3040–U+30FF, U+31F0–U+31FF, U+FF66–U+FF9F), Han is read as Japanese, otherwise as Chinese. Japanese lines are tokenized whole, so kanji and kana inside them are read together.
- **A missing pack doesn't block the others.** Whatever can be romanized is. Runs that need a missing pack stay in the original script, and every missing pack is listed for the panel hint.
- **Pinyin with tone marks** (`zhōng guó`), one syllable per character, separated by spaces. It is the most common style for sing-along lyrics.
- **Romaji in modified Hepburn**, with spaces between words as the tokenizer splits them. The particles は/へ/を are read `wa`/`e`/`o` when tagged as particles.
- **Latin text, digits and punctuation pass through unchanged.** Mixed lines like `I love you 愛してる` keep the English part as it is.
- **A line that needs no romanization** (already Latin, or empty) gets no romanized line in "under" mode. In "replace" mode it shows the original.
- **Romanization is not cached on disk.** It is recomputed when lyrics resolve, which is fast (milliseconds per song). This keeps the lyrics cache format unchanged.

## Design

### Settings (`settings.rs`, mirrored in `src/lib/types.ts`)
```rust
pub struct Romanize {
    pub enabled: bool,          // default false
    pub display: RomanDisplay,  // default Below
}
pub enum RomanDisplay { Below, Replace } // JSON: "below" | "replace"
```
It is added to `Settings` as `romanize`, defaulting field by field like the rest. Old settings files load with defaults. It is projected into the `Snapshot` by `project_settings`, the same way `show_plain_lyrics` is.

### Snapshot additions (`hub.rs`)
- `romanize: Romanize`: the setting, so the overlay knows the display mode.
- `roman: Option<Vec<Option<String>>>`: romanized text parallel to the current lyric lines. `None` means romanization is off, not computed yet, or not applicable. An inner `None` means that line needs nothing. For `Synced`, it is indexed like the synced lines. For `PlainOnly`, it is indexed like `text.lines()`.
- `roman_missing: Vec<RomanLang>`: the packs the song needs that aren't installed (`"zh"`, `"ja"`). Empty when nothing is missing. Only the panel reads it, to show a hint with an Install button for each.

`LyricLine` and `LyricsResult` stay unchanged. The disk cache and the LRC parser are untouched.

### Romanizer module (`src-tauri/src/roman/`)
- `mod.rs`: `Romanizer` holds the loaded packs behind an `RwLock`. It exposes `romanize(lines) -> (Vec<Option<String>>, Vec<RomanLang> /* missing */)`, which picks the Han reading for the song, then romanizes each run by script. It also exposes `reload(lang)` after an install or removal.
- `zh.rs`: loads `pinyin.txt` from the pack folder into a `HashMap<char, &str>` (first listed reading). It converts Han runs character by character and keeps everything else.
- `ja.rs`: uses `lindera` loading the IPADIC dictionary **from the pack folder at runtime** (no `embed-*` feature). For each token it takes the katakana reading from the dictionary details, falling back to the surface text for unknown words. It converts kana to romaji with `wana_kana` (MIT). The builder pins a `lindera` version and uses that version's load-from-path API.
- `ko.rs`: decomposes Hangul syllables (U+AC00–U+D7A3) into initial/medial/final letters and maps them to Revised Romanization. It keeps word spacing. Basic sound-change rules only (for example, the final consonant before a vowel moves to the next syllable). Full assimilation rules are out of scope.

### Pipeline (`app.rs`)
1. The lyrics resolve as today, and the snapshot publishes the original lyrics immediately.
2. If `romanize.enabled`, a follow-up task runs in `spawn_blocking`. It detects the language, romanizes, and publishes `roman` / `roman_missing`. It uses the same generation guard as lyrics, so a result for an old track is dropped.
3. Recompute when (a) `romanize.enabled` turns on, (b) a pack finishes installing or is removed, or (c) lyrics change. When the switch turns off, `roman` and `roman_missing` are cleared at once.

### Packs (`src-tauri/src/roman/packs.rs`)
- **Manifest compiled into the app:**
  ```rust
  struct PackSpec { id: RomanLang, version: u32, url: &'static str, sha256: &'static str, size: u64 }
  ```
  `url` points to an asset on this repo's release tag `packs-v1` (for example `zh-pinyin-v1.zip`, `ja-ipadic-v1.zip`).
- **Install location:** `<app data>/packs/<id>/`, with a `pack.json` that records the version.
- **Install flow:** download to `<id>.part`, streaming. Emit a `pack-progress` event to the panel. Check the SHA-256 against the manifest and **reject on mismatch**. Unzip to `<id>.tmp/`, guarding against zip-slip (no absolute paths and no `..`). Rename to `<id>/`, then `Romanizer::reload`. On any failure, clean up the temp files and report the error to the panel.
- **Trust after install:** the installed folder is trusted. Files are not re-hashed on load, because the Japanese dictionary is tens of MB and would slow startup. `pack.json` records the zip's SHA-256 and version. If a pack fails to load (missing or corrupt files), it is marked `broken`, and the panel shows a [Reinstall] button. Nothing else breaks.
- **Remove:** delete the folder, then reload.
- **Tauri commands:** `list_packs` (id, label, size, state: notInstalled / installing{pct} / installed / builtIn / updateAvailable / broken), `install_pack(id)`, `remove_pack(id)`, `cancel_pack_install(id)`.
- **A version bump** in the manifest shows "Update" for an installed older version. The old version keeps working until it is replaced.
- **Uses the existing `reqwest` client setup** with the app User-Agent.

### Building the packs (CI)
A new workflow, `.github/workflows/packs.yml`, run with `workflow_dispatch`:
- **zh:** fetch `pinyin.txt` from `mozillazg/pinyin-data` at a pinned tag (MIT), plus its LICENSE. Zip them.
- **ja:** build the IPADIC dictionary with the `lindera` CLI at the same version the app uses. Zip it with the IPADIC license file.
- Upload both to release `packs-v1` and print each SHA-256 and size. Those values go into the manifest in a normal commit.

### Overlay (`FocusView.svelte`, `WidgetView.svelte`, `Overlay.svelte`)
- `Overlay.svelte` passes `roman` and `romanize.display` down.
- **Below:** each line renders the original, then a `<span class="om-roman">` at 0.55× font size. The original keeps the line's colour. The romanized line uses the same colour at reduced opacity, so it reads as secondary. Focus view centring already measures `offsetHeight`, so taller lines keep the active line centred. Widget view keeps `lineCount` lines, so each line just gets taller.
- **Replace:** show `roman[i] ?? line.text`.
- **Plain-only lyrics:** romanization is computed only when `showPlainLyrics` is on. With romanization off, the plain block renders exactly as today. With it on, the plain block renders line by line and follows Below/Replace like synced lines.
- Text sketch (Focus, Below). Each line shows the original first, then the romanization under it:
  ```
        我们一起学猫叫                     <- previous line, dim, smaller
        wǒ men yī qǐ xué māo jiào

     一起喵喵喵喵喵                        <- current line, highlight colour
     yī qǐ miāo miāo miāo miāo miāo        <- 0.55× size, dimmer
  ```

### Panel (`SettingsView.svelte`)
A new **Romanization** section:
```
Romanization
  [x] Romanize Chinese, Japanese and Korean lyrics
  Show romanization:  (•) Under the original   ( ) Instead of the original
  Language packs
    Chinese (pinyin)    0.3 MB   [Download]
    Japanese (romaji)   15 MB    [██████░░ 62%] [Cancel]
    Korean              Built-in
```
- Installed packs show [Remove].
- If `roman_missing` is set while the switch is on, `NowPlayingCard` shows: "This song is in Japanese. Install the Japanese pack to romanize it. [Install]".

## Testing
- **Rust unit tests:**
  - script detection (kana in song → Han read as ja; Han only → zh; Hangul → ko; Latin → untouched; mixed Hangul + Han with the zh pack missing → Hangul romanized, `zh` listed as missing)
  - pinyin conversion against a small fixture `pinyin.txt`
  - Korean RR on known words (`안녕하세요` → `annyeonghaseyo`)
  - pass-through of Latin text and punctuation
  - settings default and migration
  - zip-slip rejection
  - SHA-256 mismatch rejection
  - install against a `wiremock` server
- **Japanese tests** need the dictionary. They run only when `OPENMUSIX_JA_DICT` points to a built dictionary and are skipped otherwise. CI builds it once in the packs workflow.
- **Vitest:** Overlay renders Below and Replace modes. `null` roman renders exactly as today. Settings section toggles call `onSave` with the right patch.
- **Manual:** play one Chinese, one Japanese and one Korean song. Toggle the setting live. Install and remove packs while a song is playing.

## Left to the builder
- The exact `lindera` version and its load-from-path API.
- Which zip crate to use (must be MIT/Apache).
- Exact CSS values for the romanized line, within the rule "0.55× size, same colour, dimmer".
- Where the download-progress UI state lives in the panel.
