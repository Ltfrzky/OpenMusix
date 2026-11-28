//! macOS media source. macOS has no public "now playing" API, so this tries two readers:
//! 1. The MediaRemote adapter (perl + bundled framework), which sees every app, browsers included.
//! 2. AppleScript polling of Spotify and Music.app, if the adapter is missing or stops working.
use super::macos_parse::{parse_adapter_line, parse_osascript, session_info, AdapterLine, MacNow, ADAPTER_STREAM_ARGS, SCRIPTED_APPS};
use super::system::{art_from_bytes, changed_beyond_drift, fetch_http_art, now_unix_ms, pick_session, RESCAN_INTERVAL};
use super::{PlaybackEvent, SourceSnapshot};
use crate::hub::Hub;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// How long the adapter may stay silent after starting before it counts as broken.
const ADAPTER_FIRST_LINE_TIMEOUT: Duration = Duration::from_secs(5);
/// Adapter restarts before falling back to AppleScript for the rest of the session.
const ADAPTER_MAX_STARTS: u32 = 3;

pub fn spawn_macos_source(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("nowplaying".into())
        .spawn(move || run(tx, hub))
        .expect("spawn now-playing thread");
}

/// Paths of the adapter script and framework. In the app bundle the script is a resource and
/// the framework sits in `Contents/Frameworks`; debug builds also look in `src-tauri/macos/`.
fn adapter_paths() -> Option<(PathBuf, PathBuf)> {
    let mut candidates = Vec::new();
    if let Some(contents) = std::env::current_exe().ok().and_then(|e| e.parent()?.parent().map(PathBuf::from)) {
        candidates.push((
            contents.join("Resources/mediaremote/mediaremote-adapter.pl"),
            contents.join("Frameworks/MediaRemoteAdapter.framework"),
        ));
    }
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("macos");
        candidates.push((dev.join("mediaremote-adapter.pl"), dev.join("MediaRemoteAdapter.framework")));
    }
    candidates.into_iter().find(|(s, f)| s.is_file() && f.is_dir())
}

/// Sends snapshots to the pipeline, de-duplicated, and keeps the hub's cover art current.
struct Emitter {
    tx: mpsc::Sender<PlaybackEvent>,
    hub: Arc<Hub>,
    last_track_id: String,
    last_sent: Option<Option<SourceSnapshot>>,
}

impl Emitter {
    /// Returns false once the pipeline is gone.
    fn emit(&mut self, now: Option<&MacNow>, only_beyond_drift: bool) -> bool {
        let snap = now.map(|n| {
            let id = n.track_id();
            let new_track = id != self.last_track_id;
            if new_track {
                self.last_track_id = id;
            }
            // The adapter sometimes sends a track before its artwork has loaded, so also fill
            // in art that arrives later for the same track.
            if new_track || self.hub.current_art_key().is_none() {
                let art = match (&n.art, &n.art_url) {
                    (Some((bytes, mime)), _) => art_from_bytes(bytes.clone(), mime),
                    (None, Some(url)) if new_track => fetch_http_art(url),
                    _ => None,
                };
                if new_track || art.is_some() {
                    self.hub.set_art(art);
                }
            }
            n.snapshot(self.hub.current_art_key().map(|k| format!("/art/{k}")))
        });
        if snap.is_none() {
            self.last_track_id.clear();
            self.hub.set_art(None);
        }
        let prev = self.last_sent.as_ref();
        let send = match prev {
            None => true,
            Some(p) if only_beyond_drift => changed_beyond_drift(p.as_ref(), snap.as_ref()),
            Some(p) => *p != snap,
        };
        if send {
            self.last_sent = Some(snap.clone());
            return self.tx.blocking_send(PlaybackEvent::System(snap)).is_ok();
        }
        true
    }
}

fn run(tx: mpsc::Sender<PlaybackEvent>, hub: Arc<Hub>) {
    let mut out = Emitter { tx, hub, last_track_id: String::new(), last_sent: None };
    if let Some((script, framework)) = adapter_paths() {
        for attempt in 1..=ADAPTER_MAX_STARTS {
            match run_adapter(&script, &framework, &mut out) {
                AdapterEnd::PipelineClosed => return,
                AdapterEnd::Failed(why) => {
                    tracing::warn!("MediaRemote adapter stopped (attempt {attempt}/{ADAPTER_MAX_STARTS}): {why}");
                    std::thread::sleep(Duration::from_secs(2 * attempt as u64));
                }
            }
        }
        tracing::warn!("falling back to AppleScript (Spotify and Music only)");
    } else {
        tracing::warn!("MediaRemote adapter not bundled; using AppleScript (Spotify and Music only)");
    }
    run_applescript(&mut out);
}

enum AdapterEnd {
    PipelineClosed,
    Failed(String),
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_adapter(script: &Path, framework: &Path, out: &mut Emitter) -> AdapterEnd {
    let mut child = match Command::new("/usr/bin/perl")
        .arg(script)
        .arg(framework)
        .args(ADAPTER_STREAM_ARGS)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return AdapterEnd::Failed(format!("spawn: {e}")),
    };
    let Some(stdout) = child.stdout.take() else { return AdapterEnd::Failed("no stdout".into()) };
    let _guard = KillOnDrop(child);
    let (line_tx, lines) = std_mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut first = true;
    loop {
        let line = if first {
            match lines.recv_timeout(ADAPTER_FIRST_LINE_TIMEOUT) {
                Ok(l) => l,
                Err(std_mpsc::RecvTimeoutError::Timeout) => return AdapterEnd::Failed("no output".into()),
                Err(std_mpsc::RecvTimeoutError::Disconnected) => return AdapterEnd::Failed("exited at start".into()),
            }
        } else {
            match lines.recv() {
                Ok(l) => l,
                Err(_) => return AdapterEnd::Failed("exited".into()),
            }
        };
        first = false;
        let ok = match parse_adapter_line(&line, now_unix_ms()) {
            AdapterLine::Now(n) => out.emit(Some(&n), false),
            AdapterLine::Idle => out.emit(None, false),
            AdapterLine::Skip => true,
        };
        if !ok {
            return AdapterEnd::PipelineClosed;
        }
    }
}

fn is_running(process: &str) -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", process])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn query(script: &str) -> Option<MacNow> {
    let o = Command::new("/usr/bin/osascript").args(["-e", script]).stderr(Stdio::null()).output().ok()?;
    if !o.status.success() {
        return None;
    }
    parse_osascript(&String::from_utf8_lossy(&o.stdout), now_unix_ms())
}

fn run_applescript(out: &mut Emitter) {
    let mut current: Option<String> = None;
    loop {
        let found: Vec<MacNow> = SCRIPTED_APPS
            .iter()
            .filter(|(process, _)| is_running(process))
            .filter_map(|(_, script)| query(script))
            .collect();
        let infos: Vec<_> = found.iter().map(session_info).collect();
        let cur = current.as_ref().and_then(|c| infos.iter().position(|i| &i.app_id == c));
        let chosen = pick_session(&infos, cur);
        current = chosen.map(|i| infos[i].app_id.clone());
        if !out.emit(chosen.map(|i| &found[i]), true) {
            return;
        }
        std::thread::sleep(RESCAN_INTERVAL);
    }
}
