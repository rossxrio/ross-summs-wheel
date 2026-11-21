//! System tray icon: the app has no window of its own to close, so this is
//! where the user opens settings, restarts and quits.

use crate::{hotkey, key_watch, settings};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle,
};

const ID: &str = "main";

/// What the tooltip shows: the hotkey, and whether League's fullscreen hides the overlay.
static TOOLTIP: Mutex<Tooltip> = Mutex::new(Tooltip {
    hotkey: String::new(),
    fullscreen_warning: false,
});

struct Tooltip {
    hotkey: String,
    fullscreen_warning: bool,
}

impl Tooltip {
    fn text(&self) -> String {
        if self.fullscreen_warning {
            // Windows cuts tooltips off at 127 characters.
            "Summs Wheel: League's exclusive fullscreen hides the overlay. Use Borderless.".into()
        } else {
            format!("Summs Wheel: hold {}", self.hotkey)
        }
    }
}

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

    let tooltip = update(|t| t.hotkey = hotkey_label.to_string());
    TrayIconBuilder::with_id(ID)
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip(tooltip)
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
    let text = update(|t| t.hotkey = hotkey_label.to_string());
    set_tooltip(app, text);
}

/// Says in the tooltip while League's exclusive fullscreen hides the overlay.
pub fn set_fullscreen_warning(app: &AppHandle, on: bool) {
    let text = update(|t| t.fullscreen_warning = on);
    set_tooltip(app, text);
}

fn update(change: impl FnOnce(&mut Tooltip)) -> String {
    let mut tooltip = TOOLTIP.lock().unwrap_or_else(|e| e.into_inner());
    change(&mut tooltip);
    tooltip.text()
}

fn set_tooltip(app: &AppHandle, text: String) {
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}
