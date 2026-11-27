//! Local overlay server on 127.0.0.1: `/overlay`, `/assets/*`, `/ws`, `/art/{key}`, `/spotify/callback`.
use crate::hub::Hub;
use axum::{
    Router,
    extract::{Path, Query, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(RustEmbed)]
#[folder = "../dist"]
struct Assets;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ServerStatus {
    Running { port: u16 },
    PortInUse { port: u16 },
    Failed { port: u16, message: String },
}

/// Handles the Spotify OAuth redirect; returns the HTML page shown in the browser.
pub trait OAuthCallback: Send + Sync {
    fn handle<'a>(
        &'a self,
        code: Option<String>,
        state: Option<String>,
        error: Option<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>>;
}

/// Placeholder used until Spotify is wired in.
pub struct NoSpotify;
impl OAuthCallback for NoSpotify {
    fn handle<'a>(
        &'a self,
        _: Option<String>,
        _: Option<String>,
        _: Option<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(async { "<h1>Spotify is not set up in OpenMusix.</h1>".to_string() })
    }
}

/// Dropping or sending stops the server.
pub struct Shutdown(#[allow(dead_code)] oneshot::Sender<()>);

#[derive(Clone)]
struct AppState {
    hub: Arc<Hub>,
    oauth: Arc<dyn OAuthCallback>,
    port: u16,
}

/// Public name that always resolves to 127.0.0.1. TikTok LIVE Studio refuses `localhost` and raw IPs
/// in a Link source, so the panel offers this name for it.
pub const LOOPBACK_DOMAIN: &str = "localtest.me";

/// The authorities a browser may send for this server: our own names, with `:port`, or bare on port 80.
fn own_authorities(port: u16) -> Vec<String> {
    let mut v: Vec<String> = ["127.0.0.1", "localhost", LOOPBACK_DOMAIN].iter().map(|n| format!("{n}:{port}")).collect();
    if port == 80 {
        v.extend(["127.0.0.1", "localhost", LOOPBACK_DOMAIN].iter().map(|n| n.to_string()));
    }
    v
}

/// Only our own names: blocks DNS-rebinding pages from reading the server.
fn host_allowed(host: Option<&str>, port: u16) -> bool {
    matches!(host, Some(h) if own_authorities(port).iter().any(|a| a == h))
}

/// Browsers let any site open a WebSocket to 127.0.0.1, so the upgrade checks who is asking:
/// the overlay page itself, the app's own windows, or a non-browser client (no Origin).
fn origin_allowed(origin: Option<&str>, port: u16) -> bool {
    let Some(o) = origin else { return true };
    let app = ["http://tauri.localhost", "https://tauri.localhost", "tauri://localhost"];
    own_authorities(port).iter().any(|a| o.strip_prefix("http://") == Some(a.as_str())) || app.contains(&o) || (cfg!(debug_assertions) && o == "http://localhost:1420")
}

async fn guard_host(State(st): State<AppState>, req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok());
    if !host_allowed(host, st.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(req).await
}

fn embedded(path: &str) -> Response {
    match Assets::get(path) {
        Some(f) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref().to_string())], f.data.into_owned()).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn overlay() -> Response {
    embedded("overlay.html")
}

async fn asset(Path(p): Path<String>) -> Response {
    embedded(&format!("assets/{p}"))
}

async fn art(State(st): State<AppState>, Path(key): Path<String>) -> Response {
    match st.hub.art(&key) {
        Some(a) => (
            [(header::CONTENT_TYPE, a.mime), (header::CACHE_CONTROL, "max-age=86400".to_string())],
            a.bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn spotify_callback(State(st): State<AppState>, Query(q): Query<CallbackQuery>) -> Html<String> {
    Html(st.oauth.handle(q.code, q.state, q.error).await)
}

fn frame(s: &crate::hub::Snapshot) -> Message {
    let v = serde_json::json!({ "type": "snapshot", "data": s });
    Message::Text(v.to_string().into())
}

async fn ws(State(st): State<AppState>, headers: axum::http::HeaderMap, up: WebSocketUpgrade) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
    if !origin_allowed(origin, st.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    up.on_upgrade(move |socket| pump(socket, st.hub))
}

async fn pump(mut socket: WebSocket, hub: Arc<Hub>) {
    let mut rx = hub.subscribe();
    let first = frame(&rx.borrow_and_update());
    if socket.send(first).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() { return; }
                let msg = frame(&rx.borrow_and_update());
                if socket.send(msg).await.is_err() { return; }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    _ => {}
                }
            }
        }
    }
}

pub fn router(hub: Arc<Hub>, oauth: Arc<dyn OAuthCallback>, port: u16) -> Router {
    let state = AppState { hub, oauth, port };
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/overlay") }))
        .route("/overlay", get(overlay))
        .route("/assets/{*path}", get(asset))
        .route("/art/{key}", get(art))
        .route("/ws", get(ws))
        .route("/spotify/callback", get(spotify_callback))
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard_host))
        .with_state(state)
}

/// Serve on an already-bound listener (used by tests and by `start_server`).
pub fn start_on(listener: tokio::net::TcpListener, hub: Arc<Hub>, oauth: Arc<dyn OAuthCallback>) -> Shutdown {
    let (tx, rx) = oneshot::channel::<()>();
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let app = router(hub, oauth, port);
    tokio::spawn(async move {
        let r = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
        if let Err(e) = r {
            tracing::error!("overlay server error: {e}");
        }
    });
    Shutdown(tx)
}

/// Bind 127.0.0.1:`port`. The port is fixed: a busy port is reported, never switched.
pub async fn start_server(port: u16, hub: Arc<Hub>, oauth: Arc<dyn OAuthCallback>) -> (ServerStatus, Option<Shutdown>) {
    match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => (ServerStatus::Running { port }, Some(start_on(l, hub, oauth))),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => (ServerStatus::PortInUse { port }, None),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => (ServerStatus::PortInUse { port }, None),
        Err(e) => (ServerStatus::Failed { port, message: e.to_string() }, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::{Art, Hub, Snapshot};
    use crate::settings::Mode;
    use futures_util::StreamExt;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message as TMsg;

    async fn boot() -> (Arc<Hub>, u16, Shutdown) {
        let hub = Arc::new(Hub::new(Snapshot::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let sd = start_on(listener, hub.clone(), Arc::new(NoSpotify));
        (hub, port, sd)
    }
    async fn next_json(ws: &mut (impl StreamExt<Item = Result<TMsg, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> serde_json::Value {
        loop {
            if let TMsg::Text(t) = ws.next().await.unwrap().unwrap() {
                return serde_json::from_str(&t).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn ws_sends_initial_snapshot() {
        let (_h, port, _sd) = boot().await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.unwrap();
        let v = next_json(&mut ws).await;
        assert_eq!(v["type"], "snapshot");
        assert_eq!(v["data"]["mode"], "focus");
    }

    #[tokio::test]
    async fn ws_pushes_on_change() {
        let (h, port, _sd) = boot().await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.unwrap();
        next_json(&mut ws).await;
        h.modify(|s| s.mode = Mode::Widget);
        assert_eq!(next_json(&mut ws).await["data"]["mode"], "widget");
    }

    #[tokio::test]
    async fn art_404_then_200() {
        let (h, port, _sd) = boot().await;
        let url = format!("http://127.0.0.1:{port}/art/x");
        assert_eq!(reqwest::get(&url).await.unwrap().status(), 404);
        h.set_art(Some(Art { key: "x".into(), mime: "image/png".into(), bytes: vec![1, 2, 3] }));
        let r = reqwest::get(&url).await.unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], "image/png");
        assert_eq!(r.bytes().await.unwrap().as_ref(), &[1, 2, 3]);
    }

    #[tokio::test]
    async fn port_in_use_reported() {
        let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = blocker.local_addr().unwrap().port();
        let hub = Arc::new(Hub::new(Snapshot::default()));
        let (status, sd) = start_server(port, hub, Arc::new(NoSpotify)).await;
        assert_eq!(status, ServerStatus::PortInUse { port });
        assert!(sd.is_none());
    }

    /// Requires `npm run build` first (the overlay page is embedded from ../dist).
    #[tokio::test]
    async fn overlay_served() {
        let (_h, port, _sd) = boot().await;
        let r = reqwest::get(format!("http://127.0.0.1:{port}/overlay")).await.unwrap();
        assert_eq!(r.status(), 200);
        assert!(r.text().await.unwrap().contains("<div id=\"app\""));
    }

    async fn ws_with_origin(port: u16, origin: &str) -> bool {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut req = format!("ws://127.0.0.1:{port}/ws").into_client_request().unwrap();
        req.headers_mut().insert("origin", origin.parse().unwrap());
        tokio_tungstenite::connect_async(req).await.is_ok()
    }

    #[tokio::test]
    async fn ws_rejects_foreign_origin() {
        let (_h, port, _sd) = boot().await;
        assert!(!ws_with_origin(port, "https://evil.example").await);
        assert!(!ws_with_origin(port, "http://127.0.0.1:1").await);
    }

    #[tokio::test]
    async fn ws_allows_overlay_and_app_origins() {
        let (_h, port, _sd) = boot().await;
        assert!(ws_with_origin(port, &format!("http://127.0.0.1:{port}")).await);
        assert!(ws_with_origin(port, "http://tauri.localhost").await);
    }

    #[tokio::test]
    async fn rejects_foreign_host_header() {
        let (_h, port, _sd) = boot().await;
        let c = reqwest::Client::new();
        let r = c.get(format!("http://127.0.0.1:{port}/overlay")).header("host", format!("evil.example:{port}")).send().await.unwrap();
        assert_eq!(r.status(), 403);
        let ok = c.get(format!("http://127.0.0.1:{port}/overlay")).header("host", format!("localhost:{port}")).send().await.unwrap();
        assert_eq!(ok.status(), 200);
        let tiktok = c.get(format!("http://127.0.0.1:{port}/overlay")).header("host", format!("localtest.me:{port}")).send().await.unwrap();
        assert_eq!(tiktok.status(), 200);
        let sub = c.get(format!("http://127.0.0.1:{port}/overlay")).header("host", format!("x.localtest.me:{port}")).send().await.unwrap();
        assert_eq!(sub.status(), 403);
    }

    #[test]
    fn port_80_allows_bare_names() {
        assert!(host_allowed(Some("localtest.me"), 80));
        assert!(!host_allowed(Some("localtest.me"), 7878));
        assert!(origin_allowed(Some("http://localtest.me"), 80));
        assert!(origin_allowed(Some("http://localtest.me:7878"), 7878));
        assert!(!origin_allowed(Some("http://localtest.me"), 7878));
    }

    #[tokio::test]
    async fn spotify_callback_uses_handler() {
        let (_h, port, _sd) = boot().await;
        let r = reqwest::get(format!("http://127.0.0.1:{port}/spotify/callback?code=abc&state=s")).await.unwrap();
        assert!(r.text().await.unwrap().contains("Spotify is not set up"));
    }
}
