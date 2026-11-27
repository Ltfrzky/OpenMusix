//! User override folder: `<appdata>/lyrics/Artist - Title.lrc`.
use crate::lrc::parse_lrc;
use crate::model::{LyricsResult, Track};
use crate::normalize::{clean_artist, clean_title, match_key};
use std::path::Path;

/// Look for an override file whose stem `Artist - Title` matches the track.
pub fn find_local(dir: &Path, t: &Track) -> Option<LyricsResult> {
    let want_artist = match_key(&clean_artist(&t.artist));
    let want_title = match_key(&clean_title(&t.title, &t.artist));
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lrc")) {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
        let Some((a, ti)) = stem.split_once(" - ") else { continue };
        if match_key(&clean_artist(a)) != want_artist || match_key(&clean_title(ti, a)) != want_title {
            continue;
        }
        let bytes = std::fs::read(&p).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let lines = parse_lrc(&text);
        return Some(if lines.is_empty() {
            LyricsResult::PlainOnly(text.trim_start_matches('\u{feff}').trim().to_string())
        } else {
            LyricsResult::Synced(lines)
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(title: &str, artist: &str) -> Track { Track { title: title.into(), artist: artist.into(), ..Default::default() } }
    #[test] fn local_override_matches_loosely() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("Artist - Song.lrc"), "[00:01.00]hi").unwrap();
        assert!(matches!(find_local(d.path(), &t("Song - Remastered", "artist")), Some(LyricsResult::Synced(_))));
        assert!(find_local(d.path(), &t("Other", "artist")).is_none());
    }
    #[test] fn untimed_file_is_plain() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("A - B.lrc"), "words only").unwrap();
        assert_eq!(find_local(d.path(), &t("B", "A")), Some(LyricsResult::PlainOnly("words only".into())));
    }
    #[test] fn missing_dir_is_none() { assert!(find_local(Path::new("Z:/definitely/missing"), &t("a", "b")).is_none()); }
}
