//! Measures Whisper on recorded command clips: what it heard and how long it took.
//!
//!     cargo run --release --example voice_bench -- <model.bin> <clips dir> [audio_ctx] [threads]
//!
//! The clips dir holds 16 kHz mono 16-bit WAVs plus an `index.tsv` of
//! `id<TAB>speaker<TAB>expected phrase` lines. Prints one JSON object per clip.
//! `audio_ctx` 0 (default) is what the app uses; smaller is faster but much
//! less accurate. `PARTS=1` also prints encoder time per clip.

use std::{env, fs, path::Path, time::Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const PROMPT: &str = "Top, jungle, mid, ADC, support. Flash, ignite, teleport, TP, heal, barrier, exhaust, cleanse, ghost, smite. Jinx, Lee Sin, Thresh, Ahri.";

fn main() {
    let args: Vec<String> = env::args().collect();
    let model = &args[1];
    let dir = Path::new(&args[2]);
    let audio_ctx: i32 = args.get(3).map_or(0, |a| a.parse().unwrap());
    let threads: i32 = args.get(4).map_or(4, |a| a.parse().unwrap());

    eprintln!("{}", whisper_rs::print_system_info());
    let t = Instant::now();
    let ctx = WhisperContext::new_with_params(model, WhisperContextParameters::default()).expect("model");
    let mut state = ctx.create_state().expect("state");
    eprintln!("loaded {model} in {} ms", t.elapsed().as_millis());

    let index = fs::read_to_string(dir.join("index.tsv")).unwrap();
    for line in index.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        let (id, expected) = (cols[0], cols[2]);
        let mut reader = hound::WavReader::open(dir.join(format!("{id}.wav"))).unwrap();
        let mut samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| f32::from(s.unwrap()) / 32768.0)
            .collect();
        // Whisper wants at least 1s; pad like the app does.
        samples.resize(samples.len().max(17_600), 0.0);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_n_threads(threads);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_token_timestamps(true);
        params.set_suppress_nst(true);
        params.set_temperature_inc(0.0);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_initial_prompt(PROMPT);
        if audio_ctx > 0 {
            params.set_audio_ctx(audio_ctx);
        } else if audio_ctx < 0 {
            // Scale to the clip: 50 encoder frames a second, plus headroom.
            let secs = samples.len() as f32 / 16_000.0;
            params.set_audio_ctx(((secs * 50.0) as i32 + 64).min(1500));
        }

        if env::var("PARTS").is_ok() {
            let t = Instant::now();
            state.pcm_to_mel(&samples, threads as usize).unwrap();
            let mel = t.elapsed().as_millis();
            let t = Instant::now();
            state.encode(0, threads as usize).unwrap();
            eprintln!("mel {mel} ms, encode {} ms", t.elapsed().as_millis());
        }
        let t = Instant::now();
        state.full(params, &samples).expect("full");
        let ms = t.elapsed().as_millis();
        let text: String = state
            .as_iter()
            .map(|s| s.to_str_lossy().unwrap().into_owned())
            .collect();
        println!(
            "{{\"id\":\"{id}\",\"expected\":{:?},\"heard\":{:?},\"ms\":{ms}}}",
            expected,
            text.trim()
        );
    }
}
