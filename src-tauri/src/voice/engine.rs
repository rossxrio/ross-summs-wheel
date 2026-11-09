//! The Whisper worker: one thread that loads the model and transcribes clips
//! in order. Dropping the [`Engine`] ends the thread and frees the model.

use super::{mic::Audio, Heard, HeardWord};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

/// Whisper's input rate.
pub const SAMPLE_RATE: u32 = 16_000;
/// Whisper refuses clips under a second; shorter ones are padded with silence.
const MIN_SAMPLES: usize = SAMPLE_RATE as usize * 11 / 10;
/// Loudest 30 ms of a clip below this RMS: nobody spoke. Whisper invents
/// words ("Thank you.") for silence, so it isn't asked.
const SILENCE_RMS: f32 = 0.004;
/// Whisper's own guess that a clip has no speech, above which it's ignored.
const NO_SPEECH: f32 = 0.6;

#[derive(Clone, PartialEq)]
pub enum State {
    Loading,
    Ready,
    Failed(String),
}

pub struct Job {
    pub clip: Audio,
    pub prompt: String,
    pub test: bool,
}

pub struct Engine {
    pub model: String,
    jobs: Sender<Job>,
    state: Arc<Mutex<State>>,
}

impl Engine {
    pub fn start(app: AppHandle, model: String, path: PathBuf) -> Self {
        let (jobs, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(State::Loading));
        let worker_state = state.clone();
        let spawned = thread::Builder::new()
            .name("whisper".into())
            .spawn(move || run(&app, &path, rx, &worker_state));
        if let Err(err) = spawned {
            *state.lock().unwrap() = State::Failed(format!("can't start the voice worker: {err}"));
        }
        Self { model, jobs, state }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub fn jobs(&self) -> Sender<Job> {
        self.jobs.clone()
    }
}

fn run(app: &AppHandle, path: &PathBuf, jobs: Receiver<Job>, state: &Mutex<State>) {
    let set = |next: State| {
        *state.lock().unwrap() = next;
        super::emit_status(app);
    };
    // whisper.cpp logs every load step to stderr; keep it quiet.
    whisper_rs::install_logging_hooks();
    let loaded = WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .and_then(|ctx| ctx.create_state().map(|s| (ctx, s)));
    let (ctx, mut whisper) = match loaded {
        Ok(pair) => pair,
        Err(err) => {
            return set(State::Failed(format!(
                "can't load the voice model ({err}); try downloading it again"
            )))
        }
    };
    set(State::Ready);

    // Leave most of the CPU to the game.
    let threads = thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4)) as i32;
    for job in jobs {
        let heard = transcribe(&ctx, &mut whisper, job, threads);
        let _ = app.emit("voice-heard", heard);
    }
}

fn transcribe(ctx: &WhisperContext, whisper: &mut WhisperState, job: Job, threads: i32) -> Heard {
    let mut heard = Heard {
        text: String::new(),
        words: vec![],
        decode_ms: 0,
        test: job.test,
    };
    let Audio { mut samples, start } = job.clip;
    if loudest_rms(&samples) < SILENCE_RMS {
        return heard;
    }
    samples.resize(samples.len().max(MIN_SAMPLES), 0.0);

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_n_threads(threads);
    params.set_initial_prompt(&job.prompt);
    // One short utterance: no carry-over between clips, one segment.
    params.set_no_context(true);
    params.set_single_segment(true);
    // Per-token times say *when* each word was said within a long hold.
    params.set_token_timestamps(true);
    params.set_suppress_nst(true);
    // No retries at higher temperature: a miss is better than a slow guess.
    params.set_temperature_inc(0.0);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);

    let began = Instant::now();
    if let Err(err) = whisper.full(params, &samples) {
        eprintln!("whisper: {err}");
        return heard;
    }
    heard.decode_ms = began.elapsed().as_millis() as u64;

    let eot = ctx.token_eot();
    // (word, offset into the clip)
    let mut words: Vec<(String, Duration)> = vec![];
    for segment in whisper.as_iter() {
        if segment.no_speech_probability() > NO_SPEECH {
            continue;
        }
        heard.text.push_str(&segment.to_str_lossy().unwrap_or_default());
        for i in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(i) else { continue };
            // Ids from end-of-text up are control and timestamp tokens.
            if token.token_id() >= eot {
                continue;
            }
            let piece = token.to_str_lossy().unwrap_or_default();
            // t0 is in centiseconds from the clip start.
            let at = Duration::from_millis(token.token_data().t0.max(0) as u64 * 10);
            match words.last_mut() {
                Some((word, _)) if !piece.starts_with(' ') => word.push_str(&piece),
                _ => words.push((piece.trim().to_string(), at)),
            }
        }
    }
    heard.text = heard.text.trim().to_string();

    let now = Instant::now();
    heard.words = words
        .into_iter()
        .filter(|(text, _)| !text.is_empty())
        .map(|(text, offset)| HeardWord {
            text,
            ago_ms: now.saturating_duration_since(start + offset).as_millis() as u64,
        })
        .collect();
    heard
}

/// RMS of the loudest 30 ms window.
fn loudest_rms(samples: &[f32]) -> f32 {
    samples
        .chunks(SAMPLE_RATE as usize * 30 / 1000)
        .map(|w| (w.iter().map(|s| s * s).sum::<f32>() / w.len() as f32).sqrt())
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_quiet_and_speech_isnt() {
        assert!(loudest_rms(&vec![0.0; 16_000]) < SILENCE_RMS);
        let mut clip = vec![0.001; 16_000];
        // A 50 ms burst at a normal speaking level.
        for (i, s) in clip[8_000..8_800].iter_mut().enumerate() {
            *s = 0.1 * (i as f32 * 0.3).sin();
        }
        assert!(loudest_rms(&clip) > SILENCE_RMS);
    }
}
