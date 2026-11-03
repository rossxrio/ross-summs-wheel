//! The global hotkey that opens the wheel.

use crate::config::DEFAULT_HOTKEY;
use tauri::{plugin::TauriPlugin, App, Emitter, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Forwards the hotkey to the frontend as a `hotkey` event: `true` on press,
/// `false` on release. Both edges matter: press opens the wheel, release picks
/// the role. Only one shortcut is ever registered, so every event is ours.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            let pressed = event.state() == ShortcutState::Pressed;
            let _ = app.emit("hotkey", pressed);
        })
        .build()
}

/// Registers the configured hotkey and returns a label for the tray tooltip.
///
/// A bad or already-taken hotkey (e.g. a second copy running) must not crash
/// the app: fall back to the default, else run without a hotkey.
pub fn register(app: &App, wanted: &str) -> String {
    let shortcuts = app.global_shortcut();
    match shortcuts.register(wanted) {
        Ok(()) => wanted.to_string(),
        Err(err) => {
            eprintln!("can't register hotkey {wanted:?}: {err}");
            if wanted != DEFAULT_HOTKEY && shortcuts.register(DEFAULT_HOTKEY).is_ok() {
                DEFAULT_HOTKEY.to_string()
            } else {
                format!("{wanted} (unavailable: in use or invalid)")
            }
        }
    }
}
