//! Spotify PKCE auth (no client secret). Refresh token lives in Windows Credential Manager.
use crate::server::OAuthCallback;
use base64::Engine;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use url::Url;

pub const SCOPES: &str = "user-read-currently-playing user-read-playback-state";
const AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const KEYRING_SERVICE: &str = "OpenMusix";
const KEYRING_USER: &str = "spotify-refresh-token";

pub fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn new_verifier() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    (0..64).map(|_| CHARS[rng.random_range(0..CHARS.len())] as char).collect()
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/spotify/callback")
}

pub fn authorize_url(client_id: &str, port: u16, challenge: &str, state: &str) -> Url {
    let mut u = Url::parse(AUTHORIZE).expect("static url");
    u.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri(port))
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", challenge)
        .append_pair("scope", SCOPES)
        .append_pair("state", state);
    u
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Spotify is not connected")]
    NotConnected,
    #[error("Spotify access was revoked; connect again")]
    Revoked,
    #[error("Spotify auth failed: {0}")]
    Other(String),
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

struct Pending {
    verifier: String,
    state: String,
    client_id: String,
    port: u16,
    created: Instant,
}

struct Access {
    token: String,
    expires_at: Instant,
}

pub struct SpotifyAuth {
    keyring_user: String,
    http: reqwest::Client,
    pending: Mutex<Option<Pending>>,
    access: Mutex<Option<Access>>,
    connected: watch::Sender<bool>,
}

fn keyring_entry_for(user: &str) -> Result<keyring::Entry, AuthError> {
    keyring::Entry::new(KEYRING_SERVICE, user).map_err(|e| AuthError::Other(e.to_string()))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn page(title: &str, body: &str) -> String {
    let (title, body) = (escape(title), escape(body));
    format!(
        "<!doctype html><meta charset=utf-8><title>OpenMusix</title>\
         <body style=\"font:15px system-ui;background:#101114;color:#ededef;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"text-align:center\"><h1 style=\"font-size:20px\">{title}</h1><p style=\"color:#a8a9af\">{body}</p></div>"
    )
}

impl SpotifyAuth {
    pub fn new() -> Self {
        Self::with_keyring_user(KEYRING_USER)
    }

    /// Separate credential slot (tests use their own so the real token is never touched).
    pub fn with_keyring_user(user: &str) -> Self {
        let connected = keyring_entry_for(user).and_then(|e| e.get_password().map_err(|e| AuthError::Other(e.to_string()))).is_ok();
        Self {
            keyring_user: user.to_string(),
            http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client"),
            pending: Mutex::new(None),
            access: Mutex::new(None),
            connected: watch::Sender::new(connected),
        }
    }

    fn keyring_entry(&self) -> Result<keyring::Entry, AuthError> {
        keyring_entry_for(&self.keyring_user)
    }

    pub fn has_pending(&self) -> bool {
        self.pending.lock().unwrap().is_some()
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

    pub fn watch_connected(&self) -> watch::Receiver<bool> {
        self.connected.subscribe()
    }

    /// Start a login; returns the URL to open in the browser.
    pub fn begin(&self, client_id: &str, port: u16) -> Url {
        let verifier = new_verifier();
        let state = new_verifier()[..24].to_string();
        let url = authorize_url(client_id, port, &pkce_challenge(&verifier), &state);
        *self.pending.lock().unwrap() =
            Some(Pending { verifier, state, client_id: client_id.to_string(), port, created: Instant::now() });
        url
    }

    pub fn disconnect(&self) {
        if let Ok(e) = self.keyring_entry() {
            let _ = e.delete_credential();
        }
        *self.access.lock().unwrap() = None;
        self.connected.send_replace(false);
    }

    async fn token_request(&self, form: &[(&str, &str)]) -> Result<TokenResponse, AuthError> {
        let resp = self.http.post(TOKEN).form(form).send().await.map_err(|e| AuthError::Other(e.to_string()))?;
        if resp.status().is_success() {
            return resp.json().await.map_err(|e| AuthError::Other(e.to_string()));
        }
        let text = resp.text().await.unwrap_or_default();
        if text.contains("invalid_grant") {
            return Err(AuthError::Revoked);
        }
        Err(AuthError::Other(text))
    }

    fn store(&self, t: TokenResponse) -> Result<String, AuthError> {
        if let Some(r) = &t.refresh_token {
            self.keyring_entry()?.set_password(r).map_err(|e| AuthError::Other(e.to_string()))?;
        }
        *self.access.lock().unwrap() =
            Some(Access { token: t.access_token.clone(), expires_at: Instant::now() + Duration::from_secs(t.expires_in) });
        Ok(t.access_token)
    }

    /// Cached access token, refreshed 60 s before expiry. A revoked grant disconnects.
    pub async fn access_token(&self, client_id: &str) -> Result<String, AuthError> {
        if let Some(a) = self.access.lock().unwrap().as_ref()
            && a.expires_at > Instant::now() + Duration::from_secs(60) {
                return Ok(a.token.clone());
            }
        let refresh = self.keyring_entry()?.get_password().map_err(|_| AuthError::NotConnected)?;
        let form = [("grant_type", "refresh_token"), ("refresh_token", refresh.as_str()), ("client_id", client_id)];
        match self.token_request(&form).await {
            Ok(t) => self.store(t),
            Err(AuthError::Revoked) => {
                self.disconnect();
                Err(AuthError::Revoked)
            }
            Err(e) => Err(e),
        }
    }

    /// Forget the cached access token (after a 401).
    pub fn invalidate(&self) {
        *self.access.lock().unwrap() = None;
    }

    pub(crate) async fn finish(&self, code: Option<String>, state: Option<String>, error: Option<String>) -> String {
        if let Some(e) = error {
            return page("Spotify was not connected", &format!("Spotify said: {e}. You can close this tab."));
        }
        // Peek before taking: a forged callback (wrong state) must not cancel the real login.
        let p = {
            let mut guard = self.pending.lock().unwrap();
            match guard.as_ref() {
                None => return page("Link expired", "Start Connect Spotify again from OpenMusix."),
                Some(p) if p.created.elapsed() >= Duration::from_secs(600) => {
                    *guard = None;
                    return page("Link expired", "Start Connect Spotify again from OpenMusix.");
                }
                Some(p) if state.as_deref() != Some(p.state.as_str()) => {
                    return page("Something didn't match", "Start Connect Spotify again from OpenMusix.");
                }
                Some(_) => guard.take().expect("checked above"),
            }
        };
        let Some(code) = code else { return page("Spotify was not connected", "No authorization code was returned.") };
        let redirect = redirect_uri(p.port);
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", p.client_id.as_str()),
            ("code_verifier", p.verifier.as_str()),
        ];
        match self.token_request(&form).await.and_then(|t| self.store(t)) {
            Ok(_) => {
                self.connected.send_replace(true);
                page("Spotify connected", "You can close this tab and go back to OpenMusix.")
            }
            Err(e) => page("Spotify was not connected", &e.to_string()),
        }
    }
}

impl Default for SpotifyAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthCallback for SpotifyAuth {
    fn handle<'a>(
        &'a self,
        code: Option<String>,
        state: Option<String>,
        error: Option<String>,
    ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
        Box::pin(self.finish(code, state, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rfc7636_vector() {
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
    #[test] fn verifier_shape() {
        let v = new_verifier();
        assert_eq!(v.len(), 64);
        assert!(v.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_ne!(v, new_verifier());
    }
    #[tokio::test] async fn callback_page_escapes_html() {
        let a = SpotifyAuth::with_keyring_user("test-escape");
        let html = a.finish(None, None, Some("<script>alert(1)</script>".into())).await;
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
    #[tokio::test] async fn bogus_state_keeps_pending_login() {
        let a = SpotifyAuth::with_keyring_user("test-state");
        a.begin("cid", 7878);
        let html = a.finish(Some("code".into()), Some("wrong".into()), None).await;
        assert!(html.contains("match"));
        assert!(a.has_pending(), "a forged callback must not cancel the real login");
    }
    #[test] fn authorize_url_uses_loopback_ip() {
        let u = authorize_url("cid", 7878, "ch", "st");
        let s = u.as_str();
        assert!(s.starts_with("https://accounts.spotify.com/authorize?"));
        assert!(s.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7878%2Fspotify%2Fcallback"));
        assert!(s.contains("code_challenge_method=S256"));
        assert!(s.contains("response_type=code"));
        assert!(s.contains("scope=user-read-currently-playing+user-read-playback-state"));
    }
}
