//! LRCLIB HTTP client. Lookup = `/api/get`, then `/api/search`, each filtered by the guard.
use super::matching::{passes_guard, to_result, LrclibRecord};
use crate::model::{LyricsResult, Track};
use crate::normalize::{clean_artist, clean_title};
use std::time::Duration;
use url::Url;

pub const DEFAULT_BASE: &str = "https://lrclib.net";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LyricsError {
    /// Transport error, timeout, 5xx or an unreadable body. Never cached.
    #[error("lyrics lookup failed: {0}")]
    Network(String),
}

pub fn user_agent() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY");
    if repo.is_empty() {
        format!("OpenMusix/{}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("OpenMusix/{} (+{repo})", env!("CARGO_PKG_VERSION"))
    }
}

pub struct LrclibClient {
    base: Url,
    http: reqwest::Client,
}

fn rank(q: &Track, r: &LrclibRecord) -> (u8, u64) {
    let kind = match to_result(r) {
        LyricsResult::Synced(_) => 0,
        LyricsResult::PlainOnly(_) => 1,
        _ => 2,
    };
    let delta = q.duration_ms.map(|d| (r.duration * 1000.0 - d as f64).abs() as u64).unwrap_or(0);
    (kind, delta)
}

fn has_lyrics(r: &LyricsResult) -> bool {
    matches!(r, LyricsResult::Synced(_) | LyricsResult::PlainOnly(_))
}

impl LrclibClient {
    pub fn new(base: Url) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .timeout(Duration::from_secs(8))
            .build()
            .expect("reqwest client");
        Self { base, http }
    }

    fn url(&self, p: &str, params: &[(&str, String)]) -> Url {
        let mut u = self.base.join(p).expect("valid path");
        u.query_pairs_mut().extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
        u
    }

    /// GET returning `Ok(None)` on 404 and `Err` on transport/5xx/parse failure.
    async fn fetch<T: serde::de::DeserializeOwned>(&self, url: Url) -> Result<Option<T>, LyricsError> {
        let resp = self.http.get(url).send().await.map_err(|e| LyricsError::Network(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(LyricsError::Network(format!("HTTP {status}")));
        }
        resp.json::<T>().await.map(Some).map_err(|e| LyricsError::Network(e.to_string()))
    }

    pub async fn lookup(&self, q: &Track) -> Result<LyricsResult, LyricsError> {
        let title = clean_title(&q.title, &q.artist);
        let artist = clean_artist(&q.artist);
        let mut instrumental_seen = false;

        if let Some(d) = q.duration_ms {
            let mut params = vec![("track_name", title.clone()), ("artist_name", artist.clone())];
            if let Some(a) = q.album.as_ref().filter(|a| !a.trim().is_empty()) {
                params.push(("album_name", a.clone()));
            }
            params.push(("duration", ((d + 500) / 1000).to_string()));
            if let Some(r) = self.fetch::<LrclibRecord>(self.url("/api/get", &params)).await? {
                if passes_guard(q, &r, false) {
                    let res = to_result(&r);
                    if has_lyrics(&res) {
                        return Ok(res);
                    }
                    instrumental_seen |= r.instrumental;
                }
            }
        }

        let params = [("track_name", title), ("artist_name", artist)];
        let found = self.fetch::<Vec<LrclibRecord>>(self.url("/api/search", &params)).await?.unwrap_or_default();
        let mut passing: Vec<&LrclibRecord> = found.iter().filter(|r| passes_guard(q, r, true)).collect();
        passing.sort_by_key(|r| rank(q, r));
        if let Some(best) = passing.first() {
            let res = to_result(best);
            if has_lyrics(&res) {
                return Ok(res);
            }
        }
        instrumental_seen |= passing.iter().any(|r| r.instrumental);
        Ok(if instrumental_seen { LyricsResult::Instrumental } else { LyricsResult::NotFound })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::matching::tests::trk;
    use serde_json::json;
    use wiremock::matchers::{header_regex, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn body(title: &str, dur: f64, synced: Option<&str>, instrumental: bool) -> serde_json::Value {
        json!({"id": 7, "trackName": title, "artistName": "Artist", "albumName": "Album", "duration": dur,
               "instrumental": instrumental, "plainLyrics": synced.map(|_| "plain"), "syncedLyrics": synced})
    }
    async fn client(s: &MockServer) -> LrclibClient { LrclibClient::new(Url::parse(&s.uri()).unwrap()) }
    fn song() -> Track { trk("Song", "Artist", Some(200_000)) }

    #[tokio::test]
    async fn get_hit_returns_synced() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body("Song", 200.0, Some("[00:01.00]hi"), false))).mount(&s).await;
        let r = client(&s).await.lookup(&song()).await.unwrap();
        assert!(matches!(r, LyricsResult::Synced(ref l) if !l.is_empty()));
    }

    #[tokio::test]
    async fn get_404_then_search_hit() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).respond_with(ResponseTemplate::new(404)).mount(&s).await;
        Mock::given(method("GET")).and(path("/api/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([body("Song", 201.0, Some("[00:01.00]hi"), false)]))).mount(&s).await;
        assert!(matches!(client(&s).await.lookup(&song()).await.unwrap(), LyricsResult::Synced(_)));
    }

    #[tokio::test]
    async fn instrumental_from_get_falls_through_to_search() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body("Song", 200.0, None, true))).mount(&s).await;
        Mock::given(method("GET")).and(path("/api/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([body("Song", 200.0, Some("[00:01.00]hi"), false)]))).mount(&s).await;
        assert!(matches!(client(&s).await.lookup(&song()).await.unwrap(), LyricsResult::Synced(_)));
    }

    #[tokio::test]
    async fn instrumental_kept_when_nothing_better() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body("Song", 200.0, None, true))).mount(&s).await;
        Mock::given(method("GET")).and(path("/api/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))).mount(&s).await;
        assert_eq!(client(&s).await.lookup(&song()).await.unwrap(), LyricsResult::Instrumental);
    }

    #[tokio::test]
    async fn wrong_duration_everywhere_is_not_found() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body("Song", 230.0, Some("[00:01.00]hi"), false))).mount(&s).await;
        Mock::given(method("GET")).and(path("/api/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([body("Song", 230.0, Some("[00:01.00]hi"), false)]))).mount(&s).await;
        assert_eq!(client(&s).await.lookup(&song()).await.unwrap(), LyricsResult::NotFound);
    }

    #[tokio::test]
    async fn server_error_is_not_not_found() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).respond_with(ResponseTemplate::new(503)).mount(&s).await;
        assert!(matches!(client(&s).await.lookup(&song()).await, Err(LyricsError::Network(_))));
    }

    #[tokio::test]
    async fn sends_user_agent_and_seconds() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).and(query_param("duration", "200"))
            .and(query_param("track_name", "Song")).and(header_regex("user-agent", "^OpenMusix/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body("Song", 200.0, Some("[00:01.00]hi"), false)))
            .expect(1).mount(&s).await;
        assert!(matches!(client(&s).await.lookup(&song()).await.unwrap(), LyricsResult::Synced(_)));
    }

    #[tokio::test]
    async fn unknown_duration_skips_get() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/get")).respond_with(ResponseTemplate::new(500)).expect(0).mount(&s).await;
        Mock::given(method("GET")).and(path("/api/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([body("Song", 200.0, Some("[00:01.00]hi"), false)]))).mount(&s).await;
        let q = trk("Artist - Song (Official Video)", "ArtistVEVO", None);
        assert!(matches!(client(&s).await.lookup(&q).await.unwrap(), LyricsResult::Synced(_)));
    }
}
