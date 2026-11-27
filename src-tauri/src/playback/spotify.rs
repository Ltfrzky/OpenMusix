//! Optional Spotify Web API poller (currently-playing), used to refine timing.
use super::{windows::now_unix_ms, PlaybackEvent, SourceSnapshot};
use crate::model::Track;
use crate::spotify_auth::{AuthError, SpotifyAuth};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Notify};

const CURRENTLY_PLAYING: &str = "https://api.spotify.com/v1/me/player/currently-playing";

#[derive(Deserialize)]
struct Image {
    url: String,
    #[serde(default)]
    width: Option<u32>,
}
#[derive(Deserialize)]
struct Album {
    name: Option<String>,
    #[serde(default)]
    images: Vec<Image>,
}
#[derive(Deserialize)]
struct Artist {
    name: String,
}
#[derive(Deserialize)]
struct Item {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    duration_ms: Option<u64>,
    #[serde(default)]
    artists: Vec<Artist>,
    album: Option<Album>,
}
#[derive(Deserialize)]
struct Playing {
    progress_ms: Option<u64>,
    #[serde(default)]
    is_playing: bool,
    item: Option<Item>,
}

/// Turn a currently-playing response into a snapshot. Ads, episodes and empty bodies → None.
pub fn parse_currently_playing(status: u16, body: &str, sampled_at: i64) -> Option<SourceSnapshot> {
    if status != 200 {
        return None;
    }
    let p: Playing = serde_json::from_str(body).ok()?;
    let item = p.item.filter(|i| i.kind == "track")?;
    let art_url = item
        .album
        .as_ref()
        .and_then(|a| a.images.iter().max_by_key(|i| i.width.unwrap_or(0)))
        .map(|i| i.url.clone());
    Some(SourceSnapshot {
        track: Track {
            title: item.name,
            artist: item.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
            album: item.album.and_then(|a| a.name),
            duration_ms: item.duration_ms,
            art_url,
        },
        position_ms: p.progress_ms.unwrap_or(0),
        sampled_at,
        is_playing: p.is_playing,
    })
}

/// `Retry-After` wins; otherwise 2 s while Spotify's data is on screen, 10 s otherwise.
pub fn next_poll_delay(spotify_used: bool, retry_after_s: Option<u64>) -> Duration {
    match retry_after_s {
        Some(s) => Duration::from_secs(s.max(1)),
        None if spotify_used => Duration::from_secs(2),
        None => Duration::from_secs(10),
    }
}

enum Poll {
    Ok(Option<SourceSnapshot>),
    RateLimited(u64),
    Unauthorized,
    Failed,
}

async fn poll_once(http: &reqwest::Client, token: &str) -> Poll {
    let t0 = now_unix_ms();
    let resp = match http.get(CURRENTLY_PLAYING).bearer_auth(token).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("spotify poll failed: {e}");
            return Poll::Failed;
        }
    };
    let sampled_at = (t0 + now_unix_ms()) / 2;
    let status = resp.status().as_u16();
    match status {
        200 | 204 => {
            let body = resp.text().await.unwrap_or_default();
            Poll::Ok(parse_currently_playing(status, &body, sampled_at))
        }
        401 => Poll::Unauthorized,
        429 => {
            let s = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(30);
            Poll::RateLimited(s)
        }
        _ => Poll::Failed,
    }
}

/// Poll forever while connected. `client_id` is read fresh each round (Settings may change it).
pub async fn run_poller(
    auth: Arc<SpotifyAuth>,
    client_id: impl Fn() -> Option<String> + Send + 'static,
    tx: mpsc::Sender<PlaybackEvent>,
    mut spotify_used: watch::Receiver<bool>,
    wake: Arc<Notify>,
) {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().expect("http client");
    let mut connected = auth.watch_connected();
    let mut last: Option<Option<SourceSnapshot>> = None;
    loop {
        let cid = client_id();
        if !*connected.borrow_and_update() || cid.is_none() {
            if last.take().is_some() {
                let _ = tx.send(PlaybackEvent::Spotify(None)).await;
            }
            tokio::select! {
                _ = connected.changed() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
            continue;
        }
        let cid = cid.unwrap();
        let mut retry_after = None;
        match auth.access_token(&cid).await {
            Ok(token) => {
                let mut res = poll_once(&http, &token).await;
                if matches!(res, Poll::Unauthorized) {
                    auth.invalidate();
                    if let Ok(t) = auth.access_token(&cid).await {
                        res = poll_once(&http, &t).await;
                    }
                }
                match res {
                    Poll::Ok(snap) => {
                        if last.as_ref() != Some(&snap) {
                            last = Some(snap.clone());
                            if tx.send(PlaybackEvent::Spotify(snap)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Poll::RateLimited(s) => retry_after = Some(s),
                    Poll::Unauthorized | Poll::Failed => retry_after = Some(10),
                }
            }
            Err(AuthError::Revoked) | Err(AuthError::NotConnected) => continue,
            Err(e) => {
                tracing::warn!("{e}");
                retry_after = Some(15);
            }
        }
        let delay = next_poll_delay(*spotify_used.borrow_and_update(), retry_after);
        let rate_limited = retry_after.is_some();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            // A Windows-side change (track skip, seek) re-polls at once, unless we're backing off.
            _ = wake.notified(), if !rate_limited => {}
            _ = spotify_used.changed() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    const TRACK: &str = r#"{"timestamp":1,"progress_ms":61000,"is_playing":true,"currently_playing_type":"track",
      "item":{"type":"track","name":"Song","duration_ms":200000,"artists":[{"name":"A"},{"name":"B"}],
      "album":{"name":"Album","images":[{"url":"https://i.scdn.co/big","width":640},{"url":"https://i.scdn.co/small","width":64}]}}}"#;

    #[test] fn parses_track() {
        let s = parse_currently_playing(200, TRACK, 5000).unwrap();
        assert_eq!(s.track.title, "Song");
        assert_eq!(s.track.artist, "A, B");
        assert_eq!(s.track.album.as_deref(), Some("Album"));
        assert_eq!(s.track.duration_ms, Some(200_000));
        assert_eq!(s.track.art_url.as_deref(), Some("https://i.scdn.co/big"));
        assert_eq!((s.position_ms, s.sampled_at, s.is_playing), (61_000, 5000, true));
    }
    #[test] fn ad_is_none() {
        assert!(parse_currently_playing(200, r#"{"currently_playing_type":"ad","item":null,"is_playing":true}"#, 0).is_none());
        assert!(parse_currently_playing(200, r#"{"currently_playing_type":"episode","item":{"type":"episode","name":"x"},"is_playing":true}"#, 0).is_none());
    }
    #[test] fn no_content_is_none() { assert!(parse_currently_playing(204, "", 0).is_none()); }
    #[test] fn garbage_is_none() { assert!(parse_currently_playing(200, "{oops", 0).is_none()); }
    #[test] fn poll_delays() {
        assert_eq!(next_poll_delay(true, None), Duration::from_secs(2));
        assert_eq!(next_poll_delay(false, None), Duration::from_secs(10));
        assert_eq!(next_poll_delay(true, Some(30)), Duration::from_secs(30));
    }
}
