pub mod app;
pub mod commands;
pub mod hub;
pub mod lrc;
pub mod lyrics;
pub mod model;
pub mod normalize;
pub mod playback;
pub mod server;
pub mod settings;
pub mod shell;

use app::{forward_to_panel, run_pipeline, AppCtx};
use hub::{Hub, Snapshot};
use lyrics::{LrclibClient, LyricsService};
use server::{NoSpotify, ServerStatus};
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
            let ctx = Arc::new(AppCtx {
                settings,
                hub: hub.clone(),
                lyrics,
                server: Mutex::new((ServerStatus::Failed { port, message: "starting".into() }, None)),
                oauth: Arc::new(NoSpotify),
                playback_tx: tx.clone(),
                spotify_used,
                windows_changed: Arc::new(Notify::new()),
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
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            playback::windows::spawn_windows_source(tx, hub.clone());
            tauri::async_runtime::spawn(run_pipeline(ctx.clone(), rx));
            tauri::async_runtime::spawn(forward_to_panel(handle.clone(), hub));
            let c = ctx.clone();
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                c.restart_server(&h).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let ctx = window.state::<Arc<AppCtx>>();
                    if ctx.settings.get().minimize_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_settings,
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
