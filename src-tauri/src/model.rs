//! Core data types shared across the app. JSON is camelCase to match `src/lib/types.ts`.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// The OS media session (Windows GSMTC, Linux MPRIS, macOS Now Playing).
    #[serde(alias = "windows")]
    System,
    Spotify,
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn source_serializes_as_system_and_reads_old_windows_value() {
        assert_eq!(serde_json::to_string(&Source::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::from_str::<Source>("\"windows\"").unwrap(), Source::System);
        assert_eq!(serde_json::from_str::<Source>("\"spotify\"").unwrap(), Source::Spotify);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub art_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub track: Track,
    pub position_ms: u64,
    /// Unix epoch milliseconds at which `position_ms` was true.
    pub sampled_at: i64,
    pub is_playing: bool,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum LyricsResult {
    Synced(Vec<LyricLine>),
    PlainOnly(String),
    Instrumental,
    NotFound,
}
