pub mod merger;
pub mod spotify;
pub mod windows;

use crate::model::Track;

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
    Windows(Option<SourceSnapshot>),
    Spotify(Option<SourceSnapshot>),
}
