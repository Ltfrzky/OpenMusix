pub mod app;
pub mod commands;
pub mod hub;
pub mod lrc;
pub mod lyrics;
pub mod model;
pub mod normalize;
pub mod playback;
pub mod roman;
pub mod server;
pub mod settings;
pub mod shell;
pub mod spotify_auth;

use app::{forward_to_panel, run_pipeline, AppCtx};
use hub::{Hub, Snapshot};
use lyrics::{LrclibClient, LyricsService};
use server::ServerStatus;
use settings::SettingsStore;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};
use tokio::sync::{mpsc, watch, Mutex, Notify};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "openmusix_lib=info".into()))
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            // Remember where the floating read-along window was dragged to (and its size).
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&["main"])
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION | tauri_plugin_window_state::StateFlags::SIZE)
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(|app| {
            let handle = app.handle().clone();
            let data = app.path().app_data_dir()?;
            let settings = SettingsStore::load(data.join("settings.json"));
            let base = url::Url::parse(lyrics::lrclib::DEFAULT_BASE).expect("lrclib url");
            let lyrics = Arc::new(LyricsService::new(data.join("lyrics"), data.join("cache").join("lyrics"), LrclibClient::new(base)));
            let hub = Arc::new(Hub::new(Snapshot::default()));
            let (tx, rx) = mpsc::channel(64);
            let (spotify_used, _) = watch::channel(false);
            let port = settings.get().port;
            let spotify = Arc::new(spotify_auth::SpotifyAuth::new());
            let roman = Arc::new(roman::Romanizer::new());
            let packs = Arc::new(roman::packs::PackManager::new(data.join("packs"), roman::packs::manifest(), roman.clone()));
            let ctx = Arc::new(AppCtx {
                settings,
                hub: hub.clone(),
                lyrics,
                server: Mutex::new((ServerStatus::Failed { port, message: "starting".into() }, None)),
                oauth: spotify.clone(),
                spotify: spotify.clone(),
                playback_tx: tx.clone(),
                spotify_used,
                system_changed: Arc::new(Notify::new()),
                roman,
                packs,
            });
            ctx.sync_settings();
            app.manage(ctx.clone());

            let s = ctx.settings.get();
            if let Err(e) = shell::register_hotkey(&handle, None, &s.hotkey) {
                tracing::warn!("hotkey {} unavailable: {e}", s.hotkey);
            }
            shell::setup_tray(&handle)?;
            if s.floating_window {
                let _ = shell::floating_window(&handle, true, s.port);
            }
            if std::env::args().any(|a| a == "--minimized")
                && let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }

            let tx_spotify = tx.clone();
            playback::spawn_system_source(tx, hub.clone());
            tauri::async_runtime::spawn(run_pipeline(ctx.clone(), rx));
            tauri::async_runtime::spawn(forward_to_panel(handle.clone(), hub));
            {
                let c = ctx.clone();
                tauri::async_runtime::spawn(playback::spotify::run_poller(
                    spotify.clone(),
                    move || c.settings.get().spotify_client_id.filter(|s| !s.trim().is_empty()),
                    tx_spotify,
                    ctx.spotify_used.subscribe(),
                    ctx.system_changed.clone(),
                ));
                let mut rx = spotify.watch_connected();
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    while rx.changed().await.is_ok() {
                        let on = *rx.borrow_and_update();
                        let _ = tauri::Emitter::emit(&h, "spotify-status", on);
                    }
                });
            }
            let c = ctx.clone();
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                c.restart_server(&h).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main" {
                    let ctx = window.state::<Arc<AppCtx>>();
                    if ctx.settings.get().minimize_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_settings,
            commands::list_packs,
            commands::install_pack,
            commands::remove_pack,
            commands::cancel_pack_install,
            commands::update_settings,
            commands::set_mode,
            commands::toggle_mode,
            commands::nudge_track_offset,
            commands::reset_track_offset,
            commands::get_server_status,
            commands::restart_server,
            commands::clear_lyrics_cache,
            commands::open_lyrics_folder,
            commands::list_system_fonts,
            commands::set_floating_window,
            commands::show_main,
            commands::spotify_connect,
            commands::spotify_disconnect,
            commands::spotify_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod smoke {
    #[test]
    fn smoke() {
        assert_eq!(2, 1 + 1);
    }
}
