pub mod cache;
pub mod local;
pub mod lrclib;
pub mod matching;

use crate::model::{LyricsResult, Track};
use crate::normalize::track_key;
use serde::Serialize;
use std::path::PathBuf;
use std::time::SystemTime;

pub use lrclib::{LrclibClient, LyricsError};

/// What the hub publishes for the current track. `Error` renders like NotFound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "result", rename_all = "camelCase")]
pub enum LyricsStatus {
    Loading,
    Ready(LyricsResult),
    Error,
}

pub struct LyricsService {
    local_dir: PathBuf,
    pub(crate) cache: cache::LyricsCache,
    client: LrclibClient,
}

impl LyricsService {
    pub fn new(local_dir: PathBuf, cache_dir: PathBuf, client: LrclibClient) -> Self {
        Self { local_dir, cache: cache::LyricsCache::new(cache_dir), client }
    }

    pub fn local_dir(&self) -> &std::path::Path {
        &self.local_dir
    }

    pub fn clear_cache(&self) -> std::io::Result<()> {
        self.cache.clear()
    }

    /// Local override, then cache, then LRCLIB. Only successful lookups are cached.
    pub async fn resolve(&self, t: &Track) -> Result<LyricsResult, LyricsError> {
        if let Some(r) = local::find_local(&self.local_dir, t) {
            return Ok(r);
        }
        let key = track_key(t);
        if let Some(r) = self.cache.get(&key, SystemTime::now()) {
            return Ok(r);
        }
        let r = self.client.lookup(t).await?;
        self.cache.put(&key, &r, SystemTime::now());
        Ok(r)
    }

    /// Resolve, then return `None` if the playing track changed meanwhile.
    pub async fn resolve_for(&self, t: Track, current_key: impl Fn() -> String) -> Option<LyricsStatus> {
        let res = self.resolve(&t).await;
        if track_key(&t) != current_key() {
            return None;
        }
        Some(match res {
            Ok(r) => LyricsStatus::Ready(r),
            Err(e) => {
                tracing::warn!("{e}");
                LyricsStatus::Error
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LyricsResult, Track};
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn song() -> Track { Track { title: "Song".into(), artist: "Artist".into(), duration_ms: Some(200_000), ..Default::default() } }
    fn svc(root: &std::path::Path, base: &str) -> LyricsService {
        LyricsService::new(root.join("lyrics"), root.join("cache"), lrclib::LrclibClient::new(Url::parse(base).unwrap()))
    }

    #[tokio::test]
    async fn local_override_wins_over_cache() {
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), "http://127.0.0.1:9");
        s.cache.put(&crate::normalize::track_key(&song()), &LyricsResult::NotFound, std::time::SystemTime::now());
        std::fs::create_dir_all(d.path().join("lyrics")).unwrap();
        std::fs::write(d.path().join("lyrics/Artist - Song.lrc"), "[00:01.00]mine").unwrap();
        assert!(matches!(s.resolve(&song()).await, Ok(LyricsResult::Synced(_))));
    }

    #[tokio::test]
    async fn network_error_not_cached() {
        let m = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).respond_with(ResponseTemplate::new(503)).mount(&m).await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &m.uri());
        assert!(s.resolve(&song()).await.is_err());
        let n = std::fs::read_dir(d.path().join("cache")).map(|r| r.count()).unwrap_or(0);
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn found_result_is_cached() {
        let m = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).respond_with(ResponseTemplate::new(404)).expect(1).mount(&m).await;
        Mock::given(method("GET")).and(path("/api/search")).respond_with(ResponseTemplate::new(200).set_body_string("[]")).expect(1).mount(&m).await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &m.uri());
        assert_eq!(s.resolve(&song()).await, Ok(LyricsResult::NotFound));
        assert_eq!(s.resolve(&song()).await, Ok(LyricsResult::NotFound)); // second from cache
    }

    #[tokio::test]
    async fn stale_result_is_discarded() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("lyrics")).unwrap();
        std::fs::write(d.path().join("lyrics/Artist - Song.lrc"), "[00:01.00]x").unwrap();
        let s = svc(d.path(), "http://127.0.0.1:9");
        assert_eq!(s.resolve_for(song(), || "b|other".into()).await, None);
        assert!(matches!(s.resolve_for(song(), || "artist|song".into()).await, Some(LyricsStatus::Ready(_))));
    }
}
