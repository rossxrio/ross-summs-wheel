//! The settings window, opened by tapping the hotkey three times or from the tray.
//! Its page is `settings.html` / `src/settings/`.

use crate::{
    config::{self, Config},
    hotkey, tray,
};
use tauri::{async_runtime, AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

const LABEL: &str = "settings";

/// Opens the settings window, or focuses it if it's already open.
///
/// The global shortcuts are paused while it's open, so the page can capture
/// the current hotkey like any other key press.
pub fn open(app: &AppHandle) {
    let app = app.clone();
    // Building a window from a shortcut or menu handler can deadlock on Windows.
    async_runtime::spawn(async move {
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.set_focus();
            return;
        }
        hotkey::pause(&app);
        match build(&app) {
            Ok(window) => {
                let app = app.clone();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Destroyed) {
                        let app = app.clone();
                        async_runtime::spawn(async move { hotkey::resume(&app) });
                    }
                });
            }
            Err(err) => {
                eprintln!("can't open settings: {err}");
                // A second open racing the first fails here; only resume if no window won.
                if app.get_webview_window(LABEL).is_none() {
                    hotkey::resume(&app);
                }
            }
        }
    });
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("Summs Wheel Settings")
        .inner_size(380.0, 470.0)
        .center()
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .focused(true)
        .build()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Config {
    hotkey::current(&app)
}

/// Puts the settings into use right away and saves them. The error is shown to the user.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Config) -> Result<(), String> {
    hotkey::apply(&app, &settings)?;
    config::save(&app, &settings)?;
    tray::show_hotkey(&app, &settings.hotkey);
    Ok(())
}
