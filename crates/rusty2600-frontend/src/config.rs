//! Frontend configuration (TOML), loaded from the platform config dir and surfaced in the tabbed
//! Settings window.
//!
//! Carries the display-sync pacing preference, the region (NTSC/PAL/SECAM → frame-rate target),
//! the audio settings, and the per-player [`crate::input::KeyBindings`]. This is the RustyNES
//! config schema, 2600-adapted (the region drives the frame rate + the active scanline count, and
//! the binding table maps keys to the 2600 joystick / console-switch actions, not an NES d-pad).
//!
//! Persistence has two backends, selected by target: native reads/writes a real `config.toml`
//! file under the platform config dir ([`Config::path`]); wasm32 has no filesystem, so it
//! persists the same TOML-serialized schema to a single `localStorage` key instead (`[v2.8.0]`;
//! see [`Config::load`]/[`Config::save`]'s wasm32 doc comments). Both backends share the same
//! pure `to_toml_string`/`from_toml_str` (de)serialization helpers.

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

/// The `localStorage` key the wasm32 build persists its config under. Native-only counterpart is
/// [`Config::path`] (a real file); the wasm32 build has no filesystem, so a single string key
/// under the page's origin-scoped storage is the equivalent "where does this live" concept.
#[cfg(target_arch = "wasm32")]
const LOCAL_STORAGE_KEY: &str = "rusty2600.config";

impl Config {
    /// Serialize to the same TOML representation both persistence backends store (the native
    /// `config.toml` file, the wasm32 `localStorage` value under [`LOCAL_STORAGE_KEY`]) — pure,
    /// target-agnostic, no filesystem/browser API involved, so it compiles and is exercised by the
    /// ordinary native `cargo test --workspace` run rather than only a wasm32-only build.
    ///
    /// `pub(crate)` as of `[v2.9.0]`: [`crate::share_link`] reuses this same helper (rather than
    /// hand-duplicating a parallel serializer) to encode the `?settings=` share-link blob — see
    /// its module doc for why the WHOLE `Config` is safe to share on this project (unlike
    /// RustyNES's own curated-subset `ShareSettings`, Rusty2600's `Config` carries no
    /// machine-local paths or login tokens).
    pub(crate) fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Parse a previously-persisted TOML string, falling back to defaults on any parse error (a
    /// missing, foreign, or corrupt persisted value must never block launch on either target). See
    /// [`Self::to_toml_string`] for why this is deliberately target-agnostic, and for the
    /// `pub(crate)` visibility as of `[v2.9.0]`.
    pub(crate) fn from_toml_str(s: &str) -> Self {
        toml::from_str(s).unwrap_or_default()
    }

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
        std::fs::read_to_string(&path).map_or_else(|_| Self::default(), |s| Self::from_toml_str(&s))
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
        let s = self
            .to_toml_string()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, s)
    }

    /// wasm32's `load()` — the `localStorage` counterpart to the native path's `config.toml` read.
    /// Falls back to defaults (never blocks launch) if there is no `window`/no storage available
    /// (private-browsing/storage-disabled), the key isn't set yet (first run), or the stored value
    /// fails to parse (`[v2.8.0]`; previously an explicit no-op — this target had no `load()` at
    /// all, and `wasm.rs::run_winit` always started from `Config::default()`).
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn load() -> Self {
        Self::local_storage()
            .and_then(|storage| storage.get_item(LOCAL_STORAGE_KEY).ok().flatten())
            .map_or_else(Self::default, |s| Self::from_toml_str(&s))
    }

    /// wasm32's `save()` — persists to `localStorage` (best-effort: a browser with storage
    /// disabled/full silently keeps running with an unpersisted config rather than treating this
    /// as a hard error, matching the native path's own "never block on a persistence failure"
    /// posture). `[v2.8.0]`; previously an explicit no-op stub (see this project's `docs/
    /// frontend.md` for the prior "later-release scope" note this closes).
    ///
    /// # Errors
    /// Never actually returns an `Err` — kept as `Result` to match the native `save()` signature
    /// every call site dispatches through identically (see `crate::shell::MenuAction::SaveConfig`'s
    /// dispatch arm in `app.rs`), even though this target's persistence failures are swallowed
    /// rather than propagated.
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::unnecessary_wraps)]
    pub fn save(&self) -> std::io::Result<()> {
        if let Ok(s) = self.to_toml_string()
            && let Some(storage) = Self::local_storage()
        {
            let _ = storage.set_item(LOCAL_STORAGE_KEY, &s);
        }
        Ok(())
    }

    /// `window().local_storage()` returns `Result<Option<Storage>, JsValue>` — collapse every
    /// failure mode (no `window` at all, the call itself erroring, or no storage object available)
    /// to a plain `None`, since [`Self::load`]/[`Self::save`] only ever want a best-effort handle.
    #[cfg(target_arch = "wasm32")]
    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
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

    /// The pure `to_toml_string`/`from_toml_str` helpers both persistence backends share
    /// (`config.toml` on native, `localStorage` on wasm32 — see [`Config::save`]/[`Config::load`]'s
    /// wasm32 doc comments) — exercised here target-agnostically so this is real coverage under
    /// the ordinary native `cargo test --workspace` run, not just a wasm32-only build nobody runs
    /// in CI.
    #[test]
    fn to_toml_string_round_trips_through_from_toml_str() {
        let cfg = Config {
            region: Region::Pal,
            audio: AudioConfig {
                volume: 0.42,
                ..AudioConfig::default()
            },
            video: VideoConfig {
                integer_scale: true,
                ..VideoConfig::default()
            },
            ..Config::default()
        };

        let s = cfg.to_toml_string().expect("serialize");
        let back = Config::from_toml_str(&s);

        assert_eq!(back.region, cfg.region);
        assert!((back.audio.volume - 0.42).abs() < f32::EPSILON);
        assert!(back.video.integer_scale);
    }

    /// A corrupt/foreign persisted value must fall back to defaults rather than panicking or
    /// propagating a parse error — this is the exact case a stale/incompatible `localStorage`
    /// value (e.g. from an older schema version) hits on wasm32, and the exact case a hand-edited
    /// or truncated `config.toml` hits natively.
    #[test]
    fn from_toml_str_falls_back_to_default_on_garbage() {
        let back = Config::from_toml_str("this is not valid toml {{{");
        assert_eq!(back.audio.sample_rate, Config::default().audio.sample_rate);
        assert_eq!(back.region, Config::default().region);
    }

    /// An empty string (the realistic "key exists but was never written," or a wiped value) must
    /// also fall back to defaults cleanly, not just genuinely malformed TOML.
    #[test]
    fn from_toml_str_handles_empty_string() {
        let back = Config::from_toml_str("");
        assert_eq!(back.audio.sample_rate, Config::default().audio.sample_rate);
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

    // `save_slot_path_under`/`save_slot_dir_under` are native-only (see their own doc
    // comments) — this test (and `save_then_load_round_trips_through_slot_path` below) must be
    // gated the same way so `cargo clippy --target wasm32-unknown-unknown --all-targets` can
    // actually type-check the test binary (first verified in `[v2.8.0]`; this exact wasm32
    // `--all-targets` invocation had not been run before).
    #[cfg(not(target_arch = "wasm32"))]
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
    /// threads never collide, cleaned up afterward. Native-only for the same reason
    /// `slot_path_is_keyed_by_rom_tag_and_slot` above is.
    #[cfg(not(target_arch = "wasm32"))]
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
