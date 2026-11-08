//! The global hotkey that opens the wheel, and Right Shift + that hotkey,
//! which opens the settings window.

use crate::{config::DEFAULT_HOTKEY, settings};
use std::sync::Mutex;
use tauri::{plugin::TauriPlugin, AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The hotkey in use and the shortcuts currently registered for it.
#[derive(Default)]
pub struct Hotkeys(Mutex<Bound>);

#[derive(Default)]
struct Bound {
    /// Config string of the hotkey in use, e.g. "Backquote".
    hotkey: String,
    /// Both are `None` while paused (settings window open).
    wheel: Option<Shortcut>,
    settings: Option<Shortcut>,
}

/// The wheel hotkey is forwarded to the frontend as a `hotkey` event: `true`
/// on press, `false` on release. Both edges matter: press opens the wheel,
/// release picks the role.
///
/// The settings shortcut is the hotkey plus Shift. Global shortcuts can't tell
/// the two Shift keys apart, so the side is checked when it fires.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let pressed = event.state() == ShortcutState::Pressed;
            let (wheel, settings) = with_bound(app, |b| (b.wheel, b.settings));
            if pressed && Some(*shortcut) == settings && right_shift_down() {
                settings::open(app);
            } else if Some(*shortcut) == wheel {
                let _ = app.emit("hotkey", pressed);
            }
        })
        .build()
}

/// Registers the configured hotkey at startup and returns a label for the tray tooltip.
///
/// A bad or already-taken hotkey (e.g. a second copy running) must not crash
/// the app: fall back to the default, else run without a hotkey.
pub fn init(app: &AppHandle, wanted: &str) -> String {
    set_current(app, wanted);
    match register(app, wanted) {
        Ok(()) => wanted.to_string(),
        Err(err) => {
            eprintln!("can't register hotkey {wanted:?}: {err}");
            if wanted != DEFAULT_HOTKEY && apply(app, DEFAULT_HOTKEY).is_ok() {
                DEFAULT_HOTKEY.to_string()
            } else {
                format!("{wanted} (unavailable: in use or invalid)")
            }
        }
    }
}

/// Makes `hotkey` the hotkey in use. Fails (keeping nothing registered) if it
/// can't be parsed or another app has it.
pub fn apply(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    register(app, hotkey)?;
    set_current(app, hotkey);
    Ok(())
}

/// The hotkey in use, as written in the config.
pub fn current(app: &AppHandle) -> String {
    with_bound(app, |b| b.hotkey.clone())
}

/// Releases the shortcuts so the settings window can capture them as ordinary key presses.
pub fn pause(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    with_bound(app, |b| {
        b.wheel = None;
        b.settings = None;
    });
}

/// Registers the hotkey in use again after `pause`.
pub fn resume(app: &AppHandle) {
    let hotkey = current(app);
    if let Err(err) = register(app, &hotkey) {
        eprintln!("can't register hotkey {hotkey:?}: {err}");
    }
}

fn set_current(app: &AppHandle, hotkey: &str) {
    with_bound(app, |b| b.hotkey = hotkey.to_string());
}

/// Replaces whatever is registered with `hotkey` and its settings shortcut.
///
/// The lock is never held across plugin calls: those run on the main thread,
/// which is also where the shortcut handler takes the lock.
fn register(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let wheel: Shortcut = hotkey.parse().map_err(|_| format!("{hotkey} isn't a supported key"))?;
    let settings = Shortcut::new(Some(wheel.mods | Modifiers::SHIFT), wheel.key);

    pause(app);
    let shortcuts = app.global_shortcut();
    shortcuts
        .register(wheel)
        .map_err(|_| format!("{hotkey} is already in use by another app"))?;
    // The wheel works without it, so a taken settings combo is only logged. If
    // the hotkey already includes Shift, both are the same shortcut.
    let settings = if settings == wheel || shortcuts.register(settings).is_ok() {
        Some(settings)
    } else {
        eprintln!("can't register the settings shortcut Shift+{hotkey}");
        None
    };

    with_bound(app, |b| {
        b.wheel = Some(wheel);
        b.settings = settings;
    });
    Ok(())
}

fn with_bound<T>(app: &AppHandle, f: impl FnOnce(&mut Bound) -> T) -> T {
    let state = app.state::<Hotkeys>();
    let mut bound = state.0.lock().unwrap();
    f(&mut bound)
}

#[cfg(windows)]
fn right_shift_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_RSHIFT};
    // The high bit is set while the key is down.
    unsafe { GetAsyncKeyState(i32::from(VK_RSHIFT)) < 0 }
}

/// Other platforms: no cheap way to tell the sides apart, so either Shift works.
#[cfg(not(windows))]
fn right_shift_down() -> bool {
    true
}
