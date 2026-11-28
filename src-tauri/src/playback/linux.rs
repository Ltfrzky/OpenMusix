//! Linux media source: polls MPRIS players on the session D-Bus.
use super::mpris_parse::{art_location, mime_for_path, snapshot, track_id, ArtLocation, MprisState};
use super::system::{art_from_bytes, changed_beyond_drift, fetch_http_art, now_unix_ms, pick_session, SessionInfo, MAX_ART_BYTES, RESCAN_INTERVAL};
use super::{PlaybackEvent, SourceSnapshot};
use crate::hub::{Art, Hub};
use mpris::{PlaybackStatus, Player, PlayerFinder};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// MPRIS has no position-changed signal, so the reader polls. Faster than the rescan used on
/// Windows keeps seeks and track changes snappy; each poll is a few local D-Bus calls.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Spawn the reader thread. `mpris` players are not `Send`, so they live on this thread only.
pub fn spawn_linux_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("mpris".into())
        .spawn(move || run(tx, hub))
        .expect("spawn mpris thread");
}

fn status_text(s: PlaybackStatus) -> &'static str {
    match s {
        PlaybackStatus::Playing => "Playing",
        PlaybackStatus::Paused => "Paused",
        PlaybackStatus::Stopped => "Stopped",
    }
}

fn read_state(p: &Player) -> Option<MprisState> {
    let status = p.get_playback_status().ok()?;
    let md = p.get_metadata().ok()?;
    Some(MprisState {
        status: status_text(status).into(),
        title: md.title().map(String::from),
        artists: md.artists().unwrap_or_default().into_iter().map(String::from).collect(),
        album: md.album_name().map(String::from),
        length_us: md.length_in_microseconds().map(|v| v.min(i64::MAX as u64) as i64),
        position_us: p.get_position_in_microseconds().ok().map(|v| v.min(i64::MAX as u64) as i64),
        art_url: md.art_url().map(String::from),
    })
}

fn fetch_art(url: &str) -> Option<Art> {
    match art_location(url)? {
        ArtLocation::File(path) => {
            let meta = std::fs::metadata(&path).ok()?;
            if meta.len() as usize > MAX_ART_BYTES {
                return None;
            }
            art_from_bytes(std::fs::read(&path).ok()?, mime_for_path(&path))
        }
        ArtLocation::Http(url) => fetch_http_art(&url),
    }
}

fn run(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    let finder = loop {
        match PlayerFinder::new() {
            Ok(f) => break f,
            Err(e) => {
                tracing::warn!("MPRIS unavailable, retrying: {e}");
                std::thread::sleep(RESCAN_INTERVAL * 4);
            }
        }
    };
    let mut current_bus: Option<String> = None;
    let mut last_track_id = String::new();
    let mut last_sent: Option<Option<SourceSnapshot>> = None;
    loop {
        let players = finder.find_all().unwrap_or_else(|e| {
            tracing::warn!("MPRIS player scan failed: {e}");
            Vec::new()
        });
        let states: Vec<Option<MprisState>> = players.iter().map(read_state).collect();
        let infos: Vec<SessionInfo> = players
            .iter()
            .zip(&states)
            .map(|(p, s)| SessionInfo {
                app_id: p.bus_name().to_string(),
                is_playing: s.as_ref().is_some_and(MprisState::is_playing),
            })
            .collect();
        let current = current_bus.as_ref().and_then(|b| infos.iter().position(|i| &i.app_id == b));
        let chosen = pick_session(&infos, current);
        current_bus = chosen.map(|i| infos[i].app_id.clone());

        let snap = chosen.and_then(|i| states[i].as_ref()).and_then(|st| {
            let id = track_id(st);
            if id != last_track_id {
                last_track_id = id;
                hub.set_art(st.art_url.as_deref().and_then(fetch_art));
            }
            let art_url = hub.current_art_key().map(|k| format!("/art/{k}"));
            snapshot(st, art_url, now_unix_ms())
        });
        if snap.is_none() {
            last_track_id.clear();
            hub.set_art(None);
        }
        // Position moves every poll while playing; only send when something other than the
        // expected drift changed, or the merger would see a stream of identical tracks.
        if changed_beyond_drift(last_sent.as_ref().and_then(|s| s.as_ref()), snap.as_ref()) || last_sent.is_none() {
            last_sent = Some(snap.clone());
            if tx.blocking_send(PlaybackEvent::System(snap)).is_err() {
                return;
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
