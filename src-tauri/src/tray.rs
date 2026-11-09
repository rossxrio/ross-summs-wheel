//! System tray icon: the app has no window of its own to close, so this is
//! where the user opens settings, restarts and quits.

use crate::{hotkey, key_watch, settings};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle,
};

const ID: &str = "main";

pub fn build(app: &App, hotkey_label: &str) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart", true, None::<&str>)?;
    // When the game runs as admin, Windows hides its key presses from
    // non-admin apps; this is the fix if the hotkey still does nothing in game.
    let admin = MenuItem::with_id(
        app,
        "admin",
        "Restart as administrator",
        !key_watch::is_elevated(),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &restart, &admin, &quit])?;

    TrayIconBuilder::with_id(ID)
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip(tooltip(hotkey_label))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => settings::open(app),
            "restart" => app.restart(),
            "admin" => restart_elevated(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Frees the hotkey for the admin copy, then exits; on a UAC "no", carries on.
fn restart_elevated(app: &AppHandle) {
    hotkey::pause(app);
    match key_watch::restart_elevated() {
        Ok(()) => app.exit(0),
        Err(err) => {
            key_watch::log::line(&err);
            hotkey::resume(app);
        }
    }
}

/// Updates the tooltip after the hotkey changes.
pub fn show_hotkey(app: &AppHandle, hotkey_label: &str) {
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_tooltip(Some(tooltip(hotkey_label)));
    }
}

fn tooltip(hotkey_label: &str) -> String {
    format!("Summs Wheel: hold {hotkey_label}")
}
