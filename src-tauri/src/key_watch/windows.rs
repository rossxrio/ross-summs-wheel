//! The Windows input sources: a low-level keyboard hook, Raw Input and
//! key-state polling, all on one thread with its own message loop.

use super::{log, report, snapshot, Key, Source};
use std::{
    cell::Cell,
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
};
use tauri_plugin_global_shortcut::{Code, Modifiers};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM},
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::{
        Input::KeyboardAndMouse::*,
        Input::{
            GetRawInputData, GetRegisteredRawInputDevices, RegisterRawInputDevices, HRAWINPUT, RAWINPUT,
            RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
        },
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
            GetWindowThreadProcessId, RegisterClassExW, SetTimer, SetWindowPos, SetWindowsHookExW, UnhookWindowsHookEx,
            HC_ACTION, HHOOK, HWND_MESSAGE, HWND_TOPMOST, KBDLLHOOKSTRUCT, MSG, RI_KEY_BREAK, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNORMAL, WH_KEYBOARD_LL, WM_INPUT, WM_KEYDOWN, WM_SYSKEYDOWN, WM_TIMER,
            WNDCLASSEXW,
        },
    },
};

/// Key-state polling. Windows timers tick about every 15ms whatever is asked.
const POLL_TIMER: usize = 1;
const POLL_MS: u32 = 10;
/// Re-registers Raw Input and, if needed, the hook (see `upkeep`).
const UPKEEP_TIMER: usize = 2;
const UPKEEP_MS: u32 = 5000;

/// Set when polling caught a press the hook didn't: the hook is reinstalled
/// at the next upkeep, in case Windows dropped it.
static HOOK_SUSPECT: AtomicBool = AtomicBool::new(false);

thread_local! {
    static HOOK: Cell<HHOOK> = const { Cell::new(null_mut()) };
    /// The previous poll's reading of the wheel key.
    static LAST_POLL: Cell<bool> = const { Cell::new(false) };
}

pub fn start() {
    let spawned = std::thread::Builder::new()
        .name("key-watch".into())
        .spawn(|| unsafe { run() });
    if let Err(err) = spawned {
        log::line(&format!("can't start the key watcher: {err}"));
    }
}

unsafe fn run() {
    log::line(&format!(
        "key watcher starting (app is {})",
        if is_elevated() {
            "running as admin"
        } else {
            "not running as admin"
        }
    ));

    let hinstance = GetModuleHandleW(null());
    let class = wide("SummsWheelKeyWatch");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc);
    // A message-only window: never shown, only receives Raw Input and timers.
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        null(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        null_mut(),
        hinstance,
        null(),
    );
    if hwnd.is_null() {
        log::line("can't create the key watcher window: only the global shortcut is active");
        return;
    }

    log::line(&format!("raw input: {}", ok(register_raw_input(hwnd))));
    log::line(&format!("keyboard hook: {}", ok(install_hook())));
    let polling = SetTimer(hwnd, POLL_TIMER, POLL_MS, None) != 0;
    log::line(&format!("key polling: {}", ok(polling)));
    SetTimer(hwnd, UPKEEP_TIMER, UPKEEP_MS, None);

    let mut msg: MSG = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        DispatchMessageW(&msg);
    }
}

fn ok(success: bool) -> &'static str {
    if success {
        "on"
    } else {
        "FAILED"
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => raw_input(lparam as HRAWINPUT),
        WM_TIMER if wparam == POLL_TIMER => poll(),
        WM_TIMER if wparam == UPKEEP_TIMER => upkeep(hwnd),
        _ => {}
    }
    // WM_INPUT must reach DefWindowProc too, so Windows can clean up.
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// ---- Source 1: low-level keyboard hook --------------------------------------

unsafe fn install_hook() -> bool {
    let old = HOOK.get();
    if !old.is_null() {
        UnhookWindowsHookEx(old);
    }
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
    HOOK.set(hook);
    !hook.is_null()
}

/// Runs on the watcher thread for every key press in the system. Must stay
/// fast: Windows silently drops hooks that take too long.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        let pressed = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if on_key(kb.vkCode, pressed, Source::Hook) == Action::Swallow {
            // A quick-bind key while the wheel is held: keep it from the game,
            // as the global shortcut does when it works.
            return 1;
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

// ---- Source 2: Raw Input ----------------------------------------------------

/// Tao (Tauri's windowing) registers the keyboard for its own device events,
/// which Tauri never uses; only one window per process gets keyboard Raw
/// Input, so this takes it over. Upkeep repeats it in case tao takes it back.
unsafe fn register_raw_input(hwnd: HWND) -> bool {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01, // generic desktop
        usUsage: 0x06,     // keyboard
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0
}

/// Whether keyboard Raw Input still comes to this window, with INPUTSINK.
unsafe fn owns_raw_keyboard(hwnd: HWND) -> bool {
    let mut devices: [RAWINPUTDEVICE; 8] = zeroed();
    let mut count = devices.len() as u32;
    let found = GetRegisteredRawInputDevices(devices.as_mut_ptr(), &mut count, size_of::<RAWINPUTDEVICE>() as u32);
    if found == u32::MAX {
        return false;
    }
    devices[..found as usize]
        .iter()
        .any(|d| d.usUsagePage == 0x01 && d.usUsage == 0x06 && d.hwndTarget == hwnd && d.dwFlags & RIDEV_INPUTSINK != 0)
}

unsafe fn raw_input(handle: HRAWINPUT) {
    let mut data: RAWINPUT = zeroed();
    let mut size = size_of::<RAWINPUT>() as u32;
    let read = GetRawInputData(
        handle,
        RID_INPUT,
        &mut data as *mut _ as *mut c_void,
        &mut size,
        size_of::<RAWINPUTHEADER>() as u32,
    );
    if read == u32::MAX || data.header.dwType != RIM_TYPEKEYBOARD {
        return;
    }
    let kb = data.data.keyboard;
    let pressed = u32::from(kb.Flags) & RI_KEY_BREAK == 0;
    // Key reports from Raw Input can't be blocked, so the action is ignored.
    on_key(u32::from(kb.VKey), pressed, Source::RawInput);
}

// ---- Source 3: polling ------------------------------------------------------

unsafe fn poll() {
    let (target, was_down) = snapshot();
    let Some(wheel) = target.wheel else { return };
    let Some(vk) = vk_of(wheel.key) else { return };

    // Modifiers matter for starting a hold, not for keeping it. A release
    // every event source missed shows up here once they've gone quiet, so a
    // stuck "down" (which would keep quick-bind keys from the game) can't last.
    let down = is_down(vk) && (was_down || mods_match(wheel.mods));
    // Two readings in a row: the key state can update a moment before the
    // hook's report arrives, and the event sources should get there first.
    if LAST_POLL.replace(down) != down {
        return;
    }
    if let Some(edge) = report(Key::Wheel, down, Source::Poll) {
        if edge.pressed {
            // Every event source missed this press.
            HOOK_SUSPECT.store(true, Ordering::Relaxed);
            log::line(&format!(
                "events missed that press; foreground is {}",
                foreground_info()
            ));
        }
    }
}

unsafe fn upkeep(hwnd: HWND) {
    if !owns_raw_keyboard(hwnd) {
        log::line(&format!(
            "raw input was taken over; registering again: {}",
            ok(register_raw_input(hwnd))
        ));
    }
    if HOOK_SUSPECT.swap(false, Ordering::Relaxed) {
        log::line(&format!("reinstalling keyboard hook: {}", ok(install_hook())));
    }
}

// ---- Shared -----------------------------------------------------------------

#[derive(PartialEq)]
enum Action {
    Pass,
    Swallow,
}

/// Routes a key event from the hook or Raw Input to the tracker.
fn on_key(vk: u32, pressed: bool, source: Source) -> Action {
    let (target, wheel_down) = snapshot();
    if let Some(wheel) = target.wheel {
        if vk_of(wheel.key) == Some(vk) {
            // A press only counts with the hotkey's exact modifiers, like RegisterHotKey.
            if !pressed || wheel_down || mods_match(wheel.mods) {
                report(Key::Wheel, pressed, source);
            }
            return Action::Pass;
        }
    }
    match target.armed.iter().find(|code| vk_of(**code) == Some(vk)) {
        Some(code) => {
            report(Key::Quick(*code), pressed, source);
            Action::Swallow
        }
        None => Action::Pass,
    }
}

fn is_down(vk: u32) -> bool {
    // High bit: physically down right now.
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn mods_match(mods: Modifiers) -> bool {
    let win = is_down(u32::from(VK_LWIN)) || is_down(u32::from(VK_RWIN));
    mods.contains(Modifiers::SHIFT) == is_down(u32::from(VK_SHIFT))
        && mods.contains(Modifiers::CONTROL) == is_down(u32::from(VK_CONTROL))
        && mods.contains(Modifiers::ALT) == is_down(u32::from(VK_MENU))
        && mods.intersects(Modifiers::SUPER | Modifiers::META) == win
}

/// The virtual key for a key code. Mirrors the table global-hotkey uses for
/// RegisterHotKey, so every source agrees on what the hotkey is.
pub fn vk_of(code: Code) -> Option<u32> {
    let vk = match code {
        Code::KeyA => VK_A,
        Code::KeyB => VK_B,
        Code::KeyC => VK_C,
        Code::KeyD => VK_D,
        Code::KeyE => VK_E,
        Code::KeyF => VK_F,
        Code::KeyG => VK_G,
        Code::KeyH => VK_H,
        Code::KeyI => VK_I,
        Code::KeyJ => VK_J,
        Code::KeyK => VK_K,
        Code::KeyL => VK_L,
        Code::KeyM => VK_M,
        Code::KeyN => VK_N,
        Code::KeyO => VK_O,
        Code::KeyP => VK_P,
        Code::KeyQ => VK_Q,
        Code::KeyR => VK_R,
        Code::KeyS => VK_S,
        Code::KeyT => VK_T,
        Code::KeyU => VK_U,
        Code::KeyV => VK_V,
        Code::KeyW => VK_W,
        Code::KeyX => VK_X,
        Code::KeyY => VK_Y,
        Code::KeyZ => VK_Z,
        Code::Digit0 => VK_0,
        Code::Digit1 => VK_1,
        Code::Digit2 => VK_2,
        Code::Digit3 => VK_3,
        Code::Digit4 => VK_4,
        Code::Digit5 => VK_5,
        Code::Digit6 => VK_6,
        Code::Digit7 => VK_7,
        Code::Digit8 => VK_8,
        Code::Digit9 => VK_9,
        Code::Equal => VK_OEM_PLUS,
        Code::Comma => VK_OEM_COMMA,
        Code::Minus => VK_OEM_MINUS,
        Code::Period => VK_OEM_PERIOD,
        Code::Semicolon => VK_OEM_1,
        Code::Slash => VK_OEM_2,
        Code::Backquote => VK_OEM_3,
        Code::BracketLeft => VK_OEM_4,
        Code::Backslash => VK_OEM_5,
        Code::BracketRight => VK_OEM_6,
        Code::Quote => VK_OEM_7,
        Code::Backspace => VK_BACK,
        Code::Tab => VK_TAB,
        Code::Space => VK_SPACE,
        Code::Enter | Code::NumpadEnter => VK_RETURN,
        Code::CapsLock => VK_CAPITAL,
        Code::Escape => VK_ESCAPE,
        Code::PageUp => VK_PRIOR,
        Code::PageDown => VK_NEXT,
        Code::End => VK_END,
        Code::Home => VK_HOME,
        Code::ArrowLeft => VK_LEFT,
        Code::ArrowUp => VK_UP,
        Code::ArrowRight => VK_RIGHT,
        Code::ArrowDown => VK_DOWN,
        Code::PrintScreen => VK_SNAPSHOT,
        Code::Insert => VK_INSERT,
        Code::Delete => VK_DELETE,
        Code::F1 => VK_F1,
        Code::F2 => VK_F2,
        Code::F3 => VK_F3,
        Code::F4 => VK_F4,
        Code::F5 => VK_F5,
        Code::F6 => VK_F6,
        Code::F7 => VK_F7,
        Code::F8 => VK_F8,
        Code::F9 => VK_F9,
        Code::F10 => VK_F10,
        Code::F11 => VK_F11,
        Code::F12 => VK_F12,
        Code::F13 => VK_F13,
        Code::F14 => VK_F14,
        Code::F15 => VK_F15,
        Code::F16 => VK_F16,
        Code::F17 => VK_F17,
        Code::F18 => VK_F18,
        Code::F19 => VK_F19,
        Code::F20 => VK_F20,
        Code::F21 => VK_F21,
        Code::F22 => VK_F22,
        Code::F23 => VK_F23,
        Code::F24 => VK_F24,
        Code::NumLock => VK_NUMLOCK,
        Code::Numpad0 => VK_NUMPAD0,
        Code::Numpad1 => VK_NUMPAD1,
        Code::Numpad2 => VK_NUMPAD2,
        Code::Numpad3 => VK_NUMPAD3,
        Code::Numpad4 => VK_NUMPAD4,
        Code::Numpad5 => VK_NUMPAD5,
        Code::Numpad6 => VK_NUMPAD6,
        Code::Numpad7 => VK_NUMPAD7,
        Code::Numpad8 => VK_NUMPAD8,
        Code::Numpad9 => VK_NUMPAD9,
        Code::NumpadAdd => VK_ADD,
        Code::NumpadDecimal => VK_DECIMAL,
        Code::NumpadDivide => VK_DIVIDE,
        Code::NumpadMultiply => VK_MULTIPLY,
        Code::NumpadSubtract => VK_SUBTRACT,
        Code::ScrollLock => VK_SCROLL,
        Code::Pause => VK_PAUSE,
        _ => return None,
    };
    Some(u32::from(vk))
}

// ---- Admin rights and the foreground window ---------------------------------

pub fn is_elevated() -> bool {
    unsafe { token_elevated(GetCurrentProcess()) == Some(true) }
}

/// `None` when the token can't be read, which for another process usually
/// means it runs as admin or is protected (anti-cheat).
unsafe fn token_elevated(process: HANDLE) -> Option<bool> {
    let mut token: HANDLE = null_mut();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
        return None;
    }
    let mut elevation: TOKEN_ELEVATION = zeroed();
    let mut len = 0;
    let read = GetTokenInformation(
        token,
        TokenElevation,
        &mut elevation as *mut _ as *mut c_void,
        size_of::<TOKEN_ELEVATION>() as u32,
        &mut len,
    );
    CloseHandle(token);
    (read != 0).then_some(elevation.TokenIsElevated != 0)
}

/// For the log: which program had focus and whether it runs as admin.
unsafe fn foreground_info() -> String {
    let mut pid = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return format!("pid {pid} (can't open it: protected or admin)");
    }
    let mut name = [0u16; 512];
    let mut len = name.len() as u32;
    let exe = if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, name.as_mut_ptr(), &mut len) != 0 {
        String::from_utf16_lossy(&name[..len as usize])
    } else {
        format!("pid {pid}")
    };
    let admin = match token_elevated(process) {
        Some(true) => "admin",
        Some(false) => "not admin",
        None => "admin or protected",
    };
    CloseHandle(process);
    format!("{exe} ({admin})")
}

/// Starts a second copy of this exe through the UAC prompt. The caller exits
/// this copy on success.
pub fn restart_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb = wide("runas");
    let result = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), exe.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    // Values up to 32 are errors, including the user saying no to UAC.
    if result as usize > 32 {
        Ok(())
    } else {
        Err("admin restart was cancelled or failed".into())
    }
}

/// Puts a window back on top of other always-on-top windows (a borderless
/// game can end up above the overlay).
pub fn bring_to_top(hwnd: *mut c_void) {
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
