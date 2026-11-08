//! Backup ways of seeing the wheel hotkey (and quick-bind keys), for when the
//! global shortcut alone doesn't fire in game.
//!
//! `RegisterHotKey` (the global-shortcut plugin) can go quiet while the game is
//! focused: a game reading the keyboard through Raw Input with
//! `RIDEV_NOHOTKEYS` switches hotkeys off, and Windows won't deliver input from
//! an elevated (admin) window to a non-elevated app. So on Windows a watcher
//! thread also listens through:
//!
//! 1. a low-level keyboard hook (`WH_KEYBOARD_LL`),
//! 2. Raw Input with `RIDEV_INPUTSINK` (keys arrive even while unfocused),
//! 3. polling `GetAsyncKeyState`, the last resort, which only speaks when
//!    the event sources have been silent for a moment.
//!
//! The voice talk key is seen only through these backup sources, never as a
//! global shortcut: it's watched, not taken, so it still reaches the game and
//! can double as Discord's push-to-talk key.
//!
//! Every source reports to one [`Tracker`], which turns the overlapping
//! reports into a single press and a single release per key. The rest of the
//! app sees only those edges, via the sink given to [`init`].

#[cfg(windows)]
mod windows;

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{LazyLock, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri_plugin_global_shortcut::{Code, Shortcut};

/// After an event source reports a key, polling keeps quiet about that key
/// for this long. The hook runs before Windows updates the key state that
/// polling reads, so polling briefly disagrees with it.
const POLL_QUIET: Duration = Duration::from_millis(150);

/// Which key an edge is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// The wheel hotkey.
    Wheel,
    /// A quick-bind key, while the wheel hotkey is held.
    Quick(Code),
    /// The push-to-talk key for voice commands.
    Voice,
}

/// Where a report came from. Shown in the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The global-shortcut plugin (`RegisterHotKey`).
    Shortcut,
    /// The low-level keyboard hook.
    Hook,
    /// Raw Input.
    RawInput,
    /// `GetAsyncKeyState` polling.
    Poll,
}

/// A real press or release, after de-duplication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub key: Key,
    pub pressed: bool,
    pub source: Source,
}

/// What the watcher should look for. Set by hotkey.rs.
#[derive(Clone, Default)]
pub struct Target {
    /// The wheel hotkey, `None` while paused.
    pub wheel: Option<Shortcut>,
    /// Quick-bind keys, non-empty only while the wheel hotkey is held.
    pub armed: Vec<Code>,
    /// The voice talk key, `None` while voice is off or paused. Any modifiers.
    pub voice: Option<Code>,
}

#[derive(Default)]
struct KeyState {
    down: bool,
    /// Last report from a source other than polling.
    last_event: Option<Instant>,
}

/// Merges reports from all sources into one press/release per key.
#[derive(Default)]
pub struct Tracker {
    keys: HashMap<Key, KeyState>,
}

impl Tracker {
    /// Takes one report. Returns the edge to act on, or `None` for a repeat,
    /// a duplicate from another source, or polling that should stay quiet.
    pub fn report(&mut self, key: Key, pressed: bool, source: Source, now: Instant) -> Option<Edge> {
        let state = self.keys.entry(key).or_default();
        if source == Source::Poll {
            if state.last_event.is_some_and(|t| now - t < POLL_QUIET) {
                return None;
            }
        } else {
            state.last_event = Some(now);
        }
        if state.down == pressed {
            return None;
        }
        state.down = pressed;
        Some(Edge { key, pressed, source })
    }

    pub fn is_down(&self, key: Key) -> bool {
        self.keys.get(&key).is_some_and(|s| s.down)
    }

    /// Forgets keys that are no longer watched, so a stale "down" can't
    /// swallow the next press.
    fn retain(&mut self, target: &Target) {
        self.keys.retain(|key, _| match key {
            Key::Wheel => target.wheel.is_some(),
            Key::Quick(code) => target.armed.contains(code),
            Key::Voice => target.voice.is_some(),
        });
    }
}

type Sink = Box<dyn Fn(Edge) + Send + Sync>;

struct Shared {
    tracker: Tracker,
    target: Target,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(|| {
    Mutex::new(Shared {
        tracker: Tracker::default(),
        target: Target::default(),
    })
});
static SINK: OnceLock<Sink> = OnceLock::new();

/// Sources in use. All of them unless `SUMMS_WHEEL_SOURCES` lists some
/// (e.g. `poll` or `hook,raw`), which is for testing each fallback alone.
static ENABLED: LazyLock<Vec<Source>> = LazyLock::new(|| {
    let all = vec![Source::Shortcut, Source::Hook, Source::RawInput, Source::Poll];
    let Ok(list) = std::env::var("SUMMS_WHEEL_SOURCES") else {
        return all;
    };
    all.into_iter()
        .filter(|s| {
            let name = match s {
                Source::Shortcut => "shortcut",
                Source::Hook => "hook",
                Source::RawInput => "raw",
                Source::Poll => "poll",
            };
            list.split(',').any(|item| item.trim().eq_ignore_ascii_case(name))
        })
        .collect()
});

/// Whether a source is in use (see `ENABLED`).
pub fn enabled(source: Source) -> bool {
    ENABLED.contains(&source)
}

/// Starts the backup sources. `sink` gets every real edge, from any thread.
pub fn init(log_path: Option<PathBuf>, sink: impl Fn(Edge) + Send + Sync + 'static) {
    log::open(log_path);
    log::line(&format!("sources enabled: {:?}", *ENABLED));
    let _ = SINK.set(Box::new(sink));
    #[cfg(windows)]
    windows::start();
}

/// Reports a key from any source. Duplicates are dropped here; the edge
/// passed on to the sink, if any, is returned.
pub fn report(key: Key, pressed: bool, source: Source) -> Option<Edge> {
    if !ENABLED.contains(&source) {
        return None;
    }
    let edge = {
        let mut shared = SHARED.lock().unwrap();
        shared.tracker.report(key, pressed, source, Instant::now())
    }?;
    // The lock is released first: the sink may change the target (arm/disarm).
    log::line(&format!(
        "{:?} {} via {:?}",
        edge.key,
        if edge.pressed { "down" } else { "up" },
        edge.source
    ));
    if let Some(sink) = SINK.get() {
        sink(edge);
    }
    Some(edge)
}

/// Sets the wheel hotkey to watch (`None` while paused).
pub fn set_wheel(wheel: Option<Shortcut>) {
    let mut shared = SHARED.lock().unwrap();
    shared.target.wheel = wheel;
    let Shared { tracker, target } = &mut *shared;
    tracker.retain(target);
}

/// Sets the quick-bind keys to watch (empty when the hotkey is released).
pub fn set_armed(armed: Vec<Code>) {
    let mut shared = SHARED.lock().unwrap();
    shared.target.armed = armed;
    let Shared { tracker, target } = &mut *shared;
    tracker.retain(target);
}

/// Sets the voice talk key to watch (`None` while voice is off or paused).
pub fn set_voice(voice: Option<Code>) {
    let mut shared = SHARED.lock().unwrap();
    shared.target.voice = voice;
    let Shared { tracker, target } = &mut *shared;
    tracker.retain(target);
}

/// A copy of what to watch, plus whether the wheel key is currently down.
fn snapshot() -> (Target, bool) {
    let shared = SHARED.lock().unwrap();
    (shared.target.clone(), shared.tracker.is_down(Key::Wheel))
}

/// Restarts the app with admin rights (the fix when the game runs as admin). Windows only.
pub fn restart_elevated() -> Result<(), String> {
    #[cfg(windows)]
    return windows::restart_elevated();
    #[cfg(not(windows))]
    Err("only available on Windows".into())
}

/// Puts the overlay back on top of other always-on-top windows. Windows only.
pub fn bring_to_top(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        windows::bring_to_top(hwnd.0 as _);
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// Whether this process already has admin rights.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    return windows::is_elevated();
    #[cfg(not(windows))]
    true
}

/// A small text log of what each input source saw (`input.log` next to
/// config.json), so "nothing happens" can be traced to a source.
pub mod log {
    use super::*;

    /// Stop writing after this many lines so a long session can't fill the disk.
    const MAX_LINES: u32 = 5000;

    static FILE: Mutex<Option<(File, u32, Instant)>> = Mutex::new(None);

    pub(super) fn open(path: Option<PathBuf>) {
        let Some(path) = path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Truncated on every start: only the current session matters.
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(&path);
        if let Ok(file) = file {
            *FILE.lock().unwrap() = Some((file, 0, Instant::now()));
        }
    }

    pub fn line(text: &str) {
        let mut guard = FILE.lock().unwrap();
        let Some((file, count, start)) = guard.as_mut() else {
            eprintln!("{text}");
            return;
        };
        if *count >= MAX_LINES {
            return;
        }
        *count += 1;
        let secs = start.elapsed().as_secs_f64();
        let _ = writeln!(file, "[{secs:9.3}s] {text}");
        let _ = file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(start: Instant, n: u64) -> Instant {
        start + Duration::from_millis(n)
    }

    #[test]
    fn duplicates_from_several_sources_make_one_edge() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        let e = t.report(Key::Wheel, true, Source::Hook, ms(t0, 0));
        assert_eq!(e.map(|e| e.source), Some(Source::Hook));
        // The same press from the other sources, and key repeat: nothing new.
        assert_eq!(t.report(Key::Wheel, true, Source::RawInput, ms(t0, 1)), None);
        assert_eq!(t.report(Key::Wheel, true, Source::Shortcut, ms(t0, 2)), None);
        assert_eq!(t.report(Key::Wheel, true, Source::Hook, ms(t0, 30)), None);
        // One release, whoever sees it first.
        assert!(t.report(Key::Wheel, false, Source::Shortcut, ms(t0, 400)).is_some());
        assert_eq!(t.report(Key::Wheel, false, Source::Hook, ms(t0, 401)), None);
    }

    #[test]
    fn polling_waits_for_events_to_go_quiet() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        t.report(Key::Wheel, true, Source::Hook, ms(t0, 0));
        // Key state lags the hook: an "up" poll right after the press is ignored.
        assert_eq!(t.report(Key::Wheel, false, Source::Poll, ms(t0, 10)), None);
        assert!(t.is_down(Key::Wheel));
        // A release the events missed is caught once they've been quiet a while.
        let e = t.report(Key::Wheel, false, Source::Poll, ms(t0, 500));
        assert_eq!(e.map(|e| (e.pressed, e.source)), Some((false, Source::Poll)));
    }

    #[test]
    fn polling_alone_still_works() {
        // Every event source blocked: polling carries the key by itself.
        let t0 = Instant::now();
        let mut t = Tracker::default();
        assert!(t.report(Key::Wheel, true, Source::Poll, ms(t0, 0)).is_some());
        assert_eq!(t.report(Key::Wheel, true, Source::Poll, ms(t0, 15)), None);
        assert!(t.report(Key::Wheel, false, Source::Poll, ms(t0, 600)).is_some());
    }

    #[test]
    fn unwatched_keys_are_forgotten() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        t.report(Key::Quick(Code::KeyT), true, Source::Hook, ms(t0, 0));
        t.retain(&Target::default());
        // Without the reset this press would look like a repeat.
        assert!(t
            .report(Key::Quick(Code::KeyT), true, Source::Hook, ms(t0, 5))
            .is_some());
    }
}
