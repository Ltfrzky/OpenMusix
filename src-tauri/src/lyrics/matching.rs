//! Wrong-lyrics guard: decides whether an LRCLIB record belongs to the playing track.
use crate::lrc::parse_lrc;
use crate::model::{LyricsResult, Track};
use crate::normalize::{clean_artist, clean_title, match_key};
use serde::Deserialize;

/// One record as returned by LRCLIB `/api/get` and `/api/search`. `duration` is seconds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LrclibRecord {
    pub id: i64,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub instrumental: bool,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

const DURATION_TOLERANCE_MS: f64 = 3000.0;

fn title_key(title: &str, artist: &str) -> String {
    match_key(&clean_title(title, artist))
}
fn artist_key(artist: &str) -> String {
    match_key(&clean_artist(artist))
}

/// True when `r` may be shown for `q`. See the plan's Task 4 guard rules.
pub fn passes_guard(q: &Track, r: &LrclibRecord, from_search: bool) -> bool {
    let qt = title_key(&q.title, &q.artist);
    let rt = title_key(&r.track_name, &r.artist_name);
    let qa = artist_key(&q.artist);
    let ra = artist_key(&r.artist_name);
    match q.duration_ms {
        Some(d) => {
            if (r.duration * 1000.0 - d as f64).abs() > DURATION_TOLERANCE_MS {
                return false;
            }
            if from_search {
                return qt == rt && !qa.is_empty() && !ra.is_empty() && (qa.contains(&ra) || ra.contains(&qa));
            }
            true
        }
        None => qt == rt && qa == ra,
    }
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Convert a record to the result it represents.
pub fn to_result(r: &LrclibRecord) -> LyricsResult {
    if let Some(s) = non_empty(&r.synced_lyrics) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return LyricsResult::Synced(lines);
        }
    }
    if let Some(p) = non_empty(&r.plain_lyrics) {
        return LyricsResult::PlainOnly(p.to_string());
    }
    if r.instrumental {
        return LyricsResult::Instrumental;
    }
    LyricsResult::NotFound
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn trk(title: &str, artist: &str, dur: Option<u64>) -> Track {
        Track { title: title.into(), artist: artist.into(), duration_ms: dur, ..Default::default() }
    }
    pub fn rec(title: &str, artist: &str, dur: f64) -> LrclibRecord {
        LrclibRecord { id: 1, track_name: title.into(), artist_name: artist.into(), album_name: None, duration: dur,
            instrumental: false, plain_lyrics: Some("a".into()), synced_lyrics: Some("[00:01.00]a".into()) }
    }

    #[test] fn rejects_4s_off() { assert!(!passes_guard(&trk("Song", "A", Some(200_000)), &rec("Song", "A", 204.5), false)); }
    #[test] fn accepts_3s_off() { assert!(passes_guard(&trk("Song", "A", Some(200_000)), &rec("Song", "A", 203.0), false)); }
    #[test] fn unknown_duration_needs_exact_names() {
        assert!(passes_guard(&trk("Song (Official Video)", "ArtistVEVO", None), &rec("Song", "Artist", 999.0), false));
        assert!(!passes_guard(&trk("Song", "Artist", None), &rec("Song 2", "Artist", 200.0), false));
    }
    #[test] fn search_requires_title_match() { assert!(!passes_guard(&trk("Song", "A", Some(200_000)), &rec("Other", "A", 200.0), true)); }
    #[test] fn search_artist_containment() {
        assert!(passes_guard(&trk("Song", "Artist", Some(200_000)), &rec("Song", "Artist, Guest", 200.0), true));
        assert!(!passes_guard(&trk("Song", "Artist", Some(200_000)), &rec("Song", "Someone Else", 200.0), true));
    }
    #[test] fn to_result_prefers_synced_then_plain() {
        assert!(matches!(to_result(&rec("S", "A", 1.0)), LyricsResult::Synced(_)));
        let mut r = rec("S", "A", 1.0); r.synced_lyrics = Some("  ".into());
        assert_eq!(to_result(&r), LyricsResult::PlainOnly("a".into()));
        r.plain_lyrics = None; r.instrumental = true;
        assert_eq!(to_result(&r), LyricsResult::Instrumental);
        r.instrumental = false;
        assert_eq!(to_result(&r), LyricsResult::NotFound);
    }
}
