use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

/// The backtick key is not bound in League by default.
pub const DEFAULT_HOTKEY: &str = "Backquote";

/// User settings, stored as `config.json` in the app config dir
/// (%APPDATA%\com.rossxrio.summswheel on Windows).
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Global shortcut, e.g. "Backquote", "Alt+Q", "F13". Uses W3C key code names.
    pub hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.into(),
        }
    }
}

/// Loads the config, writing the defaults on first run so the user has a file to edit.
pub fn load(app: &AppHandle) -> (Config, Option<PathBuf>) {
    let Ok(dir) = app.path().app_config_dir() else {
        return (Config::default(), None);
    };
    let path = dir.join("config.json");

    let cfg = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!("ignoring invalid {}: {err}", path.display());
            Config::default()
        }),
        Err(_) => {
            let cfg = Config::default();
            let written =
                fs::create_dir_all(&dir).and_then(|_| fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap()));
            if let Err(err) = written {
                eprintln!("could not write {}: {err}", path.display());
            }
            cfg
        }
    };
    (cfg, Some(path))
}
