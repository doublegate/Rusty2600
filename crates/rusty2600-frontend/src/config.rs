//! Frontend configuration (TOML), loaded from the platform config dir and surfaced in the tabbed
//! Settings window.
//!
//! Carries the display-sync pacing preference, the region (NTSC/PAL/SECAM → frame-rate target),
//! the audio settings, and the per-player [`crate::input::KeyBindings`]. This is the RustyNES
//! config schema, 2600-adapted (the region drives the frame rate + the active scanline count, and
//! the binding table maps keys to the 2600 joystick / console-switch actions, not an NES d-pad).

use serde::{Deserialize, Serialize};

use crate::input::KeyBindings;
use crate::palette::Region;

/// The display-sync pacing strategy (the RustyNES pacing matrix, ported).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacingMode {
    /// Pick the best mode from the display + present-mode caps (default).
    #[default]
    Auto,
    /// Lock to the display's refresh (Fifo vsync); audio resampled to fit.
    Display,
    /// Variable-refresh-rate aware (present when the frame is ready).
    Vrr,
    /// Free-run on the wall clock at the region frame rate; present-mode mailbox/immediate.
    Wallclock,
}

/// Video / windowing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    /// The wgpu present mode preference (`"fifo"` / `"mailbox"` / `"immediate"`).
    pub present_mode: String,
    /// The display-sync pacing strategy.
    pub pacing: PacingMode,
    /// Integer-scale the framebuffer (true) or fit-to-window with aspect correction (false).
    pub integer_scale: bool,
    /// Run-ahead frame count (`0` = off, the default). See [`crate::runahead`].
    ///
    /// Each additional frame hides one more frame of a game's internal input
    /// lag, at the cost of running that many extra hidden frames per real
    /// tick under the `emu-thread` feature.
    pub runahead_frames: u32,
    /// The active post-process shader stack, in order (empty = off, the
    /// default — the byte-identical direct blit). See `crate::shader_pass`.
    pub shader_passes: Vec<rusty2600_gfx_shaders::PassKind>,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            present_mode: "fifo".into(),
            pacing: PacingMode::default(),
            integer_scale: false,
            runahead_frames: 0,
            shader_passes: Vec::new(),
        }
    }
}

/// Audio settings (the lock-free ring + dynamic-rate-control servo live in `audio.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Master output sample rate (the cpal stream target; the resampler fits the TIA's native
    /// rate to it).
    pub sample_rate: u32,
    /// Master volume in `0.0..=1.0`.
    pub volume: f32,
    /// Whether audio output is enabled at all.
    pub enabled: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            volume: 0.8,
            enabled: true,
        }
    }
}

/// The number of manual save-state slots per ROM (`File -> Save State` /
/// `Load State`), matching RustyNES's own 8-slot convention — a 2600 cart's
/// serialized `System` is tiny, so 8 slots costs nothing.
pub const SAVE_SLOT_COUNT: u8 = 8;

/// The save-state slot file extension, matching this project's existing
/// `.r26m` TAS-movie convention. Native-only, matching every other save-slot
/// path helper below.
#[cfg(not(target_arch = "wasm32"))]
const SAVE_SLOT_EXTENSION: &str = "r26s";

/// The full frontend config (serialized to `config.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The console region (timing + active scanlines + palette).
    pub region: Region,
    /// Video / windowing.
    pub video: VideoConfig,
    /// Audio.
    pub audio: AudioConfig,
    /// Player 1 keyboard binds (joystick + the console switches).
    pub p1: KeyBindings,
    /// Player 2 keyboard binds (the second joystick; the default table already carries both
    /// players + the switches, so this is a per-user override hook).
    pub p2: KeyBindings,
}

impl Config {
    /// The on-disk config path (`<platform-config-dir>/Rusty2600/config.toml`), or `None` if no
    /// config dir is resolvable. Native-only.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("io.github", "doublegate", "Rusty2600")
            .map(|d| d.config_dir().join("config.toml"))
    }

    /// Load the config from disk, falling back to defaults on any error (a missing or corrupt file
    /// should never block launch). Native-only.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        std::fs::read_to_string(&path).map_or_else(
            |_| Self::default(),
            |s| toml::from_str(&s).unwrap_or_default(),
        )
    }

    /// Persist the config to disk (best-effort; creates the parent dir). Native-only.
    ///
    /// # Errors
    /// Returns an [`std::io::Error`] if the directory cannot be created or the file written.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let s = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, s)
    }

    /// wasm32's `save()` — a no-op. `wasm-winit`'s `App::with_config` always starts from
    /// `Config::default()` (no `Config::load()` counterpart exists for this target either), so
    /// there is nothing to persist to yet; `SetRegion`/`SaveConfig`'s dispatch arms call this
    /// unconditionally on both targets, so it must exist here with the same signature rather than
    /// needing its own `#[cfg]` at every call site. Real wasm persistence
    /// (`localStorage`/IndexedDB) is later-release scope (`v2.8.0`).
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unnecessary_wraps)]
    pub const fn save(&self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The base directory all ROMs' save-state slots live under
/// (`<platform-data-dir>/Rusty2600/saves`), or `None` if no data dir is
/// resolvable.
///
/// Native-only, mirroring [`Config::path`]'s own exclusion — separate from
/// the config dir since these are user save DATA, not settings.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn saves_base_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("io.github", "doublegate", "Rusty2600")
        .map(|d| d.data_dir().join("saves"))
}

/// The pure path-construction rule, parameterized over an explicit `base`
/// directory so it's testable without touching the real platform data dir
/// (see the `tests` module below) — `save_slot_dir`/`save_slot_path` are
/// thin wrappers supplying the real [`saves_base_dir`]. Native-only, matching
/// every other save-slot path helper (only ever called from native-gated
/// code, so this would otherwise be reported as dead code on wasm builds).
#[cfg(not(target_arch = "wasm32"))]
fn save_slot_dir_under(base: &std::path::Path, rom_tag: u64) -> std::path::PathBuf {
    base.join(format!("{rom_tag:016x}"))
}

/// See [`save_slot_dir_under`]. Native-only, for the same reason.
#[cfg(not(target_arch = "wasm32"))]
fn save_slot_path_under(base: &std::path::Path, rom_tag: u64, slot: u8) -> std::path::PathBuf {
    save_slot_dir_under(base, rom_tag).join(format!("slot_{slot}.{SAVE_SLOT_EXTENSION}"))
}

/// The save-slot directory for `rom_tag`
/// (`<saves-base-dir>/<rom_tag as 16-digit lowercase hex>/`).
///
/// Keeping each ROM's slots in their own directory means different games'
/// saves can never collide, and a game's whole save history is trivially
/// deletable/relocatable as one unit. Native-only.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn save_slot_dir(rom_tag: u64) -> Option<std::path::PathBuf> {
    saves_base_dir().map(|base| save_slot_dir_under(&base, rom_tag))
}

/// The path to one save-state slot file
/// (`<save-slot-dir>/slot_<N>.r26s`). Native-only.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn save_slot_path(rom_tag: u64, slot: u8) -> Option<std::path::PathBuf> {
    saves_base_dir().map(|base| save_slot_path_under(&base, rom_tag, slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_toml() {
        let cfg = Config::default();
        let s = toml::to_string_pretty(&cfg).expect("serialize");
        let back: Config = toml::from_str(&s).expect("deserialize");
        assert_eq!(back.region, cfg.region);
        assert_eq!(back.audio.sample_rate, cfg.audio.sample_rate);
        assert_eq!(back.p1.binds.len(), cfg.p1.binds.len());
    }

    #[test]
    fn region_serializes_uppercase() {
        let cfg = Config {
            region: Region::Secam,
            ..Config::default()
        };
        let s = toml::to_string_pretty(&cfg).expect("serialize");
        assert!(
            s.contains("SECAM"),
            "region should serialize UPPERCASE: {s}"
        );
    }

    #[test]
    fn slot_path_is_keyed_by_rom_tag_and_slot() {
        let base = std::path::Path::new("/tmp/rusty2600-example");
        let a = save_slot_path_under(base, 0xDEAD_BEEF, 3);
        let b = save_slot_path_under(base, 0xDEAD_BEEF, 4);
        let c = save_slot_path_under(base, 0xCAFE_F00D, 3);
        assert_ne!(a, b, "different slots must not collide");
        assert_ne!(a, c, "different ROMs must not collide");
        assert!(a.to_string_lossy().contains("deadbeef"));
        assert!(a.to_string_lossy().ends_with("slot_3.r26s"));
    }

    /// A real end-to-end save-then-load through the slot-path plumbing
    /// (the one genuinely new integration point this feature adds on top of
    /// the already-tested `SaveState::capture`/`encode`/`decode`/`restore`)
    /// — isolated from the real platform data dir via an explicit `base`
    /// under `std::env::temp_dir()`, unique per test run so parallel test
    /// threads never collide, cleaned up afterward.
    #[test]
    fn save_then_load_round_trips_through_slot_path() {
        use rusty2600_core::{SaveState, System};

        let base = std::env::temp_dir().join(format!(
            "rusty2600-slot-test-{}-{}",
            std::process::id(),
            "save_then_load_round_trips_through_slot_path"
        ));
        let rom_tag = 0x1234_5678_9abc_def0_u64;
        let slot = 2u8;
        let dir = save_slot_dir_under(&base, rom_tag);
        let path = save_slot_path_under(&base, rom_tag, slot);
        std::fs::create_dir_all(&dir).expect("create slot dir");

        let mut system = System::new(7);
        system.step_instruction();
        system.step_instruction();
        let before_pc = system.cpu.pc;
        let before_clocks = system.color_clocks();

        let bytes = SaveState::capture(&system, rom_tag).encode();
        std::fs::write(&path, &bytes).expect("write slot file");

        // Advance the "live" system further so restoring is a real rewind,
        // not a no-op identity check.
        system.step_instruction();
        system.step_instruction();
        assert_ne!(system.color_clocks(), before_clocks);

        let read_back = std::fs::read(&path).expect("read slot file");
        let restored = SaveState::restore(&read_back, rom_tag).expect("slot file should restore");

        assert_eq!(restored.cpu.pc, before_pc);
        assert_eq!(restored.color_clocks(), before_clocks);

        // A wrong `rom_tag` must be rejected, not silently loaded.
        assert!(SaveState::restore(&read_back, rom_tag.wrapping_add(1)).is_err());

        let _ = std::fs::remove_dir_all(&base);
    }
}
