//! The transparent, full-screen overlay window the wheel and HUD draw on, and
//! how it handles the mouse when the wheel opens and closes.
//!
//! The window is `"focusable": false` in tauri.conf.json (WS_EX_NOACTIVATE on
//! Windows), so taking mouse input never steals keyboard focus from the game.
//! Don't set that style by hand: Tauri recomputes the extended styles whenever
//! click-through is toggled and would drop it.

use serde::Deserialize;
use std::sync::Mutex;
use tauri::{App, Manager, PhysicalPosition, State, WebviewWindow};

/// Stretches the overlay over the primary monitor and makes it click-through.
pub fn setup(app: &App) -> tauri::Result<()> {
    let window = app
        .get_webview_window("overlay")
        .expect("overlay window in tauri.conf.json");

    if let Some(monitor) = window.primary_monitor()? {
        window.set_position(*monitor.position())?;
        window.set_size(*monitor.size())?;
    }

    window.set_ignore_cursor_events(true)?;
    window.show()?;
    Ok(())
}

/// Where the cursor was before the wheel opened, in screen coordinates.
#[derive(Default)]
pub struct SavedCursor(Mutex<Option<PhysicalPosition<f64>>>);

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WheelMode {
    /// Take the mouse and move the cursor to the screen center.
    Open,
    /// Move the cursor back to the center (role wheel -> spell wheel).
    Recenter,
    /// Release the mouse and put the cursor back where it was.
    Close,
}

/// The wheel reads mouse *movement*. Starting at the screen center means the
/// cursor can't get stuck on an edge mid-flick (it's often near the minimap
/// in game). Putting it back afterwards leaves the game cursor exactly where
/// the player had it.
#[tauri::command]
pub fn set_wheel(window: WebviewWindow, saved: State<'_, SavedCursor>, mode: WheelMode) -> Result<(), String> {
    let apply = || -> tauri::Result<()> {
        match mode {
            WheelMode::Open => {
                *saved.0.lock().unwrap() = Some(window.cursor_position()?);
                window.set_ignore_cursor_events(false)?;
                center_cursor(&window)?;
            }
            WheelMode::Recenter => center_cursor(&window)?,
            WheelMode::Close => {
                window.set_ignore_cursor_events(true)?;
                if let Some(pos) = saved.0.lock().unwrap().take() {
                    restore_cursor(&window, pos)?;
                }
            }
        }
        Ok(())
    };
    apply().map_err(|e| e.to_string())
}

fn center_cursor(window: &WebviewWindow) -> tauri::Result<()> {
    let size = window.inner_size()?;
    window.set_cursor_position(PhysicalPosition::new(size.width / 2, size.height / 2))
}

/// `cursor_position` is in screen coordinates; `set_cursor_position` is relative to the window.
fn restore_cursor(window: &WebviewWindow, screen_pos: PhysicalPosition<f64>) -> tauri::Result<()> {
    let origin = window.inner_position()?;
    window.set_cursor_position(PhysicalPosition::new(
        screen_pos.x - f64::from(origin.x),
        screen_pos.y - f64::from(origin.y),
    ))
}
