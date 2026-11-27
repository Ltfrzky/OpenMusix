//! Shared app context and the playback → lyrics → hub pipeline.
use crate::commands::project_settings;
use crate::hub::Hub;
use crate::lyrics::{LyricsService, LyricsStatus};
use crate::normalize::track_key;
use crate::playback::{merger::merge, PlaybackEvent, SourceSnapshot};
use crate::server::{start_server, OAuthCallback, ServerStatus, Shutdown};
use crate::settings::SettingsStore;
use std::sync::Arc;
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

/// Consume playback events forever, keeping the hub's now-playing and lyrics current.
pub async fn run_pipeline(ctx: Arc<AppCtx>, mut rx: mpsc::Receiver<PlaybackEvent>) {
    let mut win: Option<SourceSnapshot> = None;
    let mut sp: Option<SourceSnapshot> = None;
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
        let changed = ctx.hub.current().track_key != key;
        let settings = ctx.settings.get();
        ctx.hub.modify(|snap| {
            snap.now_playing = merged.now.clone();
            if changed {
                snap.track_key = key.clone();
                snap.lyrics = LyricsStatus::Loading;
            }
            project_settings(&settings, snap);
        });
        if changed {
            if let Some(now) = merged.now {
                let hub = ctx.hub.clone();
                let lyrics = ctx.lyrics.clone();
                let key = key.unwrap_or_default();
                tokio::spawn(async move {
                    let h2 = hub.clone();
                    let current = move || h2.current().track_key.unwrap_or_default();
                    if let Some(status) = lyrics.resolve_for(now.track, current).await {
                        hub.modify(|s| {
                            if s.track_key.as_deref() == Some(key.as_str()) {
                                s.lyrics = status;
                            }
                        });
                    }
                });
            }
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
