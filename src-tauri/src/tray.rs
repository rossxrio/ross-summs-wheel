//! System tray icon: the app has no window of its own to close, so this is
//! where the user opens settings, restarts and quits.

use crate::settings;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle,
};

const ID: &str = "main";

pub fn build(app: &App, hotkey_label: &str) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &restart, &quit])?;

    TrayIconBuilder::with_id(ID)
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip(tooltip(hotkey_label))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => settings::open(app),
            "restart" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
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
