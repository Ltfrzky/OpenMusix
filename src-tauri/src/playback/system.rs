//! Platform-neutral pieces shared by the OS media sources (Windows GSMTC, Linux MPRIS, macOS).
use super::SourceSnapshot;
use crate::hub::Art;
use sha2::{Digest, Sha256};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, SystemTime};

/// Largest cover image accepted from a media source.
pub const MAX_ART_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub app_id: String,
    pub is_playing: bool,
}

fn is_spotify(s: &SessionInfo) -> bool {
    s.app_id.to_lowercase().contains("spotify")
}

/// Spotify & playing → any playing → Spotify → current → first.
pub fn pick_session(sessions: &[SessionInfo], current: Option<usize>) -> Option<usize> {
    let find = |f: &dyn Fn(&SessionInfo) -> bool| sessions.iter().position(f);
    find(&|s| is_spotify(s) && s.is_playing)
        .or_else(|| find(&|s| s.is_playing))
        .or_else(|| find(&is_spotify))
        .or(current.filter(|&i| i < sessions.len()))
        .or(if sessions.is_empty() { None } else { Some(0) })
}

/// How often a source re-checks all players even without events. The OS only reports
/// changes for the player we subscribe to, so another player that starts or resumes
/// would otherwise go unnoticed.
pub const RESCAN_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, PartialEq, Eq)]
pub enum Wake {
    Event,
    Rescan,
    Closed,
}

/// Block until an event arrives or `timeout` passes.
pub fn wait_for_wake(rx: &std_mpsc::Receiver<()>, timeout: Duration) -> Wake {
    match rx.recv_timeout(timeout) {
        Ok(()) => Wake::Event,
        Err(std_mpsc::RecvTimeoutError::Timeout) => Wake::Rescan,
        Err(std_mpsc::RecvTimeoutError::Disconnected) => Wake::Closed,
    }
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Wrap cover bytes for the hub, keyed by content hash. Empty or oversized images are dropped.
pub fn art_from_bytes(bytes: Vec<u8>, mime: &str) -> Option<Art> {
    if bytes.is_empty() || bytes.len() > MAX_ART_BYTES {
        return None;
    }
    let h = Sha256::digest(&bytes);
    let key: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let mime = if mime.trim().is_empty() { "image/png".to_string() } else { mime.to_string() };
    Some(Art { key, mime, bytes })
}

/// Download a cover image (blocking; call from a source thread, not the async runtime).
#[cfg_attr(windows, allow(dead_code))]
pub fn fetch_http_art(url: &str) -> Option<Art> {
    let url = url.to_string();
    tauri::async_runtime::block_on(async move {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?;
        let resp = client.get(&url).send().await.ok()?.error_for_status().ok()?;
        if resp.content_length().is_some_and(|n| n as usize > MAX_ART_BYTES) {
            return None;
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();
        let bytes = resp.bytes().await.ok()?;
        art_from_bytes(bytes.to_vec(), &mime)
    })
}

/// Whether `new` differs from `old` by more than normal playback progress (1.5 s tolerance).
/// Polling sources use it so a steadily advancing position is not re-sent every poll.
#[cfg_attr(windows, allow(dead_code))]
pub fn changed_beyond_drift(old: Option<&SourceSnapshot>, new: Option<&SourceSnapshot>) -> bool {
    match (old, new) {
        (None, None) => false,
        (Some(a), Some(b)) => {
            if a.track != b.track || a.is_playing != b.is_playing {
                return true;
            }
            let expected = if a.is_playing {
                a.position_ms as i64 + (b.sampled_at - a.sampled_at)
            } else {
                a.position_ms as i64
            };
            (b.position_ms as i64 - expected).abs() > 1500
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn si(id: &str, playing: bool) -> SessionInfo { SessionInfo { app_id: id.into(), is_playing: playing } }
    #[test] fn prefers_playing_session_over_paused_spotify() {
        let s = [si("Spotify.exe", false), si("chrome", true)];
        assert_eq!(pick_session(&s, Some(0)), Some(1));
    }
    #[test] fn prefers_playing_spotify() {
        let s = [si("chrome", true), si("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", true)];
        assert_eq!(pick_session(&s, None), Some(1));
    }
    #[test] fn falls_back_to_current() { assert_eq!(pick_session(&[si("a", false), si("b", false)], Some(1)), Some(1)); }
    #[test] fn paused_spotify_beats_other_paused() { assert_eq!(pick_session(&[si("a", false), si("Spotify.exe", false)], Some(0)), Some(1)); }
    #[test] fn empty() { assert_eq!(pick_session(&[], None), None); }
    #[test] fn matches_mpris_bus_names() {
        let s = [si("org.mpris.MediaPlayer2.firefox.instance_1_42", true), si("org.mpris.MediaPlayer2.spotify", true)];
        assert_eq!(pick_session(&s, None), Some(1));
    }

    #[test] fn art_keys_by_content_and_defaults_mime() {
        let a = art_from_bytes(vec![1, 2, 3], "").unwrap();
        assert_eq!(a.mime, "image/png");
        assert_eq!(a.key.len(), 16);
        assert_eq!(art_from_bytes(vec![1, 2, 3], "image/jpeg").unwrap().key, a.key);
        assert_ne!(art_from_bytes(vec![1, 2, 4], "").unwrap().key, a.key);
    }
    #[test] fn art_rejects_empty_and_oversized() {
        assert!(art_from_bytes(vec![], "image/png").is_none());
        assert!(art_from_bytes(vec![0; MAX_ART_BYTES + 1], "image/png").is_none());
    }

    fn snap(title: &str, pos: u64, at: i64, playing: bool) -> SourceSnapshot {
        SourceSnapshot {
            track: crate::model::Track { title: title.into(), ..Default::default() },
            position_ms: pos,
            sampled_at: at,
            is_playing: playing,
        }
    }
    #[test] fn drift_ignores_normal_progress() {
        let a = snap("S", 10_000, 1_000, true);
        assert!(!changed_beyond_drift(Some(&a), Some(&snap("S", 11_000, 2_000, true))));
        assert!(!changed_beyond_drift(Some(&a), Some(&snap("S", 12_200, 2_000, true))));
    }
    #[test] fn drift_catches_seek_track_and_state() {
        let a = snap("S", 10_000, 1_000, true);
        assert!(changed_beyond_drift(Some(&a), Some(&snap("S", 40_000, 2_000, true))));
        assert!(changed_beyond_drift(Some(&a), Some(&snap("T", 11_000, 2_000, true))));
        assert!(changed_beyond_drift(Some(&a), Some(&snap("S", 11_000, 2_000, false))));
    }
    #[test] fn drift_paused_position_must_hold() {
        let a = snap("S", 10_000, 1_000, false);
        assert!(!changed_beyond_drift(Some(&a), Some(&snap("S", 10_000, 9_000, false))));
        assert!(changed_beyond_drift(Some(&a), Some(&snap("S", 30_000, 9_000, false))));
    }
    #[test] fn drift_presence_changes() {
        let a = snap("S", 0, 0, true);
        assert!(changed_beyond_drift(None, Some(&a)));
        assert!(changed_beyond_drift(Some(&a), None));
        assert!(!changed_beyond_drift(None, None));
    }

    #[test] fn wait_returns_on_timeout_without_events() {
        let (_tx, rx) = std_mpsc::channel::<()>();
        let t = std::time::Instant::now();
        assert_eq!(wait_for_wake(&rx, Duration::from_millis(40)), Wake::Rescan);
        assert!(t.elapsed() < Duration::from_millis(500));
    }
    #[test] fn wait_returns_on_event() {
        let (tx, rx) = std_mpsc::channel::<()>();
        tx.send(()).unwrap();
        assert_eq!(wait_for_wake(&rx, Duration::from_secs(5)), Wake::Event);
    }
    #[test] fn wait_reports_closed() {
        let (tx, rx) = std_mpsc::channel::<()>();
        drop(tx);
        assert_eq!(wait_for_wake(&rx, Duration::from_secs(5)), Wake::Closed);
    }
}
