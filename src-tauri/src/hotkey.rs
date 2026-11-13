//! Global shortcuts:
//! - the wheel hotkey (seen through the plugin and the backups in key_watch),
//! - three quick taps of the hotkey, which open the settings window,
//! - quick-bind keys, registered only while the hotkey is held so they reach
//!   the game normally the rest of the time,
//! - the voice talk key, only watched (key_watch), never registered.
//!
//! Any of these can be mouse button 4 or 5 instead of a key. The plugin can't
//! register those, so they're seen by key_watch's backup sources alone.

use crate::{
    config::{Config, DEFAULT_HOTKEY},
    key_watch::{self, Button, Edge, Hotkey, Key, Source},
    overlay,
    quick_binds::{Matcher, QuickBind},
    settings,
    voice::{self, VoiceConfig},
};
use std::{
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};
use tauri::{plugin::TauriPlugin, AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// A press shorter than this counts as a tap.
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
    voice: VoiceConfig,
    sounds: bool,
    /// `None` while paused (settings window open).
    wheel: Option<Hotkey>,
    taps: Taps,
    /// Quick-bind keys taken while the wheel hotkey is held.
    armed_keys: Vec<Button>,
    /// The ones the plugin managed to register (the backups watch them all).
    armed: Vec<Shortcut>,
}

/// Every way of seeing the keys (this plugin and the backups in key_watch)
/// reports to `key_watch`, which passes each real press and release on to
/// `on_edge` exactly once.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let pressed = event.state() == ShortcutState::Pressed;
            let key = with_bound(app, |b| {
                if Some(*shortcut) == b.wheel.and_then(Hotkey::shortcut) {
                    Some(Key::Wheel)
                } else if b.armed.contains(shortcut) {
                    Some(Key::Quick(Button::Key(shortcut.key)))
                } else {
                    None
                }
            });
            if let Some(key) = key {
                key_watch::report(key, pressed, Source::Shortcut);
            }
        })
        .build()
}

/// Starts the backup key sources. Call before `init`.
pub fn watch(app: &AppHandle) {
    let log_path = app.path().app_config_dir().ok().map(|dir| dir.join("input.log"));
    let handle = app.clone();
    key_watch::init(log_path, move |edge| {
        // Edges come from several threads; handle them in order on the main one.
        // The time is taken here: voice timers count from the real key press.
        let at = Instant::now();
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || on_edge(&app, edge, at));
    });
}

/// The wheel hotkey is forwarded to the frontend as a `hotkey` event: `true`
/// on press, `false` on release. Both edges matter: press opens the wheel,
/// release picks the role. A completed quick bind is sent as `quick-bind`.
///
/// Tapping the hotkey three times opens settings instead. The first two taps
/// open and cancel the wheel, which is harmless. While the overlay is hidden,
/// that's all the hotkey does.
fn on_edge(app: &AppHandle, edge: Edge, at: Instant) {
    let now = Instant::now();
    match edge.key {
        Key::Wheel => {
            if edge.pressed {
                if with_bound(app, |b| b.taps.press(now)) {
                    settings::open(app);
                    return;
                }
                // Hidden from settings: only the taps above still count.
                if !overlay::is_shown(app) {
                    return;
                }
                arm(app);
            } else {
                with_bound(app, |b| b.taps.release(now));
                disarm(app);
            }
            let _ = app.emit("hotkey", edge.pressed);
        }
        Key::Quick(button) if edge.pressed => {
            let hit = with_bound(app, |b| {
                b.armed_keys.contains(&button).then(|| b.matcher.push(button)).flatten()
            });
            if let Some(target) = hit {
                let _ = app.emit("quick-bind", target);
            }
        }
        Key::Quick(_) => {}
        Key::Voice => voice::talk(app, edge.pressed, at, false),
    }
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
    use_settings(app, cfg, matcher);

    let wanted = &cfg.hotkey;
    let label = match register(app, wanted) {
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
    };
    watch_voice(app);
    label
}

/// Puts new settings from the settings window into use. Fails if a key can't
/// be parsed or another app has the hotkey.
///
/// The settings window is still open, so everything stays paused afterwards
/// (registering is only the check that the hotkey is free); closing the
/// window resumes with the new settings.
///
/// Called from a command thread; the work is done on the main thread (see `on_main`).
pub fn apply(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let matcher = Matcher::new(&cfg.quick_binds)?;
    if !cfg.voice.key.is_empty() {
        (cfg.voice.key.parse::<Button>()).map_err(|_| format!("{} can't be the talk key", cfg.voice.key))?;
    }
    let cfg = cfg.clone();
    on_main(app, move |app| {
        register(app, &cfg.hotkey)?;
        pause(app);
        use_settings(app, &cfg, matcher);
        Ok(())
    })?
}

/// Runs `f` on the main thread and waits for it.
///
/// Shortcuts must only be changed there. The plugin holds its lock while it
/// waits for the main thread to (un)register, and the main thread takes that
/// same lock for every shortcut press: changed from any other thread, one
/// key press at the wrong moment freezes the whole app. On the main thread
/// the plugin's calls run inline instead, and changes happen in order.
fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&handle));
    })
    .map_err(|err| err.to_string())?;
    rx.recv().map_err(|err| err.to_string())
}

/// Stores the settings in use and hands the voice ones on.
fn use_settings(app: &AppHandle, cfg: &Config, matcher: Matcher) {
    with_bound(app, |b| {
        b.hotkey = cfg.hotkey.clone();
        b.quick_binds = cfg.quick_binds.clone();
        b.matcher = matcher;
        b.voice = cfg.voice.clone();
        b.sounds = cfg.sounds;
    });
    voice::apply(app, &cfg.voice);
}

/// The settings in use.
pub fn current(app: &AppHandle) -> Config {
    with_bound(app, |b| Config {
        hotkey: b.hotkey.clone(),
        quick_binds: b.quick_binds.clone(),
        voice: b.voice.clone(),
        sounds: b.sounds,
    })
}

/// The speaker button: sounds on or off, separately from the rest of the settings.
pub fn set_sounds(app: &AppHandle, on: bool) {
    with_bound(app, |b| b.sounds = on);
}

/// Releases every shortcut so the settings window can capture keys normally.
/// Main thread only (see `on_main`).
pub fn pause(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    with_bound(app, |b| {
        b.wheel = None;
        b.armed.clear();
        b.armed_keys.clear();
    });
    key_watch::set_wheel(None);
    key_watch::set_armed(vec![]);
    key_watch::set_voice(None);
}

/// Registers the hotkey in use again after `pause`. Main thread only (see `on_main`).
pub fn resume(app: &AppHandle) {
    let hotkey = with_bound(app, |b| b.hotkey.clone());
    if let Err(err) = register(app, &hotkey) {
        eprintln!("can't register hotkey {hotkey:?}: {err}");
    }
    watch_voice(app);
}

/// Starts watching the talk key, if voice is on. `register` pauses it.
fn watch_voice(app: &AppHandle) {
    let key = with_bound(app, |b| {
        let v = &b.voice;
        (v.enabled && !v.key.is_empty()).then(|| v.key.clone())
    });
    key_watch::set_voice(key.and_then(|k| k.parse::<Button>().ok()));
}

/// Replaces whatever is registered with `hotkey`.
///
/// The lock is never held across plugin calls: those run on the main thread,
/// which is also where the shortcut handler takes the lock.
fn register(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let wheel: Hotkey = hotkey.parse().map_err(|_| format!("{hotkey} isn't a supported key"))?;
    pause(app);
    // A mouse button has no shortcut: the backups alone watch it.
    if let Some(shortcut) = wheel.shortcut().filter(|_| key_watch::enabled(Source::Shortcut)) {
        app.global_shortcut()
            .register(shortcut)
            .map_err(|_| format!("{hotkey} is already in use by another app"))?;
    }
    with_bound(app, |b| {
        b.wheel = Some(wheel);
        b.taps = Taps::default();
    });
    key_watch::set_wheel(Some(wheel));
    Ok(())
}

/// Takes the quick-bind keys while the hotkey is held. They carry the
/// hotkey's modifiers: holding Ctrl+Q and pressing T arrives as Ctrl+T.
/// Mouse buttons in binds are taken by the mouse hook instead.
fn arm(app: &AppHandle) {
    let (keys, wheel) = with_bound(app, |b| {
        b.matcher.reset();
        (b.matcher.keys(), b.wheel)
    });
    let Some(wheel) = wheel else { return };
    let keys: Vec<Button> = keys.into_iter().filter(|key| *key != wheel.button).collect();
    let shortcuts = app.global_shortcut();
    let armed: Vec<Shortcut> = keys
        .iter()
        .filter_map(|key| match key {
            Button::Key(code) => Some(Shortcut::new(Some(wheel.mods), *code)),
            _ => None,
        })
        .filter(|s| key_watch::enabled(Source::Shortcut) && shortcuts.register(*s).is_ok())
        .collect();
    key_watch::set_armed(keys.clone());
    with_bound(app, |b| {
        b.armed = armed;
        b.armed_keys = keys;
    });
}

/// Gives the quick-bind keys back to the game.
fn disarm(app: &AppHandle) {
    key_watch::set_armed(vec![]);
    let armed = with_bound(app, |b| {
        b.armed_keys.clear();
        std::mem::take(&mut b.armed)
    });
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
