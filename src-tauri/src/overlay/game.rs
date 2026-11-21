//! Keeps the overlay over League's game window, including in Fullscreen mode.
//!
//! In Fullscreen, League's window is *topmost* too, and it comes to the front
//! of the topmost windows whenever it's activated (game start, alt-tab back),
//! which leaves the HUD hidden behind the game. Windows' fullscreen
//! optimizations (on by default) run such a game as a borderless window, so a
//! window above it is drawn normally: the overlay only has to be put back on top.
//! The game can also run at its own resolution or on another monitor, so the
//! overlay is fitted to the monitor the game is on.
//!
//! True exclusive fullscreen (fullscreen optimizations turned off for the game)
//! is different: nothing outside the game can be drawn over it without hooking
//! into its rendering, which anti-cheat forbids, and covering it makes the game
//! drop out of fullscreen. Then the overlay stays out of the way and the wheel
//! doesn't open (quick binds and voice still work, with sounds).
//!
//! A watcher thread looks whenever the foreground window changes, and once a
//! second as a backstop. It never touches the overlay window itself: changes
//! are made on the main thread, which owns it, so a slow main thread can't
//! stall the watcher.

/// A screen rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> u32 {
        (self.right - self.left).max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }
}

/// What one look at the desktop found.
#[derive(Debug)]
pub struct Seen {
    /// The monitor the overlay should cover: the game's, else the primary one.
    pub monitor: Rect,
    /// Where the overlay window is now.
    pub overlay: Rect,
    /// League's game window is the foreground window.
    pub game_in_front: bool,
    /// The game window is above the overlay in z-order.
    pub game_above: bool,
    /// Some program holds a display in true exclusive fullscreen.
    pub exclusive: bool,
}

/// What to do about it.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    /// Move and resize the overlay to this.
    pub fit: Option<Rect>,
    /// Put the overlay back above the game.
    pub raise: bool,
    /// The game in front is in true exclusive fullscreen: the overlay can't show.
    pub exclusive: bool,
}

pub fn plan(seen: &Seen, wheel_open: bool) -> Plan {
    // Only the game in front can be the exclusive owner worth caring about.
    let exclusive = seen.game_in_front && seen.exclusive;
    Plan {
        // Moving the window under an open wheel would throw off its cursor
        // math, and an exclusive game can't show the overlay anyway.
        fit: (!wheel_open && !exclusive && seen.overlay != seen.monitor).then_some(seen.monitor),
        // Covering an exclusive-fullscreen game would knock it out of fullscreen.
        raise: seen.game_in_front && seen.game_above && !exclusive,
        exclusive,
    }
}

#[cfg(windows)]
pub use win::start;

#[cfg(windows)]
mod win {
    use super::{plan, Plan, Rect, Seen};
    use crate::key_watch::{self, log};
    use std::{
        ffi::c_void,
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
        sync::{
            atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
            LazyLock, Mutex, OnceLock,
        },
    };
    use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};
    use windows_sys::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
            MONITOR_DEFAULTTOPRIMARY,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Accessibility::{SetWinEventHook, HWINEVENTHOOK},
            WindowsAndMessaging::{
                DispatchMessageW, FindWindowW, GetForegroundWindow, GetMessageW, GetWindow, GetWindowRect,
                IsWindowVisible, SetTimer, EVENT_SYSTEM_FOREGROUND, GW_HWNDPREV, MSG, WINEVENT_OUTOFCONTEXT,
                WINEVENT_SKIPOWNPROCESS, WM_TIMER,
            },
        },
    };

    /// The game's window title (the class name isn't stable across patches).
    const GAME_TITLE: &str = "League of Legends (TM) Client";
    /// The backstop look, for changes no foreground event announces (a
    /// resolution change, the game re-raising itself).
    const CHECK_MS: u32 = 1000;
    /// Windows above the overlay to walk before giving up.
    const MAX_ZORDER_WALK: usize = 256;

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static OVERLAY: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    /// Set while a change waits for the main thread, so a busy main thread
    /// doesn't pile them up.
    static APPLYING: AtomicBool = AtomicBool::new(false);
    /// The last game window seen, to log when a game starts and ends.
    static LAST_GAME: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    /// Raises in the current game, to keep the log short if the game keeps re-raising itself.
    static RAISES: AtomicU32 = AtomicU32::new(0);
    /// The last size and place tried. If Windows keeps the overlay slightly
    /// off it (scaling), it isn't tried again every second until it changes.
    static TRIED_FIT: Mutex<Option<Rect>> = Mutex::new(None);

    pub fn start(app: AppHandle, overlay: HWND) {
        OVERLAY.store(overlay, Ordering::Release);
        let _ = APP.set(app);
        let spawned = std::thread::Builder::new()
            .name("game-watch".into())
            .spawn(|| unsafe { run() });
        if let Err(err) = spawned {
            log::line(&format!("can't start the game window watcher: {err}"));
        }
    }

    unsafe fn run() {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if hook.is_null() {
            log::line("foreground watch: FAILED (checking once a second only)");
        }
        // A thread timer: WM_TIMER arrives with no window, handled below.
        SetTimer(null_mut(), 0, CHECK_MS, None);
        check();

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_TIMER {
                check();
            }
            DispatchMessageW(&msg);
        }
    }

    unsafe extern "system" fn on_foreground(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        check();
    }

    fn check() {
        let overlay = OVERLAY.load(Ordering::Acquire);
        let Some(app) = APP.get() else { return };
        let Some((seen, game)) = (unsafe { look(overlay) }) else {
            return;
        };
        note_game(game, &seen);
        let mut next = plan(&seen, super::super::wheel_open());
        {
            let mut tried = TRIED_FIT.lock().unwrap_or_else(|e| e.into_inner());
            if seen.overlay == seen.monitor {
                *tried = None;
            }
            if next.fit.is_some() && next.fit == *tried {
                next.fit = None;
            } else if next.fit.is_some() {
                *tried = next.fit;
            }
        }

        let exclusive = next.exclusive;
        if super::super::set_exclusive(exclusive) {
            log::line(if exclusive {
                "League is in exclusive fullscreen: the overlay can't be drawn over it (use Borderless, or turn fullscreen optimizations back on)"
            } else {
                "League left exclusive fullscreen"
            });
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || crate::tray::set_fullscreen_warning(&handle, exclusive));
        }
        if next.fit.is_some() || next.raise {
            apply(app, next);
        }
    }

    /// Logs a game window appearing or going away.
    fn note_game(game: HWND, seen: &Seen) {
        let last = LAST_GAME.swap(game, Ordering::Relaxed);
        if last == game {
            return;
        }
        RAISES.store(0, Ordering::Relaxed);
        if game.is_null() {
            log::line("League game window closed");
        } else {
            let m = seen.monitor;
            log::line(&format!(
                "League game window found, on a {}x{} monitor at ({}, {})",
                m.width(),
                m.height(),
                m.left,
                m.top
            ));
        }
    }

    /// Hands the change to the main thread, which owns the overlay window.
    fn apply(app: &AppHandle, next: Plan) {
        if APPLYING.swap(true, Ordering::AcqRel) {
            return;
        }
        let handle = app.clone();
        let queued = app.run_on_main_thread(move || {
            APPLYING.store(false, Ordering::Release);
            let Some(window) = handle.get_webview_window(super::super::LABEL) else {
                return;
            };
            // Re-checked here: the wheel may have opened since the look.
            if let Some(r) = next.fit.filter(|_| !super::super::wheel_open()) {
                // Position first: crossing to a monitor with another scale
                // makes Windows resize the window, which the size then corrects.
                let moved = window
                    .set_position(PhysicalPosition::new(r.left, r.top))
                    .and_then(|_| window.set_size(PhysicalSize::new(r.width(), r.height())));
                log::line(&match moved {
                    Ok(()) => format!(
                        "overlay fitted to {}x{} at ({}, {})",
                        r.width(),
                        r.height(),
                        r.left,
                        r.top
                    ),
                    Err(err) => format!("can't fit the overlay: {err}"),
                });
            }
            if next.raise {
                key_watch::bring_to_top(&window);
                let n = RAISES.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n.is_multiple_of(100) {
                    log::line(&format!("overlay put back above the game (time {n} this game)"));
                }
            }
        });
        if queued.is_err() {
            APPLYING.store(false, Ordering::Release);
        }
    }

    /// One look at the desktop, plus the game window (null when there's none).
    /// `None` while the overlay is hidden or gone.
    unsafe fn look(overlay: HWND) -> Option<(Seen, HWND)> {
        if overlay.is_null() || IsWindowVisible(overlay) == 0 {
            return None;
        }
        static TITLE: LazyLock<Vec<u16>> = LazyLock::new(|| GAME_TITLE.encode_utf16().chain(Some(0)).collect());
        let game = FindWindowW(null(), TITLE.as_ptr());

        // For a minimized game Windows answers with where it was before.
        let monitor = if game.is_null() {
            MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(game, MONITOR_DEFAULTTONEAREST)
        };
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let mut rect: RECT = zeroed();
        if GetWindowRect(overlay, &mut rect) == 0 {
            return None;
        }

        let game_in_front = !game.is_null() && GetForegroundWindow() == game;
        let seen = Seen {
            monitor: rect_of(info.rcMonitor),
            overlay: rect_of(rect),
            game_in_front,
            game_above: game_in_front && is_above(game, overlay),
            exclusive: game_in_front && exclusive_owner(),
        };
        Some((seen, game))
    }

    fn rect_of(r: RECT) -> Rect {
        Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }

    /// Whether `window` is somewhere above `overlay` in z-order. The overlay
    /// is topmost, so only the few other topmost windows are walked.
    unsafe fn is_above(window: HWND, overlay: HWND) -> bool {
        let mut above = GetWindow(overlay, GW_HWNDPREV);
        for _ in 0..MAX_ZORDER_WALK {
            if above.is_null() {
                return false;
            }
            if above == window {
                return true;
            }
            above = GetWindow(above, GW_HWNDPREV);
        }
        false
    }

    /// Whether any program holds a display in true exclusive fullscreen.
    /// Games under fullscreen optimizations don't count: theirs is emulated,
    /// and windows above them are drawn normally.
    fn exclusive_owner() -> bool {
        type Check = unsafe extern "system" fn() -> u8;
        static CHECK: LazyLock<Option<Check>> = LazyLock::new(|| unsafe {
            let name: Vec<u16> = "gdi32.dll".encode_utf16().chain(Some(0)).collect();
            let gdi = GetModuleHandleW(name.as_ptr());
            if gdi.is_null() {
                return None;
            }
            // BOOLEAN D3DKMTCheckExclusiveOwnership(void), exported by gdi32 since Vista.
            GetProcAddress(gdi, c"D3DKMTCheckExclusiveOwnership".as_ptr().cast())
                .map(|f| std::mem::transmute::<unsafe extern "system" fn() -> isize, Check>(f))
        });
        CHECK.is_some_and(|check| unsafe { check() != 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    const SECOND: Rect = Rect {
        left: 1920,
        top: 0,
        right: 4480,
        bottom: 1440,
    };

    fn seen(game_in_front: bool, game_above: bool, exclusive: bool) -> Seen {
        Seen {
            monitor: SCREEN,
            overlay: SCREEN,
            game_in_front,
            game_above,
            exclusive,
        }
    }

    #[test]
    fn puts_the_overlay_back_above_a_fullscreen_game() {
        let p = plan(&seen(true, true, false), false);
        assert_eq!(
            p,
            Plan {
                fit: None,
                raise: true,
                exclusive: false
            }
        );
        // Already on top, or the game isn't in front: nothing to do.
        assert!(!plan(&seen(true, false, false), false).raise);
        assert!(!plan(&seen(false, false, false), false).raise);
    }

    #[test]
    fn stays_out_of_the_way_of_exclusive_fullscreen() {
        let mut s = seen(true, true, true);
        s.overlay = SECOND;
        let p = plan(&s, false);
        assert!(p.exclusive);
        assert!(!p.raise);
        assert_eq!(p.fit, None);
        // Exclusive ownership by something other than the game in front doesn't count.
        assert!(!plan(&seen(false, false, true), false).exclusive);
    }

    #[test]
    fn follows_the_game_to_its_monitor_and_resolution() {
        let mut s = seen(true, false, false);
        s.monitor = SECOND;
        assert_eq!(plan(&s, false).fit, Some(SECOND));
        // Not while the wheel is open.
        assert_eq!(plan(&s, true).fit, None);
        assert_eq!((SECOND.width(), SECOND.height()), (2560, 1440));
    }
}
