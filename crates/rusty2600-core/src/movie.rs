//! TAS movies — a versioned, deterministic recording of a play session.
//!
//! A `.r26m` movie is a start point (either a fresh seeded power-on, per
//! ADR 0006, or an embedded [`crate::SaveState`] blob — a branch point is
//! exactly this) plus a per-frame log of every host input the 2600 exposes.
//! Replaying a movie against the ROM it was recorded against reproduces the
//! exact same run, by the same determinism contract save-states already rely
//! on (ADR 0004: same seed/ROM/input ⇒ bit-identical output).
//!
//! Deliberately mirrors [`crate::save_state`]'s structure and header
//! conventions (magic + format version + `rom_tag`, `postcard`-encoded, a
//! typed decode error) rather than inventing a parallel scheme — but is its
//! own format with its own magic and version counter, since a movie and a
//! save-state answer different questions (a whole run vs. one instant).
//!
//! `MovieFrame`, this module's per-frame record, is deliberately NOT
//! `rusty2600_frontend::input::InputState` — this crate cannot depend on the
//! frontend crate (the crate graph is one-directional: frontend depends on
//! core, never the reverse). The frontend converts `InputState <-> MovieFrame`
//! when recording/replaying.

use alloc::vec::Vec;

use crate::save_state::{SaveState, SaveStateError};
use crate::scheduler::System;

/// The movie file magic (`"R26M"` — distinct from save-state's `"R26S"`).
const MAGIC: [u8; 4] = *b"R26M";

/// The current movie format version. Independent of save-state's own
/// version counter (they're different formats answering different
/// questions), but follows the same migration spirit as
/// `docs/adr/0007-save-state-versioning.md`: same MAJOR.MINOR round-trips
/// byte-identical; additive fields within a MINOR use `#[serde(default)]`;
/// anything else bumps this and is handled explicitly by [`Movie::restore`].
const FORMAT_VERSION: u16 = 1;

/// The oldest format version this build can still read.
const MIN_SUPPORTED_FORMAT_VERSION: u16 = 1;

/// The TV broadcast region a movie was recorded under. A small, deliberate
/// duplication of `rusty2600-frontend::palette::Region` (three variants, not
/// a shared abstraction) — this crate cannot depend on the frontend crate,
/// and region here is just a label carried for reference/playback-config
/// purposes, not palette data, so a full shared type isn't warranted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MovieRegion {
    /// NTSC (60 Hz, 262 lines).
    #[default]
    Ntsc,
    /// PAL (50 Hz, 312 lines).
    Pal,
    /// SECAM (50 Hz, 312 lines, 8-colour palette).
    Secam,
}

/// One frame's worth of host input — everything a 2600 controller/console
/// panel can drive. Console switches are per-frame fields (not header-level
/// constants) because Select/Reset/Color/Difficulty can all change mid-run
/// on real hardware, unlike a fixed NES controller.
///
/// Packed to mirror the RIOT/TIA port-byte conventions
/// `rusty2600-frontend::input` already established, so the frontend's
/// `InputState -> MovieFrame` conversion is a direct reuse of that existing
/// packing logic, not a re-derivation of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MovieFrame {
    /// Packed joystick directions for both ports, same nibble layout as the
    /// RIOT's `SWCHA` (`rusty2600-frontend::input::Joystick::swcha_nibble`):
    /// bits 7-4 = port 0 up/down/left/right, bits 3-0 = port 1. Active-low.
    pub swcha: u8,
    /// The two joystick fire buttons (TIA `INPT4`/`INPT5`, not part of
    /// `SWCHA`): bit 0 = port 0 fire pressed, bit 1 = port 1 fire pressed.
    pub joy_fire: u8,
    /// Packed console switches, same layout as the RIOT's `SWCHB`
    /// (`rusty2600-frontend::input::ConsoleSwitches::swchb`).
    pub swchb: u8,
    /// The four paddles' pot positions (`0` full clockwise ..= `255` full
    /// counter-clockwise, the TIA `INPTx` dump-capacitor value).
    pub paddle_pos: [u8; 4],
    /// The four paddles' fire buttons, one bit each (bit N = paddle N).
    pub paddle_fire: u8,
}

impl Default for MovieFrame {
    /// The idle frame: no direction/fire/switch pressed. `swcha`/`swchb`
    /// default HIGH (`0xFF`), matching real hardware's active-low pull-ups
    /// — a naive all-zero default would instead mean "every direction and
    /// switch held down simultaneously," the opposite of idle.
    fn default() -> Self {
        Self {
            swcha: 0xFF,
            joy_fire: 0,
            swchb: 0xFF,
            paddle_pos: [0; 4],
            paddle_fire: 0,
        }
    }
}

/// Where a movie's recorded input starts from.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MovieStart {
    /// A fresh power-on with the given seed (ADR 0006: same seed ⇒
    /// byte-identical seeded RIOT RAM / CPU `A`/`X`/`Y`).
    PowerOn {
        /// The `System::new` seed.
        seed: u64,
    },
    /// An embedded save-state blob (`crate::SaveState::encode`'s wire
    /// format) — a branch point is exactly a movie whose start point is the
    /// save-state captured at the branch frame.
    FromSaveState(Vec<u8>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MovieHeader {
    magic: [u8; 4],
    format_version: u16,
    region: MovieRegion,
    rom_tag: u64,
}

/// Everything that can go wrong loading a movie. Mirrors
/// [`SaveStateError`]'s shape for the same failure modes.
#[derive(Debug)]
pub enum MovieError {
    /// The byte stream didn't decode as a `Movie` at all (truncated,
    /// corrupt, or not a movie file).
    Malformed,
    /// The decoded header's magic didn't match — this isn't a Rusty2600
    /// movie file.
    BadMagic,
    /// The movie's `rom_tag` doesn't match the ROM currently loaded.
    RomMismatch,
    /// The movie's format version is older than this build can read.
    UnsupportedFormat {
        /// The format version stored in the file.
        file_version: u16,
        /// The oldest format version this build supports.
        min_supported: u16,
    },
    /// [`MovieStart::FromSaveState`]'s embedded blob failed to decode.
    BadEmbeddedSaveState(SaveStateError),
}

impl core::fmt::Display for MovieError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Malformed => write!(f, "movie data is malformed or truncated"),
            Self::BadMagic => write!(f, "not a Rusty2600 movie file"),
            Self::RomMismatch => write!(f, "movie was recorded against a different ROM"),
            Self::UnsupportedFormat {
                file_version,
                min_supported,
            } => write!(
                f,
                "movie format v{file_version} is older than the minimum supported format v{min_supported}"
            ),
            Self::BadEmbeddedSaveState(e) => {
                write!(f, "movie's embedded save-state is invalid: {e}")
            }
        }
    }
}

/// A recorded (or in-progress) TAS movie: a start point plus a per-frame
/// input log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Movie {
    header: MovieHeader,
    start: MovieStart,
    frames: Vec<MovieFrame>,
}

impl Movie {
    /// Begin a new movie from a fresh power-on.
    #[must_use]
    pub fn new_power_on(rom_tag: u64, region: MovieRegion, seed: u64) -> Self {
        Self {
            header: MovieHeader {
                magic: MAGIC,
                format_version: FORMAT_VERSION,
                region,
                rom_tag,
            },
            start: MovieStart::PowerOn { seed },
            frames: Vec::new(),
        }
    }

    /// Begin a new movie (a branch point) starting from `system`'s current
    /// state, captured via [`SaveState::capture`].
    #[must_use]
    pub fn new_branch(rom_tag: u64, region: MovieRegion, system: &System) -> Self {
        let blob = SaveState::capture(system, rom_tag).encode();
        Self {
            header: MovieHeader {
                magic: MAGIC,
                format_version: FORMAT_VERSION,
                region,
                rom_tag,
            },
            start: MovieStart::FromSaveState(blob),
            frames: Vec::new(),
        }
    }

    /// This movie's recorded region.
    #[must_use]
    pub const fn region(&self) -> MovieRegion {
        self.header.region
    }

    /// This movie's start point.
    #[must_use]
    pub const fn start(&self) -> &MovieStart {
        &self.start
    }

    /// The recorded frames so far, in playback order.
    #[must_use]
    pub fn frames(&self) -> &[MovieFrame] {
        &self.frames
    }

    /// Append one frame of input to the recording.
    pub fn record_frame(&mut self, frame: MovieFrame) {
        self.frames.push(frame);
    }

    /// Overwrite an already-recorded frame (piano-roll editing). No-op if
    /// `index` is out of bounds.
    pub fn set_frame(&mut self, index: usize, frame: MovieFrame) {
        if let Some(slot) = self.frames.get_mut(index) {
            *slot = frame;
        }
    }

    /// The frame at `index`, if recorded.
    #[must_use]
    pub fn frame_at(&self, index: usize) -> Option<MovieFrame> {
        self.frames.get(index).copied()
    }

    /// Number of recorded frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether no frames have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Rebuild the [`System`] this movie's start point describes. For
    /// [`MovieStart::PowerOn`] this is a fresh, deterministically-seeded
    /// system; for [`MovieStart::FromSaveState`] this decodes the embedded
    /// blob (validated against `rom_tag`).
    pub fn start_system(&self, rom_tag: u64) -> Result<System, MovieError> {
        match &self.start {
            MovieStart::PowerOn { seed } => Ok(System::new(*seed)),
            MovieStart::FromSaveState(blob) => {
                SaveState::restore(blob, rom_tag).map_err(MovieError::BadEmbeddedSaveState)
            }
        }
    }

    /// Encodes this movie to its binary wire format.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        // Every field is a plain derive over data this crate itself owns
        // (no trait objects, no external I/O) — this can't fail in practice,
        // matching `SaveState::encode`'s same reasoning.
        postcard::to_allocvec(self).unwrap_or_default()
    }

    /// Decodes a movie from its binary wire format, without checking it
    /// against any particular ROM yet (see [`Self::restore`] for that).
    pub fn decode(bytes: &[u8]) -> Result<Self, MovieError> {
        let movie: Self = postcard::from_bytes(bytes).map_err(|_| MovieError::Malformed)?;
        if movie.header.magic != MAGIC {
            return Err(MovieError::BadMagic);
        }
        if movie.header.format_version < MIN_SUPPORTED_FORMAT_VERSION {
            return Err(MovieError::UnsupportedFormat {
                file_version: movie.header.format_version,
                min_supported: MIN_SUPPORTED_FORMAT_VERSION,
            });
        }
        Ok(movie)
    }

    /// Decodes and validates a movie against `rom_tag`.
    pub fn restore(bytes: &[u8], rom_tag: u64) -> Result<Self, MovieError> {
        let movie = Self::decode(bytes)?;
        if movie.header.rom_tag != rom_tag {
            return Err(MovieError::RomMismatch);
        }
        Ok(movie)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(seed: u8) -> MovieFrame {
        MovieFrame {
            swcha: seed,
            joy_fire: seed & 0b11,
            swchb: seed.wrapping_add(1),
            paddle_pos: [
                seed,
                seed.wrapping_add(1),
                seed.wrapping_add(2),
                seed.wrapping_add(3),
            ],
            paddle_fire: seed & 0b1111,
        }
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let mut movie = Movie::new_power_on(0xDEAD_BEEF, MovieRegion::Ntsc, 42);
        movie.record_frame(sample_frame(1));
        movie.record_frame(sample_frame(2));
        movie.record_frame(sample_frame(3));

        let bytes = movie.encode();
        let restored = Movie::restore(&bytes, 0xDEAD_BEEF).expect("round-trip should succeed");

        assert_eq!(restored.len(), 3);
        assert_eq!(restored.frame_at(0), movie.frame_at(0));
        assert_eq!(restored.frame_at(1), movie.frame_at(1));
        assert_eq!(restored.frame_at(2), movie.frame_at(2));
        assert_eq!(restored.region(), MovieRegion::Ntsc);
    }

    #[test]
    fn rom_tag_mismatch_is_rejected() {
        let movie = Movie::new_power_on(1, MovieRegion::Ntsc, 1);
        let bytes = movie.encode();
        let err = Movie::restore(&bytes, 2).expect_err("mismatched rom_tag must fail");
        assert!(matches!(err, MovieError::RomMismatch));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let movie = Movie::new_power_on(1, MovieRegion::Ntsc, 1);
        let mut bytes = movie.encode();
        bytes[0] ^= 0xFF;
        let err = Movie::decode(&bytes);
        assert!(err.is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let err = Movie::decode(&[0u8; 2]);
        assert!(matches!(err, Err(MovieError::Malformed)));
    }

    #[test]
    fn power_on_start_reproduces_the_seeded_system_deterministically() {
        let movie = Movie::new_power_on(1, MovieRegion::Ntsc, 7);
        let a = movie
            .start_system(1)
            .expect("power-on start always succeeds");
        let b = movie
            .start_system(1)
            .expect("power-on start always succeeds");
        assert_eq!(
            a.bus.riot.ram, b.bus.riot.ram,
            "same seed must seed RAM identically"
        );
        assert_eq!(a.cpu.a, b.cpu.a);
        assert_eq!(a.cpu.x, b.cpu.x);
        assert_eq!(a.cpu.y, b.cpu.y);
    }

    #[test]
    fn branch_point_embeds_a_real_save_state() {
        let mut system = System::new(3);
        system.step_instruction();
        system.step_instruction();

        let movie = Movie::new_branch(0xAAAA, MovieRegion::Pal, &system);
        let restored = movie
            .start_system(0xAAAA)
            .expect("branch start should decode the embedded save-state");
        assert_eq!(restored.color_clocks(), system.color_clocks());
        assert_eq!(restored.bus.riot.ram, system.bus.riot.ram);
    }

    #[test]
    fn edited_frame_overwrites_the_recorded_value() {
        let mut movie = Movie::new_power_on(1, MovieRegion::Ntsc, 1);
        movie.record_frame(sample_frame(0));
        movie.record_frame(sample_frame(1));
        movie.set_frame(0, sample_frame(99));
        assert_eq!(movie.frame_at(0), Some(sample_frame(99)));
        assert_eq!(movie.frame_at(1), Some(sample_frame(1)));
    }
}
