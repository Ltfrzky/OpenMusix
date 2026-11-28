pub mod merger;
pub mod spotify;
pub mod system;
pub mod mpris_parse;
pub mod macos_parse;
#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

use crate::hub::Hub;
use crate::model::Track;
use std::sync::Arc;
use tokio::sync::mpsc;

/// One reading from a playback source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub track: Track,
    pub position_ms: u64,
    pub sampled_at: i64,
    pub is_playing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackEvent {
    /// The OS media session.
    System(Option<SourceSnapshot>),
    Spotify(Option<SourceSnapshot>),
}

/// Start the OS media source for this platform. It sends `PlaybackEvent::System` on change.
#[cfg(windows)]
pub fn spawn_system_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    windows::spawn_windows_source(tx, hub);
}

#[cfg(target_os = "linux")]
pub fn spawn_system_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    linux::spawn_linux_source(tx, hub);
}

#[cfg(target_os = "macos")]
pub fn spawn_system_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    macos::spawn_macos_source(tx, hub);
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn spawn_system_source(_tx: mpsc::Sender<PlaybackEvent>, _hub: Arc<Hub>) {
    tracing::warn!("no OS media source on this platform yet; only Spotify is available");
}
