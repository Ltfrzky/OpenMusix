# Romanized Lyrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show Chinese, Japanese and Korean lyrics in Latin letters, either under the original line or in its place. The Chinese and Japanese data comes from language packs the user downloads in the panel.

**Architecture:** A new Rust module `src-tauri/src/roman/` romanizes the lyric lines once per track, after the lyrics resolve. The result is published in the hub `Snapshot` as a parallel `roman` array, and the overlay only renders it. A `PackManager` downloads, verifies (SHA-256) and unzips the pack data into `<app data>/packs/<id>/`. Korean is built in and needs no download.

**Tech Stack:** Rust (Tauri v2, tokio, reqwest, sha2), new crates `lindera` (runtime-loaded IPADIC), `wana_kana`, `zip`. Svelte 5 + Vitest for the frontend. GitHub Actions for building the packs.

**Spec:** `docs/superpowers/specs/2026-11-28-romanized-lyrics-design.md`

## Global Constraints

- **Licenses:** permissive licenses only (MIT, Apache, BSD, or IPADIC-style). No GPL or copyleft, so **no `kakasi`**.
- **Defaults:** `romanize.enabled = false`, `romanize.display = "below"`. Old `settings.json` files must still load.
- **JSON is camelCase.** `RomanLang` serializes as `"zh" | "ja" | "ko"`. `RomanDisplay` serializes as `"below" | "replace"`.
- **Layout and hosting:** packs live in `<app data>/packs/<id>/`. Downloads come only from the in-app manifest. A SHA-256 mismatch is rejected.
- **Original lyrics publish first,** and the romanization follows. Romanization is never written to the lyrics disk cache.
- **With `romanize.enabled = false`,** the overlay must render exactly as it does today, including plain lyrics.
- **Romanization styles:**
  - pinyin uses tone marks, one syllable per Han character, space-separated
  - romaji uses Hepburn via `wana_kana`, space-separated tokens, with particles は→`wa`, へ→`e`, を→`o`
  - Korean uses Revised Romanization with linking and nasalization only
- **Pass-through:** Latin text, digits, punctuation and emoji are never changed.
- **"Below" display:** the romanized line is 0.55× the font size, the same colour, and dimmer.
- **Every task ends green:** `cargo test`, `cargo clippy --all-targets -- -D warnings`, `npm test`, `npm run check`.

## Review Focus

1. **The track changes while romanization runs.** The old result must never show on the new song (Task 6 test `stale_roman_dropped`).
2. **The switch turns off mid-compute.** `roman` clears at once, and a late result is not published (Task 6 test `disabled_clears_and_blocks_late_result`).
3. **The app closes or the network drops mid-download.** The leftover `.part` / `.tmp` files are cleaned at the next start, and the pack shows "not installed", not broken (Task 5 test `startup_cleans_partial_install`).
4. **Lines that are only `♪`, emoji, punctuation or empty** get `None`. Full-width Latin `ＡＢＣ` passes through unchanged (Task 2 test `non_cjk_lines_are_none`).
5. **Download is clicked twice quickly.** The second install is rejected with `PackError::Busy` and doesn't corrupt the first (Task 5 test `second_install_is_busy`).

---

### Task 1: Settings and snapshot fields

**Files:**
- Modify: `src-tauri/src/settings.rs` (add `Romanize`, `RomanDisplay`, field `romanize` on `Settings` + default)
- Create: `src-tauri/src/roman/mod.rs` (only `RomanLang` for now) and register `mod roman;` in `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/hub.rs` (`Snapshot` fields)
- Modify: `src-tauri/src/commands.rs:50` (`project_settings`)
- Modify: `src/lib/types.ts`
- Modify (fixtures only): `src/overlay/Overlay.test.ts`, `src/panel/App.test.ts`, `dev/panel-preview.ts`

**Interfaces:**
- Produces:
  - `pub enum RomanLang { Zh, Ja, Ko }` with serde `rename_all = "lowercase"`, deriving `Copy, Eq, Hash, Ord`
  - `pub struct Romanize { pub enabled: bool, pub display: RomanDisplay }` (camelCase, `#[serde(default)]`)
  - `pub enum RomanDisplay { #[default] Below, Replace }` (camelCase)
  - `Snapshot` gains `romanize: Romanize`, `roman: Option<Vec<Option<String>>>` and `roman_missing: Vec<RomanLang>`
  - TS gains `type RomanLang = "zh" | "ja" | "ko"`, `type RomanDisplay = "below" | "replace"` and `interface Romanize { enabled: boolean; display: RomanDisplay }`
  - `Settings.romanize`, plus `Snapshot.romanize` / `roman: (string | null)[] | null` / `romanMissing: RomanLang[]`

- [ ] **Step 1: Write failing tests** in `settings.rs` tests:
```rust
#[test] fn romanize_defaults_off_below() {
    let s: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(s.romanize, Romanize { enabled: false, display: RomanDisplay::Below });
}
#[test] fn romanize_partial_patch_keeps_display() {
    let s: Settings = serde_json::from_str(r#"{"romanize":{"enabled":true}}"#).unwrap();
    assert!(s.romanize.enabled); assert_eq!(s.romanize.display, RomanDisplay::Below);
}
#[test] fn romanize_json_shape() {
    let v = serde_json::to_value(Romanize { enabled: true, display: RomanDisplay::Replace }).unwrap();
    assert_eq!(v, serde_json::json!({"enabled": true, "display": "replace"}));
}
```
  Then in `commands.rs` tests, `project_copies_romanize_and_clears_roman_when_disabled`:
  - set `snap.roman = Some(vec![Some("x".into())])` and `snap.roman_missing = vec![RomanLang::Ja]`
  - project settings with `enabled: false`
  - assert `snap.roman == None`, `snap.roman_missing.is_empty()` and `snap.romanize.enabled == false`
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml romanize`. Expected: FAIL (types don't exist).
- [ ] **Step 3: Implement** the types and fields.
  - `project_settings` copies `s.romanize` into `snap.romanize`.
  - When `!s.romanize.enabled`, it sets `snap.roman = None` and clears `snap.roman_missing`.
  - Update the TS types and the three TS fixtures (add `romanize: { enabled: false, display: "below" }`, `roman: null`, `romanMissing: []`).
- [ ] **Step 4: Run** the full Rust and TS checks (Global Constraints). Expected: all PASS.
- [ ] **Step 5: Commit** `feat(roman): add romanize setting and snapshot fields`

---

### Task 2: Script detection, the Romanizer shell and Korean

**Files:**
- Create: `src-tauri/src/roman/script.rs`, `src-tauri/src/roman/ko.rs`
- Modify: `src-tauri/src/roman/mod.rs`

**Interfaces:**
- Consumes: `RomanLang` (Task 1)
- Produces:
  - `script.rs`:
    - `pub enum Script { Han, Kana, Hangul, Other }`
    - `pub fn script_of(c: char) -> Script`
    - `pub fn runs(line: &str) -> Vec<(Script, &str)>`: consecutive characters with the same script. Under a Japanese reading, a caller merges adjacent Han and Kana runs.
    - `pub fn song_has_kana(lines: &[&str]) -> bool`
  - `ko.rs`: `pub fn romanize(run: &str) -> String`
  - `mod.rs`:
    - `pub struct RomanOutput { pub lines: Vec<Option<String>>, pub missing: Vec<RomanLang> }`
    - `pub struct Romanizer` with `pub fn new() -> Self`, `pub fn set_zh(&self, p: Option<zh::Pinyin>)`, `pub fn set_ja(&self, j: Option<ja::Japanese>)`, `pub fn has(&self, l: RomanLang) -> bool` (Ko is always true) and `pub fn romanize(&self, lines: &[&str]) -> RomanOutput`
    - In this task, `zh` / `ja` are stub modules whose types exist but are never `Some`. Tasks 3 and 4 fill them in.

Script ranges:
- **Han:** U+3400–4DBF, U+4E00–9FFF, U+F900–FAFF, U+20000–2FA1F, and 々 U+3005
- **Kana:** U+3040–30FF, U+31F0–31FF, U+FF66–FF9F (ー U+30FC is included)
- **Hangul:** U+AC00–D7A3, U+1100–11FF, U+3130–318F
- **Other:** everything else

`Romanizer::romanize` rules:
- A line with no Han, Kana or Hangul gives `None`.
- A song has a Japanese reading when `song_has_kana`. Under it, Han and Kana runs are merged and sent to `ja`; otherwise Han goes to `zh`.
- Hangul always goes to `ko`.
- A run whose pack is missing stays as original text, and its language is added to `missing` (deduplicated and sorted).
- Pieces are joined with a single space between a romanized run and an adjacent non-space neighbour. Then spaces are collapsed and the line is trimmed.
- If every CJK run in a line was missing, the line gives `None`, not a copy of the original.

- [ ] **Step 1: Write failing tests** (in `ko.rs` and `mod.rs`):
```rust
#[test] fn ko_basic() { assert_eq!(ko::romanize("안녕하세요"), "annyeonghaseyo"); assert_eq!(ko::romanize("사랑해"), "saranghae"); }
#[test] fn ko_linking() { assert_eq!(ko::romanize("한국어"), "hangugeo"); }
#[test] fn ko_nasalization() { assert_eq!(ko::romanize("감사합니다"), "gamsahamnida"); }
#[test] fn ko_keeps_spaces() { assert_eq!(ko::romanize("사랑 해"), "sarang hae"); }
#[test] fn non_cjk_lines_are_none() {
    let r = Romanizer::new().romanize(&["Hello world", "", "♪", "🎵!!", "ＡＢＣ"]);
    assert_eq!(r.lines, vec![None, None, None, None, None]); assert!(r.missing.is_empty());
}
#[test] fn mixed_latin_and_hangul() {
    assert_eq!(Romanizer::new().romanize(&["I love you 사랑해"]).lines, vec![Some("I love you saranghae".into())]);
}
#[test] fn missing_zh_keeps_hangul() {
    let r = Romanizer::new().romanize(&["사랑 愛"]);
    assert_eq!(r.lines, vec![Some("sarang 愛".into())]); assert_eq!(r.missing, vec![RomanLang::Zh]);
}
#[test] fn kana_song_routes_han_to_ja() {
    let r = Romanizer::new().romanize(&["愛してる", "空"]);
    assert_eq!(r.missing, vec![RomanLang::Ja]); assert_eq!(r.lines, vec![None, None]);
}
#[test] fn script_ranges() { assert_eq!(script_of('ー'), Script::Kana); assert_eq!(script_of('ｱ'), Script::Kana); assert_eq!(script_of('々'), Script::Han); assert_eq!(script_of('Ａ'), Script::Other); }
```
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml roman`. Expected: FAIL.
- [ ] **Step 3: Implement** `script.rs`, `ko.rs` and `Romanizer`.
  - Korean: decompose with `(c - 0xAC00)` into initial = /588, medial = %588/28, final = %28. Use the standard RR tables.
  - Linking: a final consonant before a syllable starting with silent ㅇ moves to the next syllable.
  - Nasalization: final ㄱ/ㄷ/ㅂ before ㄴ/ㅁ becomes ng/n/m.
  - `Romanizer` holds `RwLock<Option<zh::Pinyin>>` and `RwLock<Option<ja::Japanese>>`.
- [ ] **Step 4: Run** the Task 2 tests, then the full checks. Expected: PASS.
- [ ] **Step 5: Commit** `feat(roman): script detection, romanizer and built-in Korean`

---

### Task 3: Chinese pinyin from pack data

**Files:**
- Modify: `src-tauri/src/roman/zh.rs`

**Interfaces:**
- Produces:
  - `pub struct Pinyin` (owns `HashMap<char, String>`)
  - `pub fn parse(text: &str) -> Pinyin`
  - `pub fn load(dir: &Path) -> Result<Pinyin, RomanError>` (reads `dir/pinyin.txt`)
  - `pub fn romanize(&self, run: &str) -> String`
  - `pub enum RomanError { Io(std::io::Error), BadData(String) }` in `mod.rs`, via `thiserror`

`pinyin.txt` line format, for example `U+4E2D: zhōng,zhòng  # 中`:
- Skip lines starting with `#` and blank lines.
- Take the first reading before `,`.
- A Han character not in the map passes through unchanged.

- [ ] **Step 1: Write failing tests:**
```rust
const FIX: &str = "# c\nU+4E2D: zhōng,zhòng  # 中\nU+56FD: guó  # 国\nU+7231: ài  # 爱\n";
#[test] fn parse_and_romanize() { assert_eq!(parse(FIX).romanize("中国"), "zhōng guó"); }
#[test] fn unknown_char_passes() { assert_eq!(parse(FIX).romanize("中龘"), "zhōng 龘"); }
#[test] fn romanizer_uses_zh() {
    let r = Romanizer::new(); r.set_zh(Some(parse(FIX)));
    let o = r.romanize(&["我爱中国!", "Hi 中国"]);
    assert_eq!(o.lines, vec![Some("我 ài zhōng guó!".into()), Some("Hi zhōng guó".into())]); assert!(o.missing.is_empty());
}
#[test] fn load_missing_file_errors() { assert!(load(std::path::Path::new("Z:/nope")).is_err()); }
```
  `我` isn't in the fixture, so it passes through.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml zh`. Expected: FAIL.
- [ ] **Step 3: Implement** `parse`, `load` and `romanize` as specified.
- [ ] **Step 4: Run** the full checks. Expected: PASS.
- [ ] **Step 5: Commit** `feat(roman): pinyin romanization from pack data`

---

### Task 4: Japanese romaji via lindera and wana_kana

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `lindera` with **no** `embed-*` feature, and `wana_kana`, pinning exact versions)
- Modify: `src-tauri/src/roman/ja.rs`

**Interfaces:**
- Produces:
  - `pub struct Japanese` (owns a lindera `Tokenizer`)
  - `pub fn load(dir: &Path) -> Result<Japanese, RomanError>`
  - `pub fn romanize(&self, run: &str) -> String`
  - `pub struct Tok<'a> { pub surface: &'a str, pub reading: Option<&'a str>, pub particle: bool }`
  - `pub fn toks_to_romaji(toks: &[Tok]) -> String`: pure, so it can be tested without the dictionary

Rules:
- The reading comes from the IPADIC details field 7 (katakana). It is `None` when it's `*` or missing.
- A particle is a token whose part of speech (details field 0) is `助詞`. A particle surface of は/へ/を maps to `wa`/`e`/`o`.
- Otherwise use `wana_kana` to romaji on the reading, or on the surface when there's no reading. A surface that is still not kana (unknown kanji) passes through unchanged.
- Tokens are joined with single spaces.
- Use the load-from-path API of the pinned `lindera` version. Check docs.rs for that version.

- [ ] **Step 1: Write failing tests:**
```rust
fn t<'a>(s: &'a str, r: Option<&'a str>, p: bool) -> Tok<'a> { Tok { surface: s, reading: r, particle: p } }
#[test] fn particles() { assert_eq!(toks_to_romaji(&[t("私", Some("ワタシ"), false), t("は", Some("ハ"), true), t("東京", Some("トウキョウ"), false), t("へ", Some("ヘ"), true)]), "watashi wa toukyou e"); }
#[test] fn wo_particle() { assert_eq!(toks_to_romaji(&[t("君", Some("キミ"), false), t("を", Some("ヲ"), true)]), "kimi o"); }
#[test] fn sokuon_and_long_vowel() { assert_eq!(toks_to_romaji(&[t("ずっと", Some("ズット"), false), t("ラーメン", Some("ラーメン"), false)]), "zutto raamen"); }
#[test] fn unknown_kanji_passes() { assert_eq!(toks_to_romaji(&[t("龘", None, false)]), "龘"); }
#[test] fn real_dictionary() {
    let Ok(dir) = std::env::var("OPENMUSIX_JA_DICT") else { return };
    let j = load(std::path::Path::new(&dir)).unwrap();
    assert_eq!(j.romanize("私は猫です"), "watashi wa neko desu");
}
```
  If `wana_kana` renders ー as `ā` rather than `aa`, change `raamen` in the test to the crate's actual output and note it in the commit. The spec only requires Hepburn via `wana_kana`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml ja`. Expected: FAIL.
- [ ] **Step 3: Implement** `toks_to_romaji`, `load` and `romanize`.
- [ ] **Step 4: Run** the full checks, and record the release binary size before and after in the commit body. Expected: PASS, with growth of about 1–2 MB.
- [ ] **Step 5: Commit** `feat(roman): Japanese romaji via runtime-loaded IPADIC`

---

### Task 5: Pack manager (download, verify, install, remove)

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `zip`, default-features off, deflate only)
- Create: `src-tauri/src/roman/packs.rs`

**Interfaces:**
- Consumes:
  - `Romanizer::set_zh` / `set_ja` / `has`
  - `zh::load` / `ja::load`
  - `lyrics::lrclib::user_agent()`
- Produces:
  - `pub struct PackSpec { pub id: RomanLang, pub version: u32, pub label: String, pub url: String, pub sha256: String, pub size: u64 }`
  - `pub fn manifest() -> Vec<PackSpec>`: two entries, zh "Chinese (pinyin)" and ja "Japanese (romaji)". The URLs point to `https://github.com/Ltfrzky/OpenMusix/releases/download/packs-v1/zh-pinyin-v1.zip` and `.../ja-ipadic-v1.zip`. `sha256` / `size` hold placeholder values until Task 10.
  - `#[serde(tag = "state", rename_all = "camelCase")] pub enum PackState { NotInstalled, Installing { pct: u8 }, Installed, BuiltIn, UpdateAvailable, Broken }`
  - `pub struct PackInfo { pub id: RomanLang, pub label: String, pub size: u64, #[serde(flatten)] pub state: PackState }` (camelCase)
  - `pub enum PackError { Busy, Network(String), Checksum, BadArchive(String), Io(std::io::Error), Cancelled, Unknown }` (thiserror, `Display` is user-readable)
  - `pub struct PackManager` with:
    - `pub fn new(dir: PathBuf, specs: Vec<PackSpec>, roman: Arc<Romanizer>) -> Self`: removes leftover `*.part` / `*.tmp`, then loads installed packs into `roman`, marking load failures as broken
    - `pub fn list(&self) -> Vec<PackInfo>`: always includes Ko as `BuiltIn` (label "Korean", size 0)
    - `pub async fn install(&self, id: RomanLang, on_progress: impl Fn(u8) + Send) -> Result<(), PackError>`
    - `pub fn cancel(&self, id: RomanLang)`
    - `pub fn remove(&self, id: RomanLang) -> Result<(), PackError>`

Install steps:
1. Mark the pack as installing, or return `Busy`.
2. GET the URL, streaming with `resp.chunk()` into `<dir>/<id>.part`. Hash as it streams. Call `on_progress` with whole-percent steps from `size`. Check the cancel flag between chunks.
3. On a hash mismatch, return `Checksum`.
4. Unzip to `<dir>/<id>.tmp/`. Reject any entry whose `enclosed_name()` is `None` (zip-slip) with `BadArchive`.
5. Write `pack.json` `{"id","version","sha256"}`.
6. Remove the old `<dir>/<id>/`, rename the `.tmp` folder into place, and load it into `roman`.
7. On every error path, delete the `.part` / `.tmp` files and clear the installing flag.

`UpdateAvailable` means `pack.json`'s version is lower than the spec's.

- [ ] **Step 1: Write failing tests** with `wiremock` and `tempfile`. Build the zips in the test with the `zip` writer: a zh zip containing `pinyin.txt` = Task 3 `FIX`.
  - `install_zh_happy_path`: after install, `list()` shows zh `Installed`, `roman.has(Zh)` is true, and `on_progress` saw 100.
  - `checksum_mismatch_rejected`: spec sha256 = `"00"*32`, result is `Err(PackError::Checksum)`, there's no `<dir>/zh`, and no `.part` remains.
  - `zip_slip_rejected`: an archive containing entry `../evil.txt` gives `Err(BadArchive(_))`, and `dir.parent()/evil.txt` doesn't exist.
  - `http_404_is_network_error`: the result is `Err(Network(_))`, with no leftovers.
  - `second_install_is_busy`: the server delays 300 ms. Start install A and immediately install B for the same id. B gives `Err(Busy)`, and A gives `Ok`.
  - `cancel_mid_download`: the server delays the body. `cancel(Zh)` gives `Err(Cancelled)`, with no leftovers.
  - `startup_cleans_partial_install`: create `zh.part` and `zh.tmp/x`, then `new()`. Both are gone, and zh is `NotInstalled`.
  - `broken_pack_detected`: create `<dir>/zh/pack.json` without `pinyin.txt`, then `new()`. zh is `Broken`, and `roman.has(Zh)` is false.
  - `remove_unloads`: after install, `remove(Zh)` makes it `NotInstalled`, and `roman.has(Zh)` is false.
  - `update_available`: `pack.json` version 0 with spec version 1 gives `UpdateAvailable`.
  - `korean_always_builtin`: Ko is in `list()` as `BuiltIn`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml packs`. Expected: FAIL.
- [ ] **Step 3: Implement** `packs.rs` as specified. Use a `reqwest::Client` with `user_agent()` and a 30 s connect timeout but no overall timeout, because the Japanese pack is about 15 MB.
- [ ] **Step 4: Run** the full checks. Expected: PASS.
- [ ] **Step 5: Commit** `feat(roman): downloadable language packs with checksum and zip-slip guard`

---

### Task 6: Pipeline wiring and Tauri commands

**Files:**
- Modify: `src-tauri/src/app.rs` (`AppCtx` gains `roman: Arc<Romanizer>` and `packs: Arc<PackManager>`; add `refresh_roman`)
- Modify: `src-tauri/src/lib.rs` (construct both with `data.join("packs")` and `manifest()`; register the commands)
- Modify: `src-tauri/src/commands.rs` (new commands; `update_settings` triggers a refresh)
- Create: tests in `src-tauri/src/roman/mod.rs` for the pure helpers

**Interfaces:**
- Consumes: Tasks 1–5
- Produces:
  - `pub fn roman_input(snap: &Snapshot) -> Option<Vec<String>>`, in `roman/mod.rs`. It gives `None` unless `romanize.enabled` and the lyrics are `Ready`. `Synced` gives the line texts. `PlainOnly` gives `text.lines()`, but only if `show_plain_lyrics`. Anything else gives `None`.
  - `pub fn apply_roman(snap: &mut Snapshot, key: &Option<String>, input: &[String], out: RomanOutput) -> bool`: publishes only if `snap.track_key == *key && roman_input(snap).as_deref() == Some(input)`. Returns whether it published.
  - `pub fn refresh_roman(ctx: &Arc<AppCtx>)`, in `app.rs`. It snapshots `(key, input)`, clearing `roman`/`roman_missing` when `input` is `None`. Then it runs `Romanizer::romanize` in `tokio::task::spawn_blocking` and calls `hub.modify(|s| { apply_roman(s, &key, &input, out); })`.
  - Tauri commands:
    - `list_packs() -> Vec<PackInfo>`
    - `install_pack(id: RomanLang) -> Result<(), String>`: emits `pack-progress` `{ id, pct }` while downloading. On finish, it emits `packs-changed` `Vec<PackInfo>` and calls `refresh_roman`.
    - `remove_pack(id: RomanLang) -> Result<(), String>`: same events and refresh.
    - `cancel_pack_install(id: RomanLang)`

Triggers for `refresh_roman`:
- right after the lyrics task publishes `Ready`
- after `update_settings` when `romanize` or `show_plain_lyrics` changed
- after install or remove

When lyrics go back to `Loading`, `run_pipeline` clears `roman` and `roman_missing`.

- [ ] **Step 1: Write failing tests:**
  - `roman_input_rules`:
    - disabled gives `None`
    - `Loading` gives `None`
    - `Synced(["a","b"])` gives `Some(["a","b"])`
    - `PlainOnly("x\ny")` gives `None` when `show_plain_lyrics` is false, and `Some(["x","y"])` when it's true
  - `stale_roman_dropped`: build `input` for track key `"a|b"`, then set `snap.track_key = Some("c|d")`. `apply_roman` gives `false`, and `snap.roman` stays `None`.
  - `disabled_clears_and_blocks_late_result`: compute `input` while enabled, then `project_settings` with enabled = false. `apply_roman` gives `false`, and `snap.roman == None`.
  - `apply_publishes_when_current`: same key and input give `true`, and `snap.roman == Some(out.lines)` and `snap.roman_missing == out.missing`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml roman_input apply_`. Expected: FAIL.
- [ ] **Step 3: Implement** the helpers, `refresh_roman`, the triggers, the commands and the registration in `generate_handler!`.
- [ ] **Step 4: Run** the full checks. Expected: PASS. Then run `npm run tauri dev`, turn on Romanize with a Korean song playing, and check that the panel snapshot gains `roman` (log it).
- [ ] **Step 5: Commit** `feat(roman): romanize after lyrics resolve and expose pack commands`

---

### Task 7: Overlay rendering (under / replace)

**Files:**
- Modify: `src/overlay/Overlay.svelte`, `src/overlay/FocusView.svelte`, `src/overlay/WidgetView.svelte`
- Test: `src/overlay/Overlay.test.ts`

**Interfaces:**
- Consumes: `Snapshot.romanize`, `Snapshot.roman` (Task 1)
- Produces:
  - Both views take new props: `roman: (string | null)[] | null` and `display: RomanDisplay`.
  - The romanized element has class `om-roman`.

Rendering rules:
- **Below:** render `<span class="om-roman">` with `roman[i]` after the original text, inside the same line element. Use `display: block`, `font-size: 0.55em`, `font-weight: 500`, `opacity: 0.7`. Nothing renders when `roman[i]` is null.
- **Replace:** the line text is `roman[i] ?? line.text`.
- **Plain lyrics:** when `roman` is non-null and `plainText` is shown, render the plain text as one `<span class="plain-line">` per line, with the same Below/Replace rule. When `roman` is null, keep today's single `.plain` block unchanged.
- **Focus centring** keeps measuring `offsetHeight`, with no change needed.

- [ ] **Step 1: Write failing tests:**
  - `renders roman under the line`: synced `["你好"]`, roman `["nǐ hǎo"]`, display `"below"`. `getByText("你好")` and `getByText("nǐ hǎo")` both exist, and the latter has class `om-roman`.
  - `replace shows only roman`: display `"replace"`. `queryByText("你好")` is null, and `getByText("nǐ hǎo")` exists.
  - `replace falls back to original for null`: roman `[null]` shows the original.
  - `null roman renders as today`: `container.querySelector(".om-roman")` is null.
  - `plain lyrics romanized per line`: plainOnly `"你好\n再见"`, `showPlainLyrics: true`, roman `["nǐ hǎo", null]`, below. Two `.plain-line` elements.
  - `widget view also renders roman`: `pinnedMode: "widget"`, below. `.om-roman` exists.
- [ ] **Step 2: Run** `npm test -- Overlay`. Expected: FAIL.
- [ ] **Step 3: Implement** the props and markup as specified.
- [ ] **Step 4: Run** `npm test` and `npm run check`. Expected: PASS. Then look at it with `dev/panel-preview.html`, using a fixture with roman in both modes.
- [ ] **Step 5: Commit** `feat(overlay): show romanized lyrics under or instead of the original`

---

### Task 8: Panel UI (settings section, pack list, missing-pack hint)

**Files:**
- Modify: `src/panel/api.ts`, `src/panel/SettingsView.svelte`, `src/panel/NowPlayingCard.svelte`, `src/lib/types.ts`
- Test: `src/panel/App.test.ts` (or a new `src/panel/Romanization.test.ts`)
- Modify: `README.md` (short "Romanized lyrics" section)

**Interfaces:**
- Consumes: the Task 6 commands and events
- Produces:
  - TS types: `PackState` (union on `state`, with `installing` carrying `pct`) and `PackInfo { id: RomanLang; label: string; size: number } & PackState`
  - `api.listPacks()`, `api.installPack(id)`, `api.removePack(id)`, `api.cancelPackInstall(id)`
  - `onPackProgress(cb)` and `onPacksChanged(cb)`

Copy (exact):
- Section title: `Romanization`
- Switch: `Romanize Chinese, Japanese and Korean lyrics`
- Radio label: `Show romanization:`. Options: `Under the original` and `Instead of the original`.
- Subheading: `Language packs`
- Buttons: `Download`, `Cancel`, `Remove`, `Update`, `Reinstall`. The built-in label is `Built-in`.
- Size is shown as MB with 1 decimal.
- Missing-pack hint in `NowPlayingCard`, shown when `romanize.enabled && romanMissing.length`: `This song is in {Japanese|Chinese}. Install the {Japanese|Chinese} pack to romanize it.` followed by an `Install` button per missing language.
- A failed install shows the error string from the command next to the pack row.

- [ ] **Step 1: Write failing tests** (mock `api`):
  - The switch calls `onSave({ romanize: { enabled: true, display: "below" } })`.
  - The "Instead of the original" radio calls `onSave({ romanize: { enabled: <current>, display: "replace" } })`.
  - The pack list renders three rows from a mocked `listPacks`. Korean shows `Built-in` and no button. zh `notInstalled` shows `Download`, and clicking it calls `api.installPack("zh")`.
  - The `installing` state with `pct: 62` shows `62%` and `Cancel`.
  - The `NowPlayingCard` hint appears with `romanMissing: ["ja"]` and enabled, and is absent when disabled.
- [ ] **Step 2: Run** `npm test`. Expected: FAIL.
- [ ] **Step 3: Implement.** Send the whole `romanize` object in patches, because `apply_patch` merges nested objects but the TS type is `Partial<Settings>`.
- [ ] **Step 4: Run** `npm test` and `npm run check`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(panel): romanization settings and language pack manager`

---

### Task 9: Pack build workflow

**Files:**
- Create: `.github/workflows/packs.yml`

The workflow runs on `workflow_dispatch` only, on `ubuntu-latest`, with `permissions: contents: write`.

Steps:
- **zh:**
  - Download `pinyin.txt` and `LICENSE` from `https://raw.githubusercontent.com/mozillazg/pinyin-data/<TAG>/`. Pin `<TAG>` to the latest existing release tag, and check that it exists.
  - `zip -X zh-pinyin-v1.zip pinyin.txt LICENSE`.
- **ja:**
  - `cargo install lindera-cli --version <same version as the app's lindera> --locked`.
  - Download the mecab-ipadic 2.7.0-20070801 source.
  - Build the IPADIC dictionary with that CLI version's build command into `ipadic/`. Copy the IPADIC `COPYING` in as `LICENSE`.
  - `cd ipadic && zip -X -r ../ja-ipadic-v1.zip .`. Files go at the zip root, because `ja::load(dir)` reads the folder directly.
- **Release:**
  - `gh release view packs-v1 || gh release create packs-v1 --prerelease --title "Language packs v1" --notes "Romanization data for OpenMusix. Installed from inside the app."`
  - Then `gh release upload packs-v1 *.zip --clobber`.
- **Summary:** write `sha256sum *.zip` and `stat -c '%n %s' *.zip` to `$GITHUB_STEP_SUMMARY`.

- [ ] **Step 1:** Write the workflow.
- [ ] **Step 2:** Validate it locally with `actionlint`, if installed. Otherwise do a careful YAML review against the steps above.
- [ ] **Step 3:** Build the ja dictionary locally with the same CLI. Run `OPENMUSIX_JA_DICT=<path> cargo test --manifest-path src-tauri/Cargo.toml real_dictionary`. Expected: PASS (`watashi wa neko desu`). If the output differs because of tokenization, update the expected string in Task 4 to the real output and note it.
- [ ] **Step 4: Commit** `ci: workflow to build and publish romanization packs`

---

### Task 10: Publish packs (user action) and pin the manifest

**Files:**
- Modify: `src-tauri/src/roman/packs.rs` (`manifest()` values)

- [ ] **Step 1: STOP and hand off to the user.**
  - Push the branch: `git push -u origin feat/romanized-lyrics`. Ask before pushing.
  - Ask the user to run **Actions → Build language packs → Run workflow** on `feat/romanized-lyrics`. This publishes the public `packs-v1` prerelease.
  - Wait for the user to confirm it finished.
- [ ] **Step 2:** Read the SHA-256 hashes and sizes from the run summary: `gh run view <id> --log` or the summary page. Paste them into `manifest()`.
- [ ] **Step 3:** Add a test `manifest_is_pinned`. For every spec: `sha256.len() == 64`, it is all lowercase hex and not all zeros, `size > 0`, and the URL starts with `https://github.com/Ltfrzky/OpenMusix/releases/download/packs-v1/`.
- [ ] **Step 4:** Run the full checks. Expected: PASS.
- [ ] **Step 5: Manual end-to-end** in `npm run tauri dev`:
  - Download each pack from Settings and watch the progress.
  - Play a Chinese, a Japanese and a Korean song, and check both display modes in the preview and the OBS URL.
  - Toggle the switch while a song plays.
  - Remove the Japanese pack while a Japanese song plays. The romanization disappears and the missing-pack hint shows.
- [ ] **Step 6: Commit** `feat(roman): pin published pack checksums`
