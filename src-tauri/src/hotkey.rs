//! Global shortcuts:
//! - the wheel hotkey,
//! - three quick taps of the hotkey, which open the settings window,
//! - quick-bind keys, registered only while the hotkey is held so they reach
//!   the game normally the rest of the time.

use crate::{
    config::{Config, DEFAULT_HOTKEY},
    quick_binds::{Matcher, QuickBind},
    settings,
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{plugin::TauriPlugin, AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// A press shorter than this counts as a tap (release is polled every 50ms).
const TAP_HOLD: Duration = Duration::from_millis(250);
/// Taps further apart than this (release to next press) don't add up.
const TAP_GAP: Duration = Duration::from_millis(400);

/// The settings in use and the shortcuts currently registered for them.
#[derive(Default)]
pub struct Hotkeys(Mutex<Bound>);

#[derive(Default)]
struct Bound {
    /// Config string of the hotkey in use, e.g. "Backquote".
    hotkey: String,
    quick_binds: Vec<QuickBind>,
    matcher: Matcher,
    /// `None` while paused (settings window open).
    wheel: Option<Shortcut>,
    taps: Taps,
    /// Quick-bind keys, registered while the wheel hotkey is held.
    armed: Vec<Shortcut>,
}

/// The wheel hotkey is forwarded to the frontend as a `hotkey` event: `true`
/// on press, `false` on release. Both edges matter: press opens the wheel,
/// release picks the role. A completed quick bind is sent as `quick-bind`.
///
/// Tapping the hotkey three times opens settings instead. The first two taps
/// open and cancel the wheel, which is harmless.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let pressed = event.state() == ShortcutState::Pressed;
            let now = Instant::now();
            if Some(*shortcut) == with_bound(app, |b| b.wheel) {
                if pressed {
                    if with_bound(app, |b| b.taps.press(now)) {
                        settings::open(app);
                        return;
                    }
                    arm(app);
                } else {
                    with_bound(app, |b| b.taps.release(now));
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

/// Replaces whatever is registered with `hotkey`.
///
/// The lock is never held across plugin calls: those run on the main thread,
/// which is also where the shortcut handler takes the lock.
fn register(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let wheel: Shortcut = hotkey.parse().map_err(|_| format!("{hotkey} isn't a supported key"))?;
    pause(app);
    app.global_shortcut()
        .register(wheel)
        .map_err(|_| format!("{hotkey} is already in use by another app"))?;
    with_bound(app, |b| {
        b.wheel = Some(wheel);
        b.taps = Taps::default();
    });
    Ok(())
}

/// Takes the quick-bind keys while the hotkey is held. They carry the
/// hotkey's modifiers: holding Ctrl+Q and pressing T arrives as Ctrl+T.
fn arm(app: &AppHandle) {
    let (keys, wheel) = with_bound(app, |b| {
        b.matcher.reset();
        (b.matcher.keys(), b.wheel)
    });
    let Some(wheel) = wheel else { return };
    let shortcuts = app.global_shortcut();
    let armed: Vec<Shortcut> = keys
        .into_iter()
        .map(|key| Shortcut::new(Some(wheel.mods), key))
        .filter(|s| *s != wheel)
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

/// Spots three quick taps of the hotkey in a row.
///
/// Normal wheel use doesn't trip it: opening the wheel is a hold-and-flick,
/// longer than a tap.
#[derive(Default)]
struct Taps {
    /// Quick taps so far in the current run.
    count: u32,
    pressed_at: Option<Instant>,
    released_at: Option<Instant>,
}

impl Taps {
    /// True when this press completes three taps (the third counts on press, for snappiness).
    fn press(&mut self, now: Instant) -> bool {
        if !self.released_at.is_some_and(|r| now - r < TAP_GAP) {
            self.count = 0;
        }
        if self.count >= 2 {
            *self = Self::default();
            return true;
        }
        self.pressed_at = Some(now);
        false
    }

    fn release(&mut self, now: Instant) {
        let quick = self.pressed_at.take().is_some_and(|p| now - p < TAP_HOLD);
        self.count = if quick { self.count + 1 } else { 0 };
        self.released_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays (hold ms, gap ms before the next press) and reports which presses opened settings.
    fn run(taps: &[(u64, u64)]) -> Vec<bool> {
        let mut t = Taps::default();
        let mut now = Instant::now();
        let mut opened = vec![];
        for (hold, gap) in taps {
            opened.push(t.press(now));
            now += Duration::from_millis(*hold);
            t.release(now);
            now += Duration::from_millis(*gap);
        }
        opened
    }

    #[test]
    fn three_quick_taps_open_settings() {
        assert_eq!(run(&[(80, 150), (80, 150), (80, 0)]), [false, false, true]);
        // Counting starts afresh afterwards.
        let five = [(80, 150), (80, 150), (80, 150), (80, 150), (80, 0)];
        assert_eq!(run(&five), [false, false, true, false, false]);
    }

    #[test]
    fn holds_and_slow_taps_dont() {
        // Normal wheel use: a hold to flick, then taps.
        assert_eq!(run(&[(600, 150), (80, 150), (80, 0)]), [false, false, false]);
        // Too slow between taps.
        assert_eq!(run(&[(80, 150), (80, 700), (80, 0)]), [false, false, false]);
        // A hold in the middle resets the run.
        let reset = [(80, 150), (600, 150), (80, 150), (80, 150), (80, 0)];
        assert_eq!(run(&reset), [false, false, false, false, true]);
    }
}
