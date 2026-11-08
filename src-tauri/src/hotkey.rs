//! Global shortcuts:
//! - the wheel hotkey,
//! - Right Shift + the hotkey, which opens the settings window,
//! - quick-bind keys, registered only while the hotkey is held so they reach
//!   the game normally the rest of the time.

use crate::{
    config::{Config, DEFAULT_HOTKEY},
    quick_binds::{Matcher, QuickBind},
    settings,
};
use std::sync::Mutex;
use tauri::{plugin::TauriPlugin, AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The settings in use and the shortcuts currently registered for them.
#[derive(Default)]
pub struct Hotkeys(Mutex<Bound>);

#[derive(Default)]
struct Bound {
    /// Config string of the hotkey in use, e.g. "Backquote".
    hotkey: String,
    quick_binds: Vec<QuickBind>,
    matcher: Matcher,
    /// Both are `None` while paused (settings window open).
    wheel: Option<Shortcut>,
    settings: Option<Shortcut>,
    /// Quick-bind keys, registered while the wheel hotkey is held.
    armed: Vec<Shortcut>,
}

/// The wheel hotkey is forwarded to the frontend as a `hotkey` event: `true`
/// on press, `false` on release. Both edges matter: press opens the wheel,
/// release picks the role. A completed quick bind is sent as `quick-bind`.
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
                if pressed {
                    arm(app);
                } else {
                    disarm(app);
                }
                let _ = app.emit("hotkey", pressed);
            } else if pressed {
                let hit = with_bound(app, |b| {
                    b.armed
                        .contains(shortcut)
                        .then(|| b.matcher.push(shortcut.key))
                        .flatten()
                });
                if let Some(target) = hit {
                    let _ = app.emit("quick-bind", target);
                }
            }
        })
        .build()
}

/// Registers the configured hotkey at startup and returns a label for the tray tooltip.
///
/// A bad or already-taken hotkey (e.g. a second copy running) must not crash
/// the app: fall back to the default, else run without a hotkey. Bad quick
/// binds are dropped from use but kept for the settings window to show.
pub fn init(app: &AppHandle, cfg: &Config) -> String {
    let matcher = Matcher::new(&cfg.quick_binds).unwrap_or_else(|err| {
        eprintln!("ignoring quick binds: {err}");
        Matcher::default()
    });
    with_bound(app, |b| {
        b.hotkey = cfg.hotkey.clone();
        b.quick_binds = cfg.quick_binds.clone();
        b.matcher = matcher;
    });

    let wanted = &cfg.hotkey;
    match register(app, wanted) {
        Ok(()) => wanted.to_string(),
        Err(err) => {
            eprintln!("can't register hotkey {wanted:?}: {err}");
            if wanted != DEFAULT_HOTKEY && register(app, DEFAULT_HOTKEY).is_ok() {
                with_bound(app, |b| b.hotkey = DEFAULT_HOTKEY.into());
                DEFAULT_HOTKEY.to_string()
            } else {
                format!("{wanted} (unavailable: in use or invalid)")
            }
        }
    }
}

/// Puts new settings into use. Fails (keeping nothing registered) if a key
/// can't be parsed or another app has the hotkey.
pub fn apply(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let matcher = Matcher::new(&cfg.quick_binds)?;
    register(app, &cfg.hotkey)?;
    with_bound(app, |b| {
        b.hotkey = cfg.hotkey.clone();
        b.quick_binds = cfg.quick_binds.clone();
        b.matcher = matcher;
    });
    Ok(())
}

/// The settings in use.
pub fn current(app: &AppHandle) -> Config {
    with_bound(app, |b| Config {
        hotkey: b.hotkey.clone(),
        quick_binds: b.quick_binds.clone(),
    })
}

/// Releases every shortcut so the settings window can capture keys normally.
pub fn pause(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    with_bound(app, |b| {
        b.wheel = None;
        b.settings = None;
        b.armed.clear();
    });
}

/// Registers the hotkey in use again after `pause`.
pub fn resume(app: &AppHandle) {
    let hotkey = with_bound(app, |b| b.hotkey.clone());
    if let Err(err) = register(app, &hotkey) {
        eprintln!("can't register hotkey {hotkey:?}: {err}");
    }
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

/// Takes the quick-bind keys while the hotkey is held. They carry the
/// hotkey's modifiers: holding Ctrl+Q and pressing T arrives as Ctrl+T.
fn arm(app: &AppHandle) {
    let (keys, wheel, settings) = with_bound(app, |b| {
        b.matcher.reset();
        (b.matcher.keys(), b.wheel, b.settings)
    });
    let Some(wheel) = wheel else { return };
    let shortcuts = app.global_shortcut();
    let armed: Vec<Shortcut> = keys
        .into_iter()
        .map(|key| Shortcut::new(Some(wheel.mods), key))
        .filter(|s| *s != wheel && Some(*s) != settings)
        .filter(|s| shortcuts.register(*s).is_ok())
        .collect();
    with_bound(app, |b| b.armed = armed);
}

/// Gives the quick-bind keys back to the game.
fn disarm(app: &AppHandle) {
    let armed = with_bound(app, |b| std::mem::take(&mut b.armed));
    if !armed.is_empty() {
        let _ = app.global_shortcut().unregister_multiple(armed);
    }
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
