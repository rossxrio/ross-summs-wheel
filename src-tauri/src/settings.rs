//! The settings window, opened by tapping the hotkey three times or from the tray.
//! Its page is `settings.html` / `src/settings/`.

use crate::{
    config::{self, Config},
    hotkey, tray, voice,
};
use tauri::{async_runtime, AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

const LABEL: &str = "settings";

/// Opens the settings window, or focuses it if it's already open. Called on
/// the main thread (hotkey taps, tray menu).
///
/// The global shortcuts are paused while it's open, so the page can capture
/// the current hotkey like any other key press. Pausing here and resuming in
/// the window's Destroyed handler, both on the main thread, keeps them in the
/// order they happened however fast the window is closed and reopened.
pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_focus();
        return;
    }
    hotkey::pause(app);
    // The mic opens for the "hold to test" button.
    voice::set_settings_open(app, true);
    let app = app.clone();
    // Building a window from a shortcut or menu handler can deadlock on Windows.
    async_runtime::spawn(async move {
        match build(&app) {
            Ok(window) => {
                let app = app.clone();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Destroyed) {
                        closed(&app);
                    }
                });
            }
            Err(err) => {
                eprintln!("can't open settings: {err}");
                // A second open racing the first fails here; only resume if no window won.
                if app.get_webview_window(LABEL).is_none() {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || closed(&handle));
                }
            }
        }
    });
}

/// The window is gone. Main thread only, like `open`.
fn closed(app: &AppHandle) {
    voice::set_settings_open(app, false);
    hotkey::resume(app);
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("Summs Wheel Settings")
        .inner_size(380.0, 640.0)
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

/// The settings window's "Quit" button; same as the tray's.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
