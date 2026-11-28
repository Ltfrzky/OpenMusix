//! Pure mapping from MPRIS player properties to a snapshot. Kept free of D-Bus types so it is
//! compiled and tested on every platform; only `linux.rs` uses it at runtime.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
use super::SourceSnapshot;
use crate::model::Track;

/// The parts of `org.mpris.MediaPlayer2.Player` we read, already pulled out of D-Bus variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisState {
    /// `PlaybackStatus`: "Playing", "Paused" or "Stopped".
    pub status: String,
    pub title: Option<String>,
    /// `xesam:artist` is a list.
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// `mpris:length`, microseconds.
    pub length_us: Option<i64>,
    /// `Position`, microseconds.
    pub position_us: Option<i64>,
    pub art_url: Option<String>,
}

impl MprisState {
    pub fn is_playing(&self) -> bool {
        self.status == "Playing"
    }
}

fn us_to_ms(us: i64) -> u64 {
    (us.max(0) / 1000) as u64
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Identity of the playing item; art is re-read only when it changes.
pub fn track_id(st: &MprisState) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        st.title.as_deref().unwrap_or_default(),
        st.artists.join(", "),
        st.album.as_deref().unwrap_or_default()
    )
}

/// `None` when the player has no title (idle, or between tracks).
pub fn snapshot(st: &MprisState, art_url: Option<String>, now_ms: i64) -> Option<SourceSnapshot> {
    let title = non_empty(&st.title)?;
    let artist = st.artists.iter().map(|a| a.trim()).filter(|a| !a.is_empty()).collect::<Vec<_>>().join(", ");
    let duration_ms = st.length_us.map(us_to_ms).filter(|&d| d > 0);
    Some(SourceSnapshot {
        track: Track { title, artist, album: non_empty(&st.album), duration_ms, art_url },
        position_ms: st.position_us.map(us_to_ms).unwrap_or(0),
        sampled_at: now_ms,
        is_playing: st.is_playing(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtLocation {
    File(std::path::PathBuf),
    Http(String),
}

/// Where `mpris:artUrl` points. Other schemes (e.g. `data:`) are ignored.
pub fn art_location(url: &str) -> Option<ArtLocation> {
    let u = url::Url::parse(url.trim()).ok()?;
    match u.scheme() {
        "file" => u.to_file_path().ok().map(ArtLocation::File),
        "http" | "https" => Some(ArtLocation::Http(u.to_string())),
        _ => None,
    }
}

/// Guess an image MIME type from a path's extension.
pub fn mime_for_path(p: &std::path::Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> MprisState {
        MprisState {
            status: "Playing".into(),
            title: Some("Song".into()),
            artists: vec!["A".into(), "B".into()],
            album: Some("Album".into()),
            length_us: Some(200_000_000),
            position_us: Some(61_500_000),
            art_url: None,
        }
    }

    #[test]
    fn maps_fields_and_units() {
        let s = snapshot(&st(), Some("/art/k".into()), 1234).unwrap();
        assert_eq!(s.track.title, "Song");
        assert_eq!(s.track.artist, "A, B");
        assert_eq!(s.track.album.as_deref(), Some("Album"));
        assert_eq!(s.track.duration_ms, Some(200_000));
        assert_eq!(s.track.art_url.as_deref(), Some("/art/k"));
        assert_eq!(s.position_ms, 61_500);
        assert_eq!(s.sampled_at, 1234);
        assert!(s.is_playing);
    }

    #[test]
    fn paused_and_stopped_are_not_playing() {
        let mut p = st();
        p.status = "Paused".into();
        assert!(!snapshot(&p, None, 0).unwrap().is_playing);
        p.status = "Stopped".into();
        assert!(!snapshot(&p, None, 0).unwrap().is_playing);
    }

    #[test]
    fn empty_title_is_nothing() {
        let mut p = st();
        p.title = Some("  ".into());
        assert!(snapshot(&p, None, 0).is_none());
        p.title = None;
        assert!(snapshot(&p, None, 0).is_none());
    }

    #[test]
    fn missing_or_bad_length_and_position() {
        let mut p = st();
        p.length_us = Some(0);
        p.position_us = Some(-5);
        let s = snapshot(&p, None, 0).unwrap();
        assert_eq!(s.track.duration_ms, None);
        assert_eq!(s.position_ms, 0);
        p.length_us = None;
        p.position_us = None;
        assert_eq!(snapshot(&p, None, 0).unwrap().track.duration_ms, None);
    }

    #[test]
    fn blank_album_and_artists_are_dropped() {
        let mut p = st();
        p.album = Some("".into());
        p.artists = vec!["".into(), "A".into()];
        let s = snapshot(&p, None, 0).unwrap();
        assert_eq!(s.track.album, None);
        assert_eq!(s.track.artist, "A");
    }

    #[test]
    fn track_id_changes_with_identity_only() {
        let a = st();
        let mut b = st();
        b.position_us = Some(1);
        b.status = "Paused".into();
        assert_eq!(track_id(&a), track_id(&b));
        b.title = Some("Other".into());
        assert_ne!(track_id(&a), track_id(&b));
    }

    #[test]
    fn art_locations() {
        assert_eq!(art_location("https://i.scdn.co/image/x"), Some(ArtLocation::Http("https://i.scdn.co/image/x".into())));
        assert!(matches!(art_location("http://localhost/a.png"), Some(ArtLocation::Http(_))));
        assert_eq!(art_location("data:image/png;base64,AAAA"), None);
        assert_eq!(art_location("not a url"), None);
    }

    #[cfg(unix)]
    #[test]
    fn file_art_url_is_decoded() {
        assert_eq!(
            art_location("file:///tmp/cover%20art.jpg"),
            Some(ArtLocation::File(std::path::PathBuf::from("/tmp/cover art.jpg")))
        );
    }

    #[test]
    fn mime_from_extension() {
        assert_eq!(mime_for_path(std::path::Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_for_path(std::path::Path::new("a.webp")), "image/webp");
        assert_eq!(mime_for_path(std::path::Path::new("a")), "image/png");
    }
}
