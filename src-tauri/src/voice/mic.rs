//! The default microphone, captured into a rolling 16 kHz mono buffer.
//!
//! The stream stays open while voice could be used, so a clip can start a
//! moment *before* the talk key went down (see PRE_ROLL in mod.rs).

use super::engine::SAMPLE_RATE;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// How much audio is kept. Whisper reads at most 30 s at a time.
const KEEP: Duration = Duration::from_secs(28);

pub struct Mic {
    _stream: Stream,
    ring: Clip,
    failed: Arc<AtomicBool>,
}

/// A handle on the rolling buffer, to cut a clip from later.
#[derive(Clone)]
pub struct Clip(Arc<Mutex<Ring>>);

/// Audio ready for Whisper: 16 kHz mono, and when its first sample was heard.
pub struct Audio {
    pub samples: Vec<f32>,
    pub start: Instant,
}

struct Ring {
    samples: VecDeque<f32>,
    /// When the newest sample arrived.
    last: Instant,
}

impl Mic {
    pub fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("no microphone found")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        if config.sample_rate < SAMPLE_RATE {
            return Err(format!("{} Hz is too low a sample rate", config.sample_rate));
        }
        let ring = Clip(Arc::new(Mutex::new(Ring {
            samples: VecDeque::with_capacity(KEEP.as_secs() as usize * SAMPLE_RATE as usize),
            last: Instant::now(),
        })));
        let failed = Arc::new(AtomicBool::new(false));

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, &ring, &failed),
            SampleFormat::I16 => build::<i16>(&device, config, &ring, &failed),
            SampleFormat::U16 => build::<u16>(&device, config, &ring, &failed),
            SampleFormat::I32 => build::<i32>(&device, config, &ring, &failed),
            other => return Err(format!("unsupported sample format {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            ring,
            failed,
        })
    }

    /// The device went away (headset unplugged) or errored: open it again.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn clip(&self) -> Clip {
        self.ring.clone()
    }
}

impl Clip {
    /// Everything heard from `since` until now.
    pub fn since(&self, since: Instant) -> Audio {
        let ring = self.0.lock().unwrap();
        let span = ring.last.saturating_duration_since(since);
        let n = ((span.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize).min(ring.samples.len());
        let start = ring.last - Duration::from_secs_f64(n as f64 / f64::from(SAMPLE_RATE));
        Audio {
            samples: ring.samples.range(ring.samples.len() - n..).copied().collect(),
            start,
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    ring: &Clip,
    failed: &Arc<AtomicBool>,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let mut down = Downsampler::new(config.sample_rate);
    let ring = ring.clone();
    let keep = KEEP.as_secs() as usize * SAMPLE_RATE as usize;
    let failed = failed.clone();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let mut ring = ring.0.lock().unwrap();
                for frame in data.chunks(channels) {
                    let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
                    if let Some(out) = down.push(mono) {
                        if ring.samples.len() == keep {
                            ring.samples.pop_front();
                        }
                        ring.samples.push_back(out);
                    }
                }
                ring.last = Instant::now();
            },
            move |err: cpal::Error| {
                // Overruns and reroutes are routine; only a dead stream needs reopening.
                if matches!(
                    err.kind(),
                    ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated | ErrorKind::HostUnavailable
                ) {
                    eprintln!("microphone: {err}");
                    failed.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

/// Averages each run of input samples into one 16 kHz sample. The averaging
/// doubles as the low-pass filter: plenty for speech recognition.
struct Downsampler {
    /// Output samples per input sample (< 1).
    step: f64,
    pos: f64,
    sum: f32,
    count: u32,
}

impl Downsampler {
    fn new(input_rate: u32) -> Self {
        Self {
            step: f64::from(SAMPLE_RATE) / f64::from(input_rate),
            pos: 0.0,
            sum: 0.0,
            count: 0,
        }
    }

    fn push(&mut self, sample: f32) -> Option<f32> {
        self.sum += sample;
        self.count += 1;
        self.pos += self.step;
        if self.pos < 1.0 {
            return None;
        }
        self.pos -= 1.0;
        let out = self.sum / self.count as f32;
        self.sum = 0.0;
        self.count = 0;
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsamples_48k_to_16k() {
        let mut d = Downsampler::new(48_000);
        let out: Vec<f32> = (0..48_000).filter_map(|i| d.push((i % 3) as f32)).collect();
        assert_eq!(out.len(), 16_000);
        // Each output is the mean of three inputs (0, 1, 2).
        assert!(out.iter().all(|s| (s - 1.0).abs() < 1e-6));
    }

    #[test]
    fn downsamples_44k1_to_the_right_length() {
        let mut d = Downsampler::new(44_100);
        let n = (0..44_100).filter_map(|_| d.push(0.5)).count();
        assert!((15_999..=16_000).contains(&n), "{n}");
    }

    #[test]
    fn clip_since_cuts_from_the_given_moment() {
        let now = Instant::now();
        let ring = Clip(Arc::new(Mutex::new(Ring {
            samples: (0..32_000).map(|i| i as f32).collect(),
            last: now,
        })));
        let audio = ring.since(now - Duration::from_millis(500));
        assert_eq!(audio.samples.len(), 8_000);
        assert_eq!(audio.samples[0], 24_000.0);
        // Asking for more than was kept gives everything.
        assert_eq!(ring.since(now - Duration::from_secs(10)).samples.len(), 32_000);
    }
}
