//! Source precedence: the PC wins; Spotify refines the same track or fills in when the PC is idle.
use super::SourceSnapshot;
use crate::model::{NowPlaying, Source};
use crate::normalize::track_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub now: Option<NowPlaying>,
    /// Whether Spotify's data is on screen (drives its poll cadence).
    pub spotify_used: bool,
}

fn from(s: &SourceSnapshot, source: Source) -> NowPlaying {
    NowPlaying {
        track: s.track.clone(),
        position_ms: s.position_ms,
        sampled_at: s.sampled_at,
        is_playing: s.is_playing,
        source,
    }
}

pub fn merge(win: Option<&SourceSnapshot>, sp: Option<&SourceSnapshot>) -> Merged {
    let sp_playing = sp.filter(|s| s.is_playing);
    if let Some(w) = win.filter(|w| w.is_playing) {
        if let Some(s) = sp_playing.filter(|s| track_key(&s.track) == track_key(&w.track)) {
            let mut n = from(w, Source::Spotify);
            n.position_ms = s.position_ms;
            n.sampled_at = s.sampled_at;
            n.track.duration_ms = s.track.duration_ms.or(w.track.duration_ms);
            n.track.art_url = s.track.art_url.clone().or_else(|| w.track.art_url.clone());
            if n.track.album.is_none() {
                n.track.album = s.track.album.clone();
            }
            return Merged { now: Some(n), spotify_used: true };
        }
        return Merged { now: Some(from(w, Source::System)), spotify_used: false };
    }
    if let Some(s) = sp_playing {
        return Merged { now: Some(from(s, Source::Spotify)), spotify_used: true };
    }
    if let Some(w) = win {
        return Merged { now: Some(from(w, Source::System)), spotify_used: false };
    }
    Merged { now: sp.map(|s| from(s, Source::Spotify)), spotify_used: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Track;
    fn snap(title: &str, playing: bool, pos: u64, dur: Option<u64>) -> SourceSnapshot {
        SourceSnapshot { track: Track { title: title.into(), artist: "A".into(), duration_ms: dur, ..Default::default() },
            position_ms: pos, sampled_at: 1000, is_playing: playing }
    }
    #[test] fn windows_only() {
        let m = merge(Some(&snap("S", true, 5, None)), None);
        assert_eq!(m.now.unwrap().source, Source::System); assert!(!m.spotify_used);
    }
    #[test] fn spotify_refines_same_track() {
        let mut sp = snap("S - Remastered", true, 61_000, Some(200_000));
        sp.track.art_url = Some("https://i.scdn.co/x".into());
        let m = merge(Some(&snap("S", true, 60_500, None)), Some(&sp));
        let n = m.now.unwrap();
        assert_eq!(n.position_ms, 61_000); assert_eq!(n.source, Source::Spotify);
        assert_eq!(n.track.duration_ms, Some(200_000)); assert_eq!(n.track.art_url.as_deref(), Some("https://i.scdn.co/x"));
        assert_eq!(n.track.title, "S"); assert!(m.spotify_used);
    }
    #[test] fn pc_wins_on_different_track() {
        let m = merge(Some(&snap("A", true, 0, None)), Some(&snap("B", true, 0, None)));
        let n = m.now.unwrap(); assert_eq!(n.track.title, "A"); assert_eq!(n.source, Source::System); assert!(!m.spotify_used);
    }
    #[test] fn spotify_when_pc_paused() {
        let m = merge(Some(&snap("A", false, 0, None)), Some(&snap("B", true, 0, None)));
        let n = m.now.unwrap(); assert_eq!(n.track.title, "B"); assert_eq!(n.source, Source::Spotify); assert!(m.spotify_used);
    }
    #[test] fn paused_fallbacks() {
        assert_eq!(merge(Some(&snap("A", false, 0, None)), Some(&snap("B", false, 0, None))).now.unwrap().track.title, "A");
        assert_eq!(merge(None, Some(&snap("B", false, 0, None))).now.unwrap().source, Source::Spotify);
    }
    #[test] fn nothing() { assert!(merge(None, None).now.is_none()); }
}
