//! Checks the default microphone opens the way the app opens it, and prints
//! its format and loudness for two seconds. Nothing is saved.
//!
//!     cargo run --release --example mic_check

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn main() {
    let device = cpal::default_host().default_input_device().expect("no microphone");
    let supported = device.default_input_config().expect("no input config");
    println!(
        "{:?}: {} Hz, {} ch, {:?}",
        device.description().map(|d| d.to_string()).unwrap_or_default(),
        supported.sample_rate(),
        supported.channels(),
        supported.sample_format()
    );
    let peak = Arc::new(Mutex::new((0usize, 0f32)));
    let p = peak.clone();
    let stream = device
        .build_input_stream::<f32, _, _>(
            supported.config(),
            move |data: &[f32], _| {
                let mut p = p.lock().unwrap();
                p.0 += data.len();
                p.1 = data.iter().fold(p.1, |m, s| m.max(s.abs()));
            },
            |e| eprintln!("stream error: {e}"),
            None,
        )
        .expect("build stream (format may not be f32)");
    stream.play().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let (n, max) = *peak.lock().unwrap();
    println!("{n} samples in 2 s, peak level {max:.4}");
}
