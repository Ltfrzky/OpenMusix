//! Pure parsers for the two macOS song sources: the MediaRemote adapter's JSON lines and the
//! AppleScript fallback's tab-separated output. Compiled and tested on every platform; only
//! `macos.rs` uses them at runtime.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
use super::system::SessionInfo;
use super::SourceSnapshot;
use crate::model::Track;
use base64::Engine;
use serde_json::Value;

/// What one source reading yields before the caller attaches the hub's art URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacNow {
    /// Player identity, e.g. a bundle id or "spotify".
    pub app_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub position_ms: u64,
    pub sampled_at: i64,
    pub is_playing: bool,
    /// Inline cover bytes and MIME type (adapter only).
    pub art: Option<(Vec<u8>, String)>,
    /// Remote cover URL (Spotify AppleScript only).
    pub art_url: Option<String>,
}

impl MacNow {
    pub fn track_id(&self) -> String {
        format!("{}\u{1f}{}\u{1f}{}", self.title, self.artist, self.album.as_deref().unwrap_or_default())
    }

    pub fn snapshot(&self, art_url: Option<String>) -> SourceSnapshot {
        SourceSnapshot {
            track: Track {
                title: self.title.clone(),
                artist: self.artist.clone(),
                album: self.album.clone(),
                duration_ms: self.duration_ms,
                art_url,
            },
            position_ms: self.position_ms,
            sampled_at: self.sampled_at,
            is_playing: self.is_playing,
        }
    }
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

fn micros_to_ms(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_f64).filter(|f| f.is_finite() && *f >= 0.0).map(|f| (f / 1000.0) as u64)
}

/// Arguments for `mediaremote-adapter.pl <framework> stream ...`. Full payloads (no diffs) in
/// microseconds keep the parser stateless; a short debounce merges bursts on track change.
pub const ADAPTER_STREAM_ARGS: [&str; 4] = ["stream", "--no-diff", "--micros", "--debounce=100"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterLine {
    /// Something is playing (or paused).
    Now(Box<MacNow>),
    /// The payload was empty: nothing is playing.
    Idle,
    /// Not a data line, or not JSON. Ignore it.
    Skip,
}

/// Parse one stdout line of the adapter in `--no-diff --micros` mode.
pub fn parse_adapter_line(line: &str, now_ms: i64) -> AdapterLine {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return AdapterLine::Skip };
    if v.get("type").and_then(Value::as_str) != Some("data") {
        return AdapterLine::Skip;
    }
    let Some(p) = v.get("payload").filter(|p| p.is_object()) else { return AdapterLine::Skip };
    let Some(title) = text(p, "title") else { return AdapterLine::Idle };
    let is_playing = p.get("playing").and_then(Value::as_bool).unwrap_or(false);
    let elapsed = micros_to_ms(p, "elapsedTimeMicros").unwrap_or(0);
    let sampled_at = p
        .get("timestampEpochMicros")
        .and_then(Value::as_f64)
        .filter(|f| f.is_finite() && *f > 0.0)
        .map(|f| (f / 1000.0) as i64)
        .unwrap_or(now_ms);
    let art = text(p, "artworkData").and_then(|b64| {
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).ok()?;
        let mime = text(p, "artworkMimeType").unwrap_or_else(|| "image/png".into());
        Some((bytes, mime))
    });
    AdapterLine::Now(Box::new(MacNow {
        app_id: text(p, "bundleIdentifier").unwrap_or_default(),
        title,
        artist: text(p, "artist").unwrap_or_default(),
        album: text(p, "album"),
        duration_ms: micros_to_ms(p, "durationMicros").filter(|&d| d > 0),
        position_ms: elapsed,
        sampled_at,
        is_playing,
        art,
        art_url: None,
    }))
}

/// Process names checked with `pgrep -x` before scripting an app, so osascript never launches
/// it or asks where an uninstalled app is.
pub const SCRIPTED_APPS: [(&str, &str); 2] = [("Spotify", SPOTIFY_SCRIPT), ("Music", MUSIC_SCRIPT)];

/// Prints: app, state, title, artist, album, duration (ms), position (s), artwork URL.
pub const SPOTIFY_SCRIPT: &str = r#"tell application "Spotify"
  set st to player state as text
  if st is "stopped" then return ""
  set t to current track
  return "spotify" & tab & st & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((duration of t) as text) & tab & ((player position) as text) & tab & (artwork url of t)
end tell"#;

/// Prints: app, state, title, artist, album, duration (s), position (s), empty artwork URL.
pub const MUSIC_SCRIPT: &str = r#"tell application "Music"
  set st to player state as text
  if st is "stopped" then return ""
  try
    set t to current track
  on error
    return ""
  end try
  return "music" & tab & st & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & ((duration of t) as text) & tab & ((player position) as text) & tab & ""
end tell"#;

/// AppleScript prints reals with the user's decimal separator, so accept "1,5" as well as "1.5".
fn number(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|f| f.is_finite() && *f >= 0.0)
}

/// Parse one line printed by `SPOTIFY_SCRIPT` or `MUSIC_SCRIPT`. `None` for empty or malformed output.
pub fn parse_osascript(out: &str, now_ms: i64) -> Option<MacNow> {
    let line = out.trim_end_matches(['\r', '\n']);
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 8 {
        return None;
    }
    let app = f[0].trim();
    let title = f[2].trim();
    if title.is_empty() || title == "missing value" {
        return None;
    }
    // Spotify reports duration in ms; Music in seconds.
    let duration_ms = number(f[5]).map(|d| if app == "spotify" { d as u64 } else { (d * 1000.0) as u64 }).filter(|&d| d > 0);
    let clean = |s: &str| Some(s.trim().to_string()).filter(|v| !v.is_empty() && v != "missing value");
    Some(MacNow {
        app_id: app.to_string(),
        title: title.to_string(),
        artist: clean(f[3]).unwrap_or_default(),
        album: clean(f[4]),
        duration_ms,
        position_ms: number(f[6]).map(|p| (p * 1000.0) as u64).unwrap_or(0),
        sampled_at: now_ms,
        is_playing: f[1].trim() == "playing",
        art: None,
        art_url: clean(f[7]).filter(|u| u.starts_with("https://")),
    })
}

pub fn session_info(n: &MacNow) -> SessionInfo {
    SessionInfo { app_id: n.app_id.clone(), is_playing: n.is_playing }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_full_payload() {
        let line = r#"{"type":"data","diff":false,"payload":{"bundleIdentifier":"com.spotify.client","title":"Song","artist":"A","album":"Al","durationMicros":200000000,"elapsedTimeMicros":61500000,"timestampEpochMicros":1700000000123000,"playing":true,"artworkData":"AQID","artworkMimeType":"image/jpeg"}}"#;
        let AdapterLine::Now(n) = parse_adapter_line(line, 5) else { panic!("expected Now") };
        assert_eq!(n.app_id, "com.spotify.client");
        assert_eq!(n.title, "Song");
        assert_eq!(n.artist, "A");
        assert_eq!(n.album.as_deref(), Some("Al"));
        assert_eq!(n.duration_ms, Some(200_000));
        assert_eq!(n.position_ms, 61_500);
        assert_eq!(n.sampled_at, 1_700_000_000_123);
        assert!(n.is_playing);
        assert_eq!(n.art, Some((vec![1, 2, 3], "image/jpeg".into())));
        let s = n.snapshot(Some("/art/k".into()));
        assert_eq!(s.track.art_url.as_deref(), Some("/art/k"));
        assert_eq!(s.position_ms, 61_500);
    }

    #[test]
    fn adapter_minimal_payload_uses_defaults() {
        let line = r#"{"type":"data","diff":false,"payload":{"bundleIdentifier":"com.google.Chrome","title":"Video","playing":false}}"#;
        let AdapterLine::Now(n) = parse_adapter_line(line, 42) else { panic!("expected Now") };
        assert_eq!(n.artist, "");
        assert_eq!(n.album, None);
        assert_eq!(n.duration_ms, None);
        assert_eq!(n.position_ms, 0);
        assert_eq!(n.sampled_at, 42);
        assert!(!n.is_playing);
        assert_eq!(n.art, None);
    }

    #[test]
    fn adapter_empty_payload_is_idle() {
        assert_eq!(parse_adapter_line(r#"{"type":"data","diff":false,"payload":{}}"#, 0), AdapterLine::Idle);
        assert_eq!(parse_adapter_line(r#"{"type":"data","diff":false,"payload":{"title":""}}"#, 0), AdapterLine::Idle);
    }

    #[test]
    fn adapter_skips_noise() {
        assert_eq!(parse_adapter_line("", 0), AdapterLine::Skip);
        assert_eq!(parse_adapter_line("not json", 0), AdapterLine::Skip);
        assert_eq!(parse_adapter_line(r#"{"type":"other","payload":{"title":"x"}}"#, 0), AdapterLine::Skip);
        assert_eq!(parse_adapter_line(r#"{"type":"data","payload":null}"#, 0), AdapterLine::Skip);
    }

    #[test]
    fn adapter_bad_artwork_is_dropped() {
        let line = r#"{"type":"data","payload":{"title":"S","playing":true,"artworkData":"%%%"}}"#;
        let AdapterLine::Now(n) = parse_adapter_line(line, 0) else { panic!() };
        assert_eq!(n.art, None);
    }

    #[test]
    fn osascript_spotify() {
        let out = "spotify\tplaying\tSong\tA\tAl\t200000\t61.5\thttps://i.scdn.co/image/x\n";
        let n = parse_osascript(out, 7).unwrap();
        assert_eq!(n.app_id, "spotify");
        assert_eq!(n.duration_ms, Some(200_000));
        assert_eq!(n.position_ms, 61_500);
        assert_eq!(n.sampled_at, 7);
        assert!(n.is_playing);
        assert_eq!(n.art_url.as_deref(), Some("https://i.scdn.co/image/x"));
    }

    #[test]
    fn osascript_music_seconds_and_comma_decimals() {
        let n = parse_osascript("music\tpaused\tSong\tA\tAl\t200,5\t10,25\t", 0).unwrap();
        assert_eq!(n.duration_ms, Some(200_500));
        assert_eq!(n.position_ms, 10_250);
        assert!(!n.is_playing);
        assert_eq!(n.art_url, None);
    }

    #[test]
    fn osascript_missing_values() {
        let n = parse_osascript("music\tplaying\tSong\tmissing value\tmissing value\tmissing value\t0\t", 0).unwrap();
        assert_eq!(n.artist, "");
        assert_eq!(n.album, None);
        assert_eq!(n.duration_ms, None);
        assert!(parse_osascript("music\tplaying\tmissing value\tA\tAl\t1\t0\t", 0).is_none());
    }

    #[test]
    fn osascript_empty_or_short() {
        assert!(parse_osascript("", 0).is_none());
        assert!(parse_osascript("\n", 0).is_none());
        assert!(parse_osascript("spotify\tplaying\tSong", 0).is_none());
        assert!(parse_osascript("spotify\tplaying\t \tA\tAl\t1\t0\t", 0).is_none());
    }
}
