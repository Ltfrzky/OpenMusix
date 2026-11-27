//! Tauri IPC commands plus the pure helpers they rely on.
use crate::hub::Snapshot;
use crate::settings::{Settings, OFFSET_LIMIT_MS};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::app::AppCtx;
use crate::server::ServerStatus;
use crate::settings::Mode;
use crate::roman::packs::PackInfo;
use crate::roman::RomanLang;
use crate::shell;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

/// Add `delta` to a per-track offset, clamped to ±2000; zero removes the entry.
pub fn apply_nudge(map: &mut HashMap<String, i32>, key: &str, delta: i32) -> i32 {
    let v = (map.get(key).copied().unwrap_or(0) + delta).clamp(-OFFSET_LIMIT_MS, OFFSET_LIMIT_MS);
    if v == 0 {
        map.remove(key);
    } else {
        map.insert(key.to_string(), v);
    }
    v
}

/// Merge `patch` (partial camelCase JSON) onto `current`, recursing into objects.
/// Fields the patch doesn't name are kept, so backend-side changes are never clobbered.
pub fn apply_patch(current: &Settings, patch: serde_json::Value) -> Result<Settings, String> {
    fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
        match (base, patch) {
            (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
                for (k, v) in p {
                    match b.get_mut(&k) {
                        Some(slot) if v.is_object() && slot.is_object() => merge(slot, v),
                        _ => {
                            b.insert(k, v);
                        }
                    }
                }
            }
            (b, p) => *b = p,
        }
    }
    let mut v = serde_json::to_value(current).map_err(err)?;
    merge(&mut v, patch);
    serde_json::from_value(v).map_err(err)
}

/// Copy the overlay-relevant settings into the snapshot.
pub fn project_settings(s: &Settings, snap: &mut Snapshot) {
    snap.mode = s.mode;
    snap.styles = s.styles.clone();
    snap.global_offset_ms = s.global_offset_ms;
    snap.show_plain_lyrics = s.show_plain_lyrics;
    snap.hide_when_idle = s.hide_when_idle;
    snap.romanize = s.romanize;
    if !s.romanize.enabled {
        snap.roman = None;
        snap.roman_missing.clear();
    }
    snap.track_offset_ms = snap.track_key.as_ref().and_then(|k| s.track_offsets_ms.get(k)).copied().unwrap_or(0);
}

type Ctx<'a> = State<'a, Arc<AppCtx>>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_snapshot(ctx: Ctx<'_>) -> Snapshot {
    ctx.hub.current()
}

#[tauri::command]
pub fn get_settings(ctx: Ctx<'_>) -> Settings {
    ctx.settings.get()
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, ctx: Ctx<'_>, patch: serde_json::Value) -> Result<Settings, String> {
    let old = ctx.settings.get();
    let mut bad = None;
    let new = ctx
        .settings
        .update(|s| match apply_patch(s, patch) {
            Ok(next) => *s = next,
            Err(e) => bad = Some(e),
        })
        .map_err(err)?;
    if let Some(e) = bad {
        return Err(format!("Invalid setting: {e}"));
    }
    ctx.sync_settings();
    if new.romanize != old.romanize || new.show_plain_lyrics != old.show_plain_lyrics {
        ctx.refresh_roman();
    }
    if new.hotkey != old.hotkey
        && let Err(e) = shell::register_hotkey(&app, Some(&old.hotkey), &new.hotkey) {
            // Keep the old, working hotkey and report the problem.
            let _ = shell::register_hotkey(&app, None, &old.hotkey);
            ctx.settings.update(|s| s.hotkey = old.hotkey.clone()).map_err(err)?;
            return Err(format!("Hotkey not available: {e}"));
        }
    if new.start_with_windows != old.start_with_windows {
        let al = app.autolaunch();
        let r = if new.start_with_windows { al.enable() } else { al.disable() };
        r.map_err(err)?;
    }
    if new.floating_window != old.floating_window {
        shell::floating_window(&app, new.floating_window, new.port).map_err(err)?;
    }
    if new.port != old.port {
        ctx.restart_server(&app).await;
        if new.floating_window {
            let _ = shell::floating_window(&app, false, new.port);
            let _ = shell::floating_window(&app, true, new.port);
        }
    }
    Ok(new)
}

#[tauri::command]
pub fn list_packs(ctx: Ctx<'_>) -> Vec<PackInfo> {
    ctx.packs.list()
}

#[derive(Clone, serde::Serialize)]
struct PackProgress {
    id: RomanLang,
    pct: u8,
}

/// Download and install a pack; emits `pack-progress` while downloading and `packs-changed` at the end.
#[tauri::command]
pub async fn install_pack(app: AppHandle, ctx: Ctx<'_>, id: RomanLang) -> Result<(), String> {
    let ctx = ctx.inner().clone();
    let h = app.clone();
    let _ = app.emit("packs-changed", ctx.packs.list());
    let res = ctx.packs.install(id, move |pct| {
        let _ = h.emit("pack-progress", PackProgress { id, pct });
    }).await;
    let _ = app.emit("packs-changed", ctx.packs.list());
    ctx.refresh_roman();
    res.map_err(err)
}

#[tauri::command]
pub fn remove_pack(app: AppHandle, ctx: Ctx<'_>, id: RomanLang) -> Result<(), String> {
    let res = ctx.packs.remove(id);
    let _ = app.emit("packs-changed", ctx.packs.list());
    ctx.inner().refresh_roman();
    res.map_err(err)
}

#[tauri::command]
pub fn cancel_pack_install(ctx: Ctx<'_>, id: RomanLang) {
    ctx.packs.cancel(id);
}

pub fn set_mode_inner(ctx: &AppCtx, mode: Mode) -> Result<(), String> {
    ctx.settings.update(|s| s.mode = mode).map_err(err)?;
    ctx.sync_settings();
    Ok(())
}

pub fn toggle_mode_inner(ctx: &AppCtx) -> Result<Mode, String> {
    let next = match ctx.settings.get().mode {
        Mode::Focus => Mode::Widget,
        Mode::Widget => Mode::Focus,
    };
    set_mode_inner(ctx, next)?;
    Ok(next)
}

#[tauri::command]
pub fn set_mode(ctx: Ctx<'_>, mode: Mode) -> Result<(), String> {
    set_mode_inner(&ctx, mode)
}

#[tauri::command]
pub fn toggle_mode(ctx: Ctx<'_>) -> Result<Mode, String> {
    toggle_mode_inner(&ctx)
}

#[tauri::command]
pub fn nudge_track_offset(ctx: Ctx<'_>, delta_ms: i32) -> Result<i32, String> {
    let Some(key) = ctx.hub.current().track_key else { return Ok(0) };
    let mut out = 0;
    ctx.settings.update(|s| out = apply_nudge(&mut s.track_offsets_ms, &key, delta_ms)).map_err(err)?;
    ctx.sync_settings();
    Ok(out)
}

#[tauri::command]
pub fn reset_track_offset(ctx: Ctx<'_>) -> Result<(), String> {
    let Some(key) = ctx.hub.current().track_key else { return Ok(()) };
    ctx.settings.update(|s| { s.track_offsets_ms.remove(&key); }).map_err(err)?;
    ctx.sync_settings();
    Ok(())
}

#[tauri::command]
pub async fn get_server_status(ctx: Ctx<'_>) -> Result<ServerStatus, String> {
    Ok(ctx.server.lock().await.0.clone())
}

#[tauri::command]
pub async fn restart_server(app: AppHandle, ctx: Ctx<'_>) -> Result<ServerStatus, String> {
    Ok(ctx.restart_server(&app).await)
}

#[tauri::command]
pub fn clear_lyrics_cache(ctx: Ctx<'_>) -> Result<(), String> {
    ctx.lyrics.clear_cache().map_err(err)
}

#[tauri::command]
pub fn open_lyrics_folder(app: AppHandle, ctx: Ctx<'_>) -> Result<(), String> {
    let dir = ctx.lyrics.local_dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener().open_path(dir.to_string_lossy().to_string(), None::<&str>).map_err(err)
}

#[tauri::command]
pub async fn list_system_fonts() -> Vec<String> {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    if let Some(f) = FONTS.get() {
        return f.clone();
    }
    let list = tauri::async_runtime::spawn_blocking(|| {
        let mut v = font_kit::source::SystemSource::new().all_families().unwrap_or_default();
        v.retain(|f| !f.starts_with('@'));
        v.sort_by_key(|f| f.to_lowercase());
        v.dedup();
        v
    })
    .await
    .unwrap_or_default();
    FONTS.get_or_init(|| list).clone()
}

#[tauri::command]
pub fn set_floating_window(app: AppHandle, ctx: Ctx<'_>, on: bool) -> Result<(), String> {
    let s = ctx.settings.update(|s| s.floating_window = on).map_err(err)?;
    shell::floating_window(&app, on, s.port).map_err(err)
}

/// Used by the panel to show the window when launched minimized.
#[tauri::command]
pub fn show_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn nudge_accumulates_and_clamps() {
        let mut m = HashMap::new();
        assert_eq!(apply_nudge(&mut m, "k", 100), 100);
        assert_eq!(apply_nudge(&mut m, "k", 100), 200);
        m.insert("k".into(), 1950);
        assert_eq!(apply_nudge(&mut m, "k", 100), 2000);
        m.insert("k".into(), -1950);
        assert_eq!(apply_nudge(&mut m, "k", -100), -2000);
    }
    #[test] fn zero_offset_removes_key() {
        let mut m = HashMap::from([("k".to_string(), 100)]);
        assert_eq!(apply_nudge(&mut m, "k", -100), 0);
        assert!(!m.contains_key("k"));
    }
    #[test] fn patch_keeps_fields_it_does_not_name() {
        let mut cur = Settings { mode: crate::settings::Mode::Widget, ..Default::default() };
        cur.track_offsets_ms.insert("a|b".into(), 300);
        let next = apply_patch(&cur, serde_json::json!({ "globalOffsetMs": 100 })).unwrap();
        assert_eq!(next.global_offset_ms, 100);
        assert_eq!(next.mode, crate::settings::Mode::Widget);
        assert_eq!(next.track_offsets_ms.get("a|b"), Some(&300));
    }
    #[test] fn patch_merges_nested_styles() {
        let cur = Settings::default();
        let next = apply_patch(&cur, serde_json::json!({ "styles": { "widget": { "fontSizePx": 30 } } })).unwrap();
        assert_eq!(next.styles.widget.font_size_px, 30);
        assert_eq!(next.styles.widget.preset, "glass");
        assert_eq!(next.styles.focus, cur.styles.focus);
    }
    #[test] fn patch_can_clear_client_id() {
        let cur = Settings { spotify_client_id: Some("x".into()), ..Default::default() };
        assert_eq!(apply_patch(&cur, serde_json::json!({ "spotifyClientId": null })).unwrap().spotify_client_id, None);
    }
    #[test] fn bad_patch_is_rejected() { assert!(apply_patch(&Settings::default(), serde_json::json!({ "port": "x" })).is_err()); }
    #[test] fn project_copies_settings_and_track_offset() {
        let mut s = Settings { global_offset_ms: 250, show_plain_lyrics: true, ..Default::default() };
        s.track_offsets_ms.insert("a|b".into(), -300);
        let mut snap = Snapshot { track_key: Some("a|b".into()), ..Default::default() };
        project_settings(&s, &mut snap);
        assert_eq!((snap.global_offset_ms, snap.track_offset_ms, snap.show_plain_lyrics), (250, -300, true));
        snap.track_key = None;
        project_settings(&s, &mut snap);
        assert_eq!(snap.track_offset_ms, 0);
    }
    #[test] fn project_copies_romanize_and_clears_roman_when_disabled() {
        let s = Settings::default();
        let mut snap = Snapshot { roman: Some(vec![Some("x".into())]), roman_missing: vec![crate::roman::RomanLang::Ja], ..Default::default() };
        project_settings(&s, &mut snap);
        assert_eq!(snap.roman, None);
        assert!(snap.roman_missing.is_empty());
        assert!(!snap.romanize.enabled);
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyStatus {
    connected: bool,
    client_id_set: bool,
    redirect_uri: String,
}

#[tauri::command]
pub fn spotify_status(ctx: Ctx<'_>) -> SpotifyStatus {
    let s = ctx.settings.get();
    SpotifyStatus {
        connected: ctx.spotify.is_connected(),
        client_id_set: s.spotify_client_id.as_deref().is_some_and(|c| !c.trim().is_empty()),
        redirect_uri: crate::spotify_auth::redirect_uri(s.port),
    }
}

#[tauri::command]
pub fn spotify_connect(app: AppHandle, ctx: Ctx<'_>) -> Result<(), String> {
    let s = ctx.settings.get();
    let Some(cid) = s.spotify_client_id.filter(|c| !c.trim().is_empty()) else {
        return Err("Paste your Spotify Client ID first.".into());
    };
    if !matches!(ctx.server.try_lock().map(|g| g.0.clone()), Ok(ServerStatus::Running { .. })) {
        return Err("The local server must be running to receive Spotify's redirect.".into());
    }
    let url = ctx.spotify.begin(cid.trim(), s.port);
    app.opener().open_url(url.as_str(), None::<&str>).map_err(err)
}

#[tauri::command]
pub async fn spotify_disconnect(ctx: Ctx<'_>) -> Result<(), String> {
    ctx.spotify.disconnect();
    let _ = ctx.playback_tx.send(crate::playback::PlaybackEvent::Spotify(None)).await;
    Ok(())
}
