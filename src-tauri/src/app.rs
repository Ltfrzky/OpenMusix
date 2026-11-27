//! Shared app context and the playback → lyrics → hub pipeline.
use crate::commands::project_settings;
use crate::hub::Hub;
use crate::lyrics::{LyricsService, LyricsStatus};
use crate::normalize::track_key;
use crate::playback::{merger::merge, PlaybackEvent, SourceSnapshot};
use crate::server::{start_server, OAuthCallback, ServerStatus, Shutdown};
use crate::settings::SettingsStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch, Mutex, Notify};

pub struct AppCtx {
    pub settings: SettingsStore,
    pub hub: Arc<Hub>,
    pub lyrics: Arc<LyricsService>,
    pub server: Mutex<(ServerStatus, Option<Shutdown>)>,
    pub oauth: Arc<dyn OAuthCallback>,
    pub spotify: Arc<crate::spotify_auth::SpotifyAuth>,
    pub playback_tx: mpsc::Sender<PlaybackEvent>,
    /// Whether Spotify data is on screen; read by the Spotify poller for its cadence.
    pub spotify_used: watch::Sender<bool>,
    /// Poked whenever the Windows source reports a change so Spotify re-polls at once.
    pub windows_changed: Arc<Notify>,
}

impl AppCtx {
    /// Re-project settings into the hub (mode, styles, offsets, flags).
    pub fn sync_settings(&self) {
        let s = self.settings.get();
        self.hub.modify(|snap| project_settings(&s, snap));
    }

    /// (Re)start the overlay server on the configured port and broadcast its status.
    pub async fn restart_server(&self, app: &AppHandle) -> ServerStatus {
        let mut guard = self.server.lock().await;
        guard.1.take(); // dropping the Shutdown stops the old listener
        // Give the OS a moment to release the old socket before rebinding the same port.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let port = self.settings.get().port;
        let (status, sd) = start_server(port, self.hub.clone(), self.oauth.clone()).await;
        *guard = (status.clone(), sd);
        let _ = app.emit("server-status", &status);
        status
    }
}

const DURATION_TOLERANCE_MS: u64 = 3000;
/// Backoff between lyrics retries after a network error (Wi-Fi not up yet, LRCLIB timeout).
const RETRY_DELAYS: [Duration; 3] = [Duration::from_secs(5), Duration::from_secs(15), Duration::from_secs(45)];

/// Whether the playing track needs a (new) lyrics lookup: a new track, or the same track
/// whose duration just became known or changed beyond the guard (late GSMTC/Spotify data).
pub fn needs_resolve(prev: Option<&(String, Option<u64>)>, key: &str, dur: Option<u64>) -> bool {
    match prev {
        None => true,
        Some((k, d)) => {
            k != key
                || match (d, dur) {
                    (None, Some(_)) => true,
                    (Some(a), Some(b)) => a.abs_diff(b) > DURATION_TOLERANCE_MS,
                    _ => false,
                }
        }
    }
}

/// Consume playback events forever, keeping the hub's now-playing and lyrics current.
pub async fn run_pipeline(ctx: Arc<AppCtx>, mut rx: mpsc::Receiver<PlaybackEvent>) {
    let mut win: Option<SourceSnapshot> = None;
    let mut sp: Option<SourceSnapshot> = None;
    let mut resolved: Option<(String, Option<u64>)> = None;
    // Bumped for every lookup; a lookup may publish only while it is still the latest.
    let generation = Arc::new(AtomicU64::new(0));
    while let Some(ev) = rx.recv().await {
        match ev {
            PlaybackEvent::Windows(s) => {
                win = s;
                ctx.windows_changed.notify_one();
            }
            PlaybackEvent::Spotify(s) => sp = s,
        }
        let merged = merge(win.as_ref(), sp.as_ref());
        let _ = ctx.spotify_used.send(merged.spotify_used);
        let key = merged.now.as_ref().map(|n| track_key(&n.track));
        let duration = merged.now.as_ref().and_then(|n| n.track.duration_ms);
        let lookup = match &key {
            Some(k) => needs_resolve(resolved.as_ref(), k, duration),
            None => false,
        };
        if key.is_none() {
            resolved = None;
        }
        let settings = ctx.settings.get();
        ctx.hub.modify(|snap| {
            snap.now_playing = merged.now.clone();
            if snap.track_key != key || lookup {
                snap.track_key = key.clone();
                snap.lyrics = LyricsStatus::Loading;
            }
            project_settings(&settings, snap);
        });
        if lookup {
            let (Some(now), Some(key)) = (merged.now, key) else { continue };
            resolved = Some((key, duration));
            let my_gen = generation.fetch_add(1, Ordering::SeqCst) + 1;
            let hub = ctx.hub.clone();
            let lyrics = ctx.lyrics.clone();
            let latest = generation.clone();
            tokio::spawn(async move {
                let is_current = || latest.load(Ordering::SeqCst) == my_gen;
                if let Some(status) = lyrics.resolve_until(&now.track, is_current, &RETRY_DELAYS).await {
                    hub.modify(|s| {
                        if latest.load(Ordering::SeqCst) == my_gen {
                            s.lyrics = status;
                        }
                    });
                }
            });
        }
    }
}

/// Mirror every hub change to the control panel as a Tauri event.
pub async fn forward_to_panel(app: AppHandle, hub: Arc<Hub>) {
    let mut rx = hub.subscribe();
    loop {
        let snap = rx.borrow_and_update().clone();
        let _ = app.emit("snapshot", &snap);
        if rx.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prev(k: &str, d: Option<u64>) -> (String, Option<u64>) { (k.to_string(), d) }
    #[test] fn first_track_resolves() { assert!(needs_resolve(None, "a|b", None)); }
    #[test] fn same_track_same_duration_does_not() { assert!(!needs_resolve(Some(&prev("a|b", Some(200_000))), "a|b", Some(201_000))); }
    #[test] fn key_change_resolves() { assert!(needs_resolve(Some(&prev("a|b", Some(200_000))), "a|c", Some(200_000))); }
    #[test] fn late_duration_resolves() { assert!(needs_resolve(Some(&prev("a|b", None)), "a|b", Some(200_000))); }
    #[test] fn duration_jump_resolves() { assert!(needs_resolve(Some(&prev("a|b", Some(200_000))), "a|b", Some(262_000))); }
    #[test] fn losing_duration_does_not() { assert!(!needs_resolve(Some(&prev("a|b", Some(200_000))), "a|b", None)); }
}
