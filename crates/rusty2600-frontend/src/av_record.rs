//! `[v2.13.0]` — A/V (video + synchronized audio) recording, adapted from the
//! sibling RustyNES project's own `av_record.rs` (`v1.6.0` "Studio" Workstream
//! G) to Rusty2600's own frame/audio types.
//!
//! A **read-only frontend tap** on the already-produced output: each produced
//! frame's RGBA8 framebuffer (the same slice [`crate::emu_thread::EmuCore::framebuffer`]
//! exposes to the present path — variable-sized per [`crate::emu_thread::EmuCore::fb_dims`],
//! since NTSC/PAL/SECAM have different active heights, unlike a fixed-resolution
//! console) plus the audio samples produced for that same frame (mono `f32`,
//! DC-blocked and normalized, the exact slice pushed to [`crate::audio_ring`])
//! are buffered to disk and, at [`AvRecorder::stop`], muxed by an external
//! `ffmpeg` process into an `.mp4`/`.mkv` container.
//!
//! ## Why this does NOT touch determinism
//!
//! The recorder NEVER advances the emulator, mutates the core, or alters the
//! per-frame framebuffer / audio production. It only *copies* what
//! [`crate::emu_thread::EmuCore::run_frame`]/`extract_frame` has already
//! produced — the same data the renderer presents and the audio device
//! consumes. The determinism contract (same seed + ROM + input => bit-identical
//! framebuffer + audio) is unaffected, and with the `av-record` feature off
//! (the default) this module is not compiled at all — the shipped / wasm /
//! `no_std` builds are byte-identical.
//!
//! ## Encoder approach — mux at stop from two COMPLETE files
//!
//! Spawning `ffmpeg` at arm time with a still-empty audio sidecar passed as a
//! regular-file `-i` input is broken: `ffmpeg` opens a regular-file input and
//! reads it to EOF eagerly at startup, so it would see an empty (or
//! truncated) audio file before any samples had been written. The robust,
//! dependency-free fix is to make **both** inputs complete before `ffmpeg`
//! ever runs:
//!
//! * during recording, append **rawvideo** (`rgba`, at this session's fixed
//!   `(width, height)`) to a video temp file and the corresponding **mono
//!   `f32le`** PCM to an audio temp file — no child process is alive, so
//!   there is no two-pipe deadlock and no read-before-write race; and
//! * at [`AvRecorder::stop`], flush both files and spawn `ffmpeg` **once**
//!   with `-i video.raw -i audio.raw`, muxing the two fully-written inputs
//!   into the output container, then delete both temps.
//!
//! `ffmpeg` availability is probed at [`AvRecorder::start`] (a cheap
//! `ffmpeg -version` spawn) so arming fails fast and gracefully with
//! [`AvError::FfmpegMissing`] when `ffmpeg` is absent — emulation continues
//! untouched and the recorder is never armed.
//!
//! Choosing an external `ffmpeg` (over a vendored pure-Rust encoder) keeps the
//! default build free of heavy media codecs; the feature is additive,
//! off-by-default, and native-only.
//!
//! ## Scope narrower than RustyNES's own `av_record.rs` (a deliberate v1 cut)
//!
//! This first cut ships MP4 (H.264 + AAC) only, at one fixed, sane quality
//! preset (CRF 18, `veryfast`, 192 kbit/s AAC) — no in-Settings codec/CRF/
//! preset picker, and no GIF/WAV export variants (RustyNES's own `v2.1.9`
//! additions on top of its original `v1.6.0` landing). A `.mkv` output still
//! works (same H.264/AAC codecs, a freer container), inferred from the chosen
//! file extension. Full parameterization is a natural, explicitly deferred
//! follow-up if it turns out to matter in practice.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Errors raised while arming / driving the recorder.
#[derive(Debug)]
#[non_exhaustive]
pub enum AvError {
    /// `ffmpeg` is not on `PATH` (or failed to spawn). Recording is unavailable;
    /// emulation is unaffected.
    FfmpegMissing(io::Error),
    /// A temp capture file (video or audio) could not be created / written.
    Sidecar(io::Error),
    /// `ffmpeg` exited non-zero during the final mux, or it could not be spawned
    /// at stop time.
    Encode(String),
}

impl core::fmt::Display for AvError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FfmpegMissing(e) => {
                write!(f, "ffmpeg not found (A/V recording unavailable): {e}")
            }
            Self::Sidecar(e) => write!(f, "A/V capture temp I/O failed: {e}"),
            Self::Encode(e) => write!(f, "ffmpeg encode failed: {e}"),
        }
    }
}

impl std::error::Error for AvError {}

/// The output container, inferred from the chosen file extension. Anything
/// unrecognized (including no extension) defaults to [`Container::Mp4`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Container {
    /// `.mp4` (H.264 video + AAC audio). The default.
    #[default]
    Mp4,
    /// `.mkv` (Matroska; the same codecs in a freer container).
    Mkv,
}

impl Container {
    /// Infer the container from a path's extension (case-insensitive).
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("mkv") => Self::Mkv,
            _ => Self::Mp4,
        }
    }
}

/// Parameters fixed when a recording is armed (constant for its lifetime).
#[derive(Debug, Clone)]
pub struct AvParams {
    /// Output container path (e.g. `<data_dir>/recordings/<rom>-<utc>.mp4`).
    pub out_path: PathBuf,
    /// Video frame width in pixels — this session's fixed
    /// [`crate::emu_thread::EmuCore::fb_dims`] width.
    pub width: u32,
    /// Video frame height in pixels — this session's fixed active height
    /// (NTSC/PAL/SECAM differ; a recording session is pinned to whatever
    /// region was active when it was armed).
    pub height: u32,
    /// Audio sample rate (Hz) — the device rate, matching the drained samples.
    pub sample_rate: u32,
    /// Video frame rate as an exact rational (`num/den`) so NTSC's ~59.92 fps
    /// / PAL's 50 fps stay drift-free across long recordings.
    pub fps_num: u32,
    /// Video frame rate denominator.
    pub fps_den: u32,
}

/// Bytes per produced framebuffer at `params.width x params.height` (RGBA8).
#[must_use]
const fn frame_bytes(width: u32, height: u32) -> usize {
    (width as usize) * (height as usize) * 4
}

/// Build the `ffmpeg` argument vector for the final (stop-time) mux.
///
/// Pure + side-effect-free so it can be unit-tested without spawning
/// anything. Both inputs are **complete on-disk raw files** read at mux
/// time: input 0 is the rawvideo (`rgba`, at `params.width x params.height`,
/// at the region frame rate); input 1 is the mono `f32le` PCM. Output is
/// H.264 + AAC into the chosen container (CRF 18, `veryfast`, 192 kbit/s AAC
/// — see this module's own doc comment for why this v1 doesn't expose those
/// as tunables yet).
#[must_use]
pub fn ffmpeg_args(params: &AvParams, video_raw: &Path, audio_raw: &Path) -> Vec<String> {
    vec![
        // Overwrite the output without prompting.
        "-y".into(),
        // ---- input 0: rawvideo from the completed video temp file ----
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgba".into(),
        "-video_size".into(),
        format!("{}x{}", params.width, params.height),
        "-framerate".into(),
        format!("{}/{}", params.fps_num, params.fps_den),
        "-i".into(),
        video_raw.to_string_lossy().into_owned(),
        // ---- input 1: mono f32le PCM from the completed audio temp file ----
        "-f".into(),
        "f32le".into(),
        "-ar".into(),
        params.sample_rate.to_string(),
        "-ac".into(),
        "1".into(),
        "-i".into(),
        audio_raw.to_string_lossy().into_owned(),
        // ---- encode: H.264 + AAC, one fixed sane quality preset ----
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-crf".into(),
        "18".into(),
        // yuv420p so the output plays everywhere (rgba -> yuv420p).
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        // Stop at the shorter stream so a slightly-uneven A/V tail doesn't pad.
        "-shortest".into(),
        params.out_path.to_string_lossy().into_owned(),
    ]
}

/// An active A/V recording session.
///
/// Buffers rawvideo + mono-`f32le` audio to two temp files while recording;
/// no child process is alive until [`AvRecorder::stop`] muxes the two
/// completed files with a single `ffmpeg` invocation. `ffmpeg` presence is
/// verified at [`AvRecorder::start`]. Dropping without [`AvRecorder::stop`]
/// removes both temp files (no encode is produced).
pub struct AvRecorder {
    params: AvParams,
    /// Buffered writer for the rawvideo capture (input 0). Taken (closed) in
    /// [`AvRecorder::stop`] before `ffmpeg` runs, so the muxer sees a fully
    /// flushed file.
    video: Option<io::BufWriter<std::fs::File>>,
    video_path: PathBuf,
    /// Buffered writer for the mono-`f32le` audio capture (input 1). Closed in
    /// [`AvRecorder::stop`] before `ffmpeg` runs.
    audio: Option<io::BufWriter<std::fs::File>>,
    audio_path: PathBuf,
    /// Frames written so far (status reporting — the Tools menu shows this).
    frames: u64,
    /// Audio samples written so far (informational).
    samples: u64,
}

impl AvRecorder {
    /// Arm a recording: verify `ffmpeg` is available, then create the two temp
    /// capture files. Returns [`AvError::FfmpegMissing`] (recording unavailable)
    /// if `ffmpeg` is not installed — the caller should surface a status
    /// message and carry on.
    ///
    /// No `ffmpeg` child is spawned here; muxing happens once, at
    /// [`AvRecorder::stop`], from the completed temp files.
    ///
    /// # Errors
    /// Fails with [`AvError::FfmpegMissing`] if `ffmpeg` cannot be spawned, or
    /// [`AvError::Sidecar`] if a temp capture file cannot be created.
    pub fn start(params: AvParams) -> Result<Self, AvError> {
        // Probe ffmpeg up front so arming fails fast + gracefully when it is
        // absent (the actual mux runs at stop()). `-version` exits immediately.
        Self::probe_ffmpeg().map_err(AvError::FfmpegMissing)?;

        // Capture temps live next to the output so they share its filesystem
        // (cheap cleanup) and are unique per recording.
        let video_path = capture_path(&params.out_path, ".video.rusty2600-avtmp");
        let audio_path = capture_path(&params.out_path, ".audio.rusty2600-avtmp");

        let video_file = std::fs::File::create(&video_path).map_err(AvError::Sidecar)?;
        let audio_file = std::fs::File::create(&audio_path).map_err(|e| {
            // Don't leak the video temp if the audio temp create fails.
            let _ = std::fs::remove_file(&video_path);
            AvError::Sidecar(e)
        })?;

        Ok(Self {
            params,
            video: Some(io::BufWriter::new(video_file)),
            video_path,
            audio: Some(io::BufWriter::new(audio_file)),
            audio_path,
            frames: 0,
            samples: 0,
        })
    }

    /// Run `ffmpeg -version` to confirm the binary is on `PATH` and spawnable.
    fn probe_ffmpeg() -> Result<(), io::Error> {
        let status = Command::new("ffmpeg")
            .arg("-version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("ffmpeg -version exited {status}")))
        }
    }

    /// Append one produced video frame (RGBA8, `params.width x params.height`)
    /// to the video temp file.
    ///
    /// A short / mis-sized framebuffer is silently ignored (defensive: never
    /// feed `ffmpeg` a frame of the wrong stride, e.g. mid-region-switch); a
    /// write failure returns an error so the caller can stop.
    ///
    /// # Errors
    /// Returns [`AvError::Sidecar`] if the video temp file write fails.
    pub fn push_video(&mut self, framebuffer: &[u8]) -> Result<(), AvError> {
        // A silent no-op here would let `push_audio` (called independently at
        // every call site) keep appending for a frame whose video half never
        // wrote, permanently drifting the audio stream ahead of the video
        // stream for the rest of the recording. Erroring lets the caller's
        // existing `is_err() => stop recording` handling take over instead,
        // matching every other failure mode this recorder already has.
        if framebuffer.len() != frame_bytes(self.params.width, self.params.height) {
            return Err(AvError::Sidecar(io::Error::new(
                io::ErrorKind::InvalidData,
                "framebuffer size does not match the recording's fixed dimensions",
            )));
        }
        let Some(video) = self.video.as_mut() else {
            return Err(AvError::Sidecar(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "video capture already closed",
            )));
        };
        video.write_all(framebuffer).map_err(AvError::Sidecar)?;
        self.frames += 1;
        Ok(())
    }

    /// Append this frame's audio samples (mono `f32`) to the audio temp file.
    ///
    /// # Errors
    /// Returns [`AvError::Sidecar`] on an audio temp write failure.
    pub fn push_audio(&mut self, samples: &[f32]) -> Result<(), AvError> {
        // f32le: little-endian IEEE-754, exactly what ffmpeg's `f32le` expects.
        let Some(audio) = self.audio.as_mut() else {
            return Err(AvError::Sidecar(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "audio capture already closed",
            )));
        };
        for &s in samples {
            audio
                .write_all(&s.to_le_bytes())
                .map_err(AvError::Sidecar)?;
        }
        self.samples += samples.len() as u64;
        Ok(())
    }

    /// Frames written so far.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// The output path this session writes to.
    #[must_use]
    pub fn out_path(&self) -> &Path {
        &self.params.out_path
    }

    /// Finalize: flush + close both temp capture files, spawn `ffmpeg` once to
    /// mux the two COMPLETE files, wait for it, then delete the temps.
    /// Consumes `self`.
    ///
    /// # Errors
    /// Returns [`AvError::Sidecar`] on a final flush failure, or
    /// [`AvError::Encode`] if `ffmpeg` cannot be spawned or exits non-zero.
    pub fn stop(mut self) -> Result<PathBuf, AvError> {
        // Flush + close both inputs (taking the Option drops the BufWriter,
        // closing the file handle) so ffmpeg reads two fully-written files.
        if let Some(mut video) = self.video.take() {
            video.flush().map_err(AvError::Sidecar)?;
        }
        if let Some(mut audio) = self.audio.take() {
            audio.flush().map_err(AvError::Sidecar)?;
        }

        let args = ffmpeg_args(&self.params, &self.video_path, &self.audio_path);
        let result = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            // ffmpeg is chatty on stderr; silence it (errors surface via the
            // exit status).
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        // Best-effort temp cleanup regardless of the encode outcome.
        let _ = std::fs::remove_file(&self.video_path);
        let _ = std::fs::remove_file(&self.audio_path);

        match result {
            Ok(status) if status.success() => Ok(self.params.out_path.clone()),
            Ok(status) => Err(AvError::Encode(format!(
                "ffmpeg exited with {status} ({} frames, {} samples)",
                self.frames, self.samples
            ))),
            Err(e) => Err(AvError::Encode(format!("ffmpeg spawn failed: {e}"))),
        }
    }
}

impl Drop for AvRecorder {
    fn drop(&mut self) {
        // If the session was dropped without stop() (e.g. ROM closed mid-record
        // or the app exiting), drop the writers and remove both temp files so
        // we don't leak stray captures. No child process is alive to reap.
        self.video.take();
        self.audio.take();
        let _ = std::fs::remove_file(&self.video_path);
        let _ = std::fs::remove_file(&self.audio_path);
    }
}

/// Derive a capture-temp path for an output path: `<out><suffix>`.
#[must_use]
fn capture_path(out_path: &Path, suffix: &str) -> PathBuf {
    let mut p = out_path.as_os_str().to_os_string();
    p.push(suffix);
    PathBuf::from(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> AvParams {
        AvParams {
            out_path: PathBuf::from("/tmp/out.mp4"),
            width: 160,
            height: 192,
            sample_rate: 48_000,
            fps_num: 60_098_814,
            fps_den: 1_000_000,
        }
    }

    #[test]
    fn frame_bytes_matches_dimensions() {
        assert_eq!(frame_bytes(160, 192), 160 * 192 * 4);
        assert_eq!(frame_bytes(160, 228), 160 * 228 * 4); // PAL/SECAM active height
    }

    #[test]
    fn container_inference_is_case_insensitive() {
        assert_eq!(Container::from_path(Path::new("a.mp4")), Container::Mp4);
        assert_eq!(Container::from_path(Path::new("a.MP4")), Container::Mp4);
        assert_eq!(Container::from_path(Path::new("a.mkv")), Container::Mkv);
        assert_eq!(Container::from_path(Path::new("a.MKV")), Container::Mkv);
        // Unknown / missing extension defaults to mp4.
        assert_eq!(Container::from_path(Path::new("a.avi")), Container::Mp4);
        assert_eq!(Container::from_path(Path::new("noext")), Container::Mp4);
    }

    #[test]
    fn ffmpeg_args_describe_both_inputs_and_codecs() {
        let p = params();
        let video = capture_path(&p.out_path, ".video.rusty2600-avtmp");
        let audio = capture_path(&p.out_path, ".audio.rusty2600-avtmp");
        let args = ffmpeg_args(&p, &video, &audio);

        // Two inputs: rawvideo file + the f32le audio file (both complete on
        // disk at mux time — neither is a pipe).
        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 2);
        assert!(args.iter().any(|a| a == "rawvideo"));
        assert!(args.iter().any(|a| a == "rgba"));
        assert!(args.iter().any(|a| a == "f32le"));
        assert!(!args.iter().any(|a| a == "pipe:0"));
        // Frame size + exact rational frame rate are passed through verbatim.
        assert!(args.iter().any(|a| a == "160x192"));
        assert!(args.iter().any(|a| a == "60098814/1000000"));
        assert!(args.iter().any(|a| a == "48000"));
        let ac_idx = args.iter().position(|a| a == "-ac").unwrap();
        assert_eq!(args[ac_idx + 1], "1");
        assert!(args.iter().any(|a| a == "libx264"));
        assert!(args.iter().any(|a| a == "aac"));
        assert!(args.iter().any(|a| a == "-shortest"));
        assert_eq!(args.last().unwrap(), "/tmp/out.mp4");
    }

    #[test]
    fn ffmpeg_args_respect_pal_dimensions() {
        let mut p = params();
        p.height = 228; // PAL/SECAM active height
        let args = ffmpeg_args(&p, Path::new("/tmp/v.raw"), Path::new("/tmp/a.pcm"));
        assert!(args.iter().any(|a| a == "160x228"));
    }

    #[test]
    fn capture_paths_are_derived_from_output_and_distinct() {
        let out = Path::new("/x/y/rec.mp4");
        let v = capture_path(out, ".video.rusty2600-avtmp");
        let a = capture_path(out, ".audio.rusty2600-avtmp");
        assert_eq!(v, PathBuf::from("/x/y/rec.mp4.video.rusty2600-avtmp"));
        assert_eq!(a, PathBuf::from("/x/y/rec.mp4.audio.rusty2600-avtmp"));
        assert_ne!(v, a);
    }
}
