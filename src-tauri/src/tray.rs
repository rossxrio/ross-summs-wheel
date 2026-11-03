//! System tray icon: the app has no window of its own to close, so this is
//! where the user edits the config, restarts and quits.

use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App,
};

pub fn build(app: &App, hotkey_label: &str, config_path: Option<PathBuf>) -> tauri::Result<()> {
    let edit = MenuItem::with_id(app, "config", "Edit config…", config_path.is_some(), None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart (apply config)", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&edit, &restart, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip(format!("Summs Wheel: hold {hotkey_label}"))
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "config" => {
                if let Some(path) = &config_path {
                    let _ = std::process::Command::new("notepad").arg(path).spawn();
                }
            }
            "restart" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
