//! The Whisper models on offer, and downloading them to the app data dir
//! (%APPDATA%\com.rossxrio.summswheel\models).
//!
//! Each file is pinned to one revision of ggerganov/whisper.cpp on Hugging
//! Face and checked against its SHA-256 before use.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

pub const DEFAULT: &str = "tiny.en";

const REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

struct Model {
    id: &'static str,
    label: &'static str,
    file: &'static str,
    bytes: u64,
    sha256: &'static str,
}

/// English-only models: better than the multilingual ones of the same size at English.
/// Times are for a 2 s command on a Ryzen 7 7800X3D using 4 threads.
const MODELS: [Model; 3] = [
    Model {
        id: "tiny.en",
        label: "Fast (~0.5 s)",
        file: "ggml-tiny.en.bin",
        bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
    },
    Model {
        id: "base.en",
        label: "Balanced (~1 s)",
        file: "ggml-base.en.bin",
        bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    },
    Model {
        id: "small.en",
        label: "Accurate (~3 s)",
        file: "ggml-small.en.bin",
        bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
    },
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: &'static str,
    label: &'static str,
    size_mb: u64,
    pub downloaded: bool,
}

/// Sent as `voice-download` while a model downloads.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    model: &'a str,
    received: u64,
    total: u64,
}

fn find(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    let model = find(id)?;
    Some(app.path().app_data_dir().ok()?.join("models").join(model.file))
}

/// Present with the right size. (The hash is checked once, when downloading.)
pub fn is_downloaded(app: &AppHandle, id: &str) -> bool {
    let (Some(model), Some(path)) = (find(id), path(app, id)) else {
        return false;
    };
    fs::metadata(path).is_ok_and(|m| m.len() == model.bytes)
}

pub fn list(app: &AppHandle) -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|m| ModelInfo {
            id: m.id,
            label: m.label,
            size_mb: m.bytes.div_ceil(1_000_000),
            downloaded: is_downloaded(app, m.id),
        })
        .collect()
}

/// One download at a time.
static DOWNLOADING: Mutex<bool> = Mutex::new(false);

/// Downloads a model, then puts it into use if it's the selected one.
#[tauri::command]
pub async fn download_voice_model(app: AppHandle, model: String) -> Result<(), String> {
    let spec = find(&model).ok_or(format!("unknown voice model {model}"))?;
    {
        let mut busy = DOWNLOADING.lock().unwrap();
        if *busy {
            return Err("a download is already running".into());
        }
        *busy = true;
    }
    let result = download(&app, spec).await;
    *DOWNLOADING.lock().unwrap() = false;
    super::refresh(&app);
    result
}

async fn download(app: &AppHandle, model: &Model) -> Result<(), String> {
    let path = path(app, model.id).ok_or("no app data directory")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    let part = path.with_extension("part");
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/{REVISION}/{}",
        model.file
    );

    let fail = |e: reqwest::Error| format!("download failed: {e}");
    let mut res = reqwest::get(&url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(fail)?;
    let mut file = fs::File::create(&part).map_err(|e| format!("can't write {}: {e}", part.display()))?;
    let mut hash = Sha256::new();
    let mut received = 0u64;
    let mut last_report = Instant::now();
    while let Some(chunk) = res.chunk().await.map_err(fail)? {
        hash.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("can't write the model: {e}"))?;
        received += chunk.len() as u64;
        if last_report.elapsed() > Duration::from_millis(150) {
            last_report = Instant::now();
            let _ = app.emit(
                "voice-download",
                Progress {
                    model: model.id,
                    received,
                    total: model.bytes,
                },
            );
        }
    }
    drop(file);

    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != model.sha256 {
        let _ = fs::remove_file(&part);
        return Err("the downloaded model is damaged; try again".into());
    }
    fs::rename(&part, &path).map_err(|e| format!("can't save the model: {e}"))?;
    let _ = app.emit(
        "voice-download",
        Progress {
            model: model.id,
            received,
            total: model.bytes,
        },
    );
    Ok(())
}
