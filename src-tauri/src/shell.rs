//! Desktop shell: tray icon, global hotkey and the floating read-along window.
use crate::app::AppCtx;
use crate::commands::{set_mode_inner, toggle_mode_inner};
use crate::settings::Mode;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show / Hide", true, None::<&str>)?;
    let focus = MenuItem::with_id(app, "mode-focus", "Focus", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "mode-widget", "Widget", true, None::<&str>)?;
    let modes = Submenu::with_items(app, "Overlay mode", true, &[&focus, &widget])?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit OpenMusix", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &modes, &sep, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("OpenMusix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            let ctx = app.state::<Arc<AppCtx>>();
            match ev.id().as_ref() {
                "show" => toggle_main(app),
                "mode-focus" => drop(set_mode_inner(&ctx, Mode::Focus)),
                "mode-widget" => drop(set_mode_inner(&ctx, Mode::Widget)),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Register `accel` as the mode-toggle hotkey, unregistering `previous` first.
pub fn register_hotkey(app: &AppHandle, previous: Option<&str>, accel: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    if let Some(p) = previous {
        let _ = gs.unregister(p);
    }
    gs.on_shortcut(accel, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            let ctx = app.state::<Arc<AppCtx>>();
            let _ = toggle_mode_inner(&ctx);
        }
    })
    .map_err(|e| e.to_string())
}

/// Show or close the always-on-top transparent read-along window.
pub fn floating_window(app: &AppHandle, on: bool, port: u16) -> tauri::Result<()> {
    let existing = app.get_webview_window("float");
    match (on, existing) {
        (true, Some(w)) => w.show(),
        (true, None) => {
            let url = WebviewUrl::App(format!("overlay.html?float=1&port={port}").into());
            WebviewWindowBuilder::new(app, "float", url)
                .title("OpenMusix Lyrics")
                .inner_size(520.0, 260.0)
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(true)
                .build()
                .map(|_| ())
        }
        (false, Some(w)) => w.close(),
        (false, None) => Ok(()),
    }
}
