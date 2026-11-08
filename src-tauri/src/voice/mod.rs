//! Push-to-talk voice commands: hold the talk key, say "ADC no flash", let go.
//!
//! Rust records and transcribes (Whisper, on the CPU); the overlay turns the
//! words into timers (src/voice/parse.ts), since it knows the enemy roster.
//! Each heard word carries how long ago it was said, so a timer counts from
//! the moment the spell was called, not from when Whisper finished.
//!
//! - mic.rs: the microphone, kept open while it could be needed (in game, or
//!   settings open for the test button) so the start of a word isn't lost.
//! - engine.rs: a worker thread holding the loaded model.
//! - models.rs: which models exist, and downloading them.

mod engine;
mod mic;
pub mod models;

use crate::{overlay, riot::live_client};
use serde::{Deserialize, Serialize};
use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

/// Taps shorter than this are ignored (a quick Discord push-to-talk tap).
const MIN_HOLD: Duration = Duration::from_millis(250);
/// Audio kept from before the key went down: people start talking as they press.
const PRE_ROLL: Duration = Duration::from_millis(300);
/// ...and after it came up: the last word often trails the release.
const TAIL: Duration = Duration::from_millis(200);

/// Voice settings, stored in config.json.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceConfig {
    pub enabled: bool,
    /// W3C key code of the talk key, e.g. "KeyV" or "CapsLock", or "Mouse4"/"Mouse5".
    /// Empty when not set.
    /// Modifiers don't matter, and the key still reaches the game.
    pub key: String,
    /// Model id from `models::MODELS`, e.g. "tiny.en".
    pub model: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key: String::new(),
            model: models::DEFAULT.into(),
        }
    }
}

#[derive(Default)]
pub struct Voice(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    config: VoiceConfig,
    engine: Option<engine::Engine>,
    mic: Option<mic::Mic>,
    /// Set while the talk key (or the settings test button) is held.
    talk: Option<Talk>,
    in_game: bool,
    settings_open: bool,
    /// Why the mic couldn't open, for the settings page.
    mic_error: Option<String>,
}

#[derive(Clone, Copy)]
struct Talk {
    since: Instant,
    /// From the settings test button: the overlay ignores the result.
    test: bool,
}

/// What the settings page shows. Sent as `voice-status` whenever it changes.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    /// "off", "needsModel", "loading", "ready" or "error".
    state: &'static str,
    message: Option<String>,
    models: Vec<models::ModelInfo>,
}

/// One clip's transcript. Sent as `voice-heard`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    text: String,
    words: Vec<HeardWord>,
    /// How long Whisper took.
    decode_ms: u64,
    test: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardWord {
    text: String,
    /// How long before this event the word was said.
    ago_ms: u64,
}

fn with_inner<T>(app: &AppHandle, f: impl FnOnce(&mut Inner) -> T) -> T {
    let state = app.state::<Voice>();
    let mut inner = state.0.lock().unwrap();
    f(&mut inner)
}

/// Puts new voice settings into use.
pub fn apply(app: &AppHandle, config: &VoiceConfig) {
    with_inner(app, |inner| inner.config = config.clone());
    refresh(app);
}

/// The mic is only needed in game, or for the settings test button.
pub fn set_in_game(app: &AppHandle, in_game: bool) {
    if with_inner(app, |inner| std::mem::replace(&mut inner.in_game, in_game)) != in_game {
        refresh(app);
    }
}

pub fn set_settings_open(app: &AppHandle, open: bool) {
    let was_talking = with_inner(app, |inner| {
        inner.settings_open = open;
        // The talk key isn't watched while settings is open: don't wait for its release.
        inner.talk.take().is_some()
    });
    if was_talking {
        let _ = app.emit("voice-listening", false);
    }
    refresh(app);
}

/// Loads or drops the model and opens or closes the mic to match the settings.
fn refresh(app: &AppHandle) {
    with_inner(app, |inner| {
        let cfg = &inner.config;
        let model_path = models::path(app, &cfg.model).filter(|_| models::is_downloaded(app, &cfg.model));
        match model_path {
            Some(path) if cfg.enabled => {
                if inner.engine.as_ref().is_none_or(|e| e.model != cfg.model) {
                    inner.engine = Some(engine::Engine::start(app.clone(), cfg.model.clone(), path));
                }
            }
            // Dropping the engine ends its thread and frees the model's memory.
            _ => inner.engine = None,
        }

        let want_mic = inner.engine.is_some() && (inner.in_game || inner.settings_open);
        if !want_mic {
            inner.mic = None;
            inner.mic_error = None;
        } else if inner.mic.as_ref().is_none_or(mic::Mic::failed) {
            inner.mic = None;
            match mic::Mic::open() {
                Ok(m) => {
                    inner.mic = Some(m);
                    inner.mic_error = None;
                }
                Err(err) => inner.mic_error = Some(err),
            }
        }
    });
    emit_status(app);
}

pub fn status(app: &AppHandle) -> VoiceStatus {
    let models = models::list(app);
    with_inner(app, |inner| {
        let cfg = &inner.config;
        let downloaded = models.iter().any(|m| m.id == cfg.model && m.downloaded);
        let (state, message) = if !cfg.enabled {
            ("off", None)
        } else if !downloaded {
            (
                "needsModel",
                Some("Download the voice model to use voice commands".to_string()),
            )
        } else if let Some(err) = &inner.mic_error {
            ("error", Some(format!("Microphone: {err}")))
        } else {
            match inner.engine.as_ref().map(engine::Engine::state) {
                Some(engine::State::Ready) => ("ready", None),
                Some(engine::State::Failed(err)) => ("error", Some(err)),
                Some(engine::State::Loading) | None => ("loading", Some("Loading the voice model…".to_string())),
            }
        };
        VoiceStatus { state, message, models }
    })
}

pub fn emit_status(app: &AppHandle) {
    let _ = app.emit("voice-status", status(app));
}

/// The talk key (or the settings test button) went down or up at `at`.
pub fn talk(app: &AppHandle, pressed: bool, at: Instant, test: bool) {
    if pressed {
        // Hidden from settings: the overlay can't show what it heard.
        if !test && !overlay::is_shown(app) {
            return;
        }
        let ready = with_inner(app, |inner| {
            let ready = inner.talk.is_none()
                && inner.mic.as_ref().is_some_and(|m| !m.failed())
                && inner.engine.as_ref().is_some_and(|e| e.state() == engine::State::Ready);
            if ready {
                inner.talk = Some(Talk { since: at, test });
            }
            ready
        });
        if ready {
            let _ = app.emit("voice-listening", true);
        } else {
            // A mic that dropped out (headset unplugged) is reopened for next time.
            refresh(app);
        }
        return;
    }

    let Some(talk) = with_inner(app, |inner| inner.talk.take()) else {
        return;
    };
    let _ = app.emit("voice-listening", false);
    if at.saturating_duration_since(talk.since) < MIN_HOLD {
        return;
    }
    let Some((clip, jobs)) = with_inner(app, |inner| {
        Some((inner.mic.as_ref()?.clip(), inner.engine.as_ref()?.jobs()))
    }) else {
        return;
    };

    let prompt = prompt(&live_client::enemy_champions(app));
    thread::spawn(move || {
        thread::sleep(TAIL);
        let clip = clip.since(talk.since.checked_sub(PRE_ROLL).unwrap_or(talk.since));
        let _ = jobs.send(engine::Job {
            clip,
            prompt,
            test: talk.test,
        });
    });
}

/// Whisper copies the style and spelling of its prompt, so listing the words
/// we're after makes "ADC" come out as "ADC" rather than "a DC".
fn prompt(champions: &[String]) -> String {
    let mut prompt = String::from(
        "Top, jungle, mid, ADC, support. Flash, ignite, teleport, TP, heal, barrier, exhaust, cleanse, ghost, smite.",
    );
    if !champions.is_empty() {
        prompt.push(' ');
        prompt.push_str(&champions.join(", "));
        prompt.push('.');
    }
    prompt
}

#[tauri::command]
pub fn get_voice_status(app: AppHandle) -> VoiceStatus {
    status(&app)
}

/// The settings page's "hold to test" button.
#[tauri::command]
pub fn voice_test(app: AppHandle, pressed: bool) {
    talk(&app, pressed, Instant::now(), true);
}
