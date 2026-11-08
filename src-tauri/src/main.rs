//! Summs Wheel: a GTA-style radial wheel overlay for tracking enemy summoner
//! spells in League of Legends.
//!
//! The Rust side owns everything the webview can't do on its own: the global
//! hotkey, the click-through overlay window and cursor, the tray icon, the
//! settings window, and fetching Riot data. The wheel and timers live in the frontend (`src/`).

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod hotkey;
mod overlay;
mod riot;
mod settings;
mod tray;

fn main() {
    tauri::Builder::default()
        .plugin(hotkey::plugin())
        .manage(hotkey::Hotkeys::default())
        .manage(riot::live_client::LiveState::default())
        .manage(overlay::SavedCursor::default())
        .invoke_handler(tauri::generate_handler![
            riot::ddragon::get_spell_data,
            riot::live_client::get_game_state,
            overlay::set_wheel,
            settings::get_hotkey,
            settings::set_hotkey
        ])
        .setup(|app| {
            let cfg = config::load(app.handle());
            let hotkey_label = hotkey::init(app.handle(), &cfg.hotkey);
            tray::build(app, &hotkey_label)?;
            overlay::setup(app)?;
            riot::live_client::spawn_poller(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Summs Wheel");
}
