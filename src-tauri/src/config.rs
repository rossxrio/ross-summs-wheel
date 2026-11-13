use crate::{quick_binds::QuickBind, voice::VoiceConfig};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

/// The backtick key is not bound in League by default.
pub const DEFAULT_HOTKEY: &str = "Backquote";

/// User settings, stored as `config.json` in the app config dir
/// (%APPDATA%\com.rossxrio.summswheel on Windows).
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Global shortcut, e.g. "Backquote", "Alt+Q", "F13", "Mouse4". Uses W3C
    /// key code names, plus "Mouse4"/"Mouse5" for the mouse side buttons.
    pub hotkey: String,
    /// Key sequences typed while holding the hotkey (see quick_binds.rs).
    pub quick_binds: Vec<QuickBind>,
    /// Push-to-talk voice commands (see voice/).
    pub voice: VoiceConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.into(),
            quick_binds: vec![],
            voice: VoiceConfig::default(),
        }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("config.json"))
}

/// Loads the config, writing the defaults on first run so the user has a file to edit.
pub fn load(app: &AppHandle) -> Config {
    let Some(path) = path(app) else {
        return Config::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!("ignoring invalid {}: {err}", path.display());
            Config::default()
        }),
        Err(_) => {
            let cfg = Config::default();
            if let Err(err) = save(app, &cfg) {
                eprintln!("{err}");
            }
            cfg
        }
    }
}

pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let path = path(app).ok_or("no config directory")?;
    let text = serde_json::to_string_pretty(cfg).unwrap();
    path.parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|_| fs::write(&path, text))
        .map_err(|err| format!("could not write {}: {err}", path.display()))
}
