//! Audio output: the cpal device stream that drains the frontend's audio ring.
//!
//! The lock-free ring + the dynamic-rate-control (DRC) servo live in [`crate::audio_ring`]; this
//! module is the cpal half — it opens the default output device and starts a stream whose callback
//! pulls samples from the ring's [`crate::audio_ring::Consumer`] (silence on underrun).
//!
//! This is the RustyNES audio path, 2600-adapted: the TIA's two channels mix to a single value the
//! frontend pulls at the CPU cadence, resampled (a future `resampler` stage) to the cpal device
//! rate (commonly 48 kHz). The ring + DRC are console-agnostic; only the source rate + channel
//! count differ.
//!
//! The DRC servo + resampler live in the FRONTEND (never the core's synthesis) — that is what
//! keeps the determinism contract intact (the core emits the same samples regardless of how the
//! frontend paces playback).
//!
//! v0.1.0: the cpal stream is wired and plays silence (the TIA audio is a skeleton). The ring + DRC
//! math (in `audio_ring`) are real and tested.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Mutex;

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};

use crate::audio_ring::channel;
use crate::resampler::{HermiteResampler, drc_ratio};

/// The live cpal output stream + the ring consumer it drains (kept alive for the program's
/// duration). Dropping it stops the stream.
pub struct AudioOutput {
    /// The device output sample rate (the resample target).
    pub sample_rate: u32,
    // The stream must outlive its callback; keep it owned here. `Mutex` only to make `AudioOutput`
    // `Send` for the app struct — the stream itself is never re-locked.
    _stream: Mutex<cpal::Stream>,
}

impl AudioOutput {
    /// Open the default output device and start a stereo f32 stream draining `consumer`. The TIA's
    /// mono sample is fanned out to every channel.
    ///
    /// # Errors
    /// Returns an [`AudioError`] when the host has no default output device, the config query
    /// fails, or the stream cannot be built/started.
    pub fn new() -> Result<(Self, AudioProducer), AudioError> {
        let (producer, consumer) = channel(16384);
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| AudioError::Config(e.to_string()))?;
        // cpal 0.18: `SampleRate` is a `u32` alias; `sample_rate()` returns it directly.
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();

        let err_fn = |e| eprintln!("rusty2600 audio stream error: {e}");
        let mut mono = Vec::new();
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    let chans = channels.max(1);
                    let frames = data.len() / chans;
                    if mono.len() < frames {
                        mono.resize(frames, 0.0);
                    }
                    consumer.pop_or_silence(&mut mono[..frames]);
                    for (frame, &s) in data.chunks_mut(chans).zip(mono.iter()) {
                        for ch in frame.iter_mut() {
                            *ch = s;
                        }
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioError::Build(e.to_string()))?;
        stream
            .play()
            .map_err(|e| AudioError::Build(e.to_string()))?;

        Ok((
            Self {
                sample_rate,
                _stream: Mutex::new(stream),
            },
            AudioProducer {
                sample_rate,
                queue: producer,
                resampler: HermiteResampler::new(),
                resample_buf: Vec::with_capacity(1024),
                latency_samples: (sample_rate as usize * 60) / 1000, // 60ms latency target
            },
        ))
    }
}

/// The detached audio producer (resampler + queue push).
pub struct AudioProducer {
    /// The device output sample rate.
    pub sample_rate: u32,
    queue: crate::audio_ring::Producer,
    resampler: HermiteResampler,
    resample_buf: Vec<f32>,
    latency_samples: usize,
}

impl AudioProducer {
    /// How full the audio ring buffer is (0.0 = empty, 1.0 = full). The
    /// emu-thread run loop paces on this to avoid overrunning the consumer.
    #[must_use]
    pub fn fill_ratio(&self) -> f32 {
        self.queue.fill_ratio()
    }

    /// Push core samples (at ~31.4 kHz) into the resampler and queue.
    // `queue.len()`/`latency_samples` are small audio buffer sizes (far below f64's 52-bit
    // exact-integer range), so the usize -> f64 conversions below never lose precision in practice.
    #[allow(clippy::cast_precision_loss)]
    pub fn push_samples(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        let source_hz = 31400.0;
        let base = source_hz / f64::from(self.sample_rate);
        self.resampler.set_base_ratio(base);

        let fill = self.queue.len() as f64 / (2.0 * self.latency_samples.max(1) as f64);
        self.resampler.set_ratio(drc_ratio(fill) * base);

        self.resample_buf.clear();
        self.resampler.process(samples, &mut self.resample_buf);
        self.queue.push_slice(&self.resample_buf);
    }
}

/// Audio initialization failures.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// No default output device.
    #[error("no default audio output device")]
    NoDevice,
    /// Device config query failed.
    #[error("audio device config error: {0}")]
    Config(String),
    /// Stream build/start failed.
    #[error("audio stream build error: {0}")]
    Build(String),
}
