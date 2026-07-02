//! `rusty2600-mobile` — a platform-agnostic `UniFFI` bridge over
//! `rusty2600_core::System`, for the Android (`v1.11.0`) and future iOS
//! (`v1.12.0`) hosts.
//!
//! Deliberately carries NO Android/JNI/Swift types — that glue lives in
//! `rusty2600-android` (the one crate where `unsafe` is permitted, isolated
//! per this project's unsafe-confinement convention). This crate is `std`,
//! host-testable, and reusable as-is by both mobile trains.
//!
//! Mirrors `rusty2600_frontend::input::InputState`'s shape (joystick
//! directions/fire, four paddle positions, console switches) rather than
//! depending on the frontend crate directly — the one-directional crate
//! graph means `rusty2600-mobile` cannot depend on `rusty2600-frontend`.

use std::sync::Mutex;

use rusty2600_cart::detect;
use rusty2600_core::{SaveState, SaveStateError, System};

uniffi::setup_scaffolding!();

/// One joystick's four directions + fire, active-HIGH (`true` = pressed) —
/// a host-language-friendly convention, not a hardware register mirror.
// Five independent bools genuinely model five independent physical inputs
// (four directions + fire); a state machine would be less readable, not more.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, uniffi::Record)]
pub struct MobileJoystick {
    /// Up.
    pub up: bool,
    /// Down.
    pub down: bool,
    /// Left.
    pub left: bool,
    /// Right.
    pub right: bool,
    /// The single fire button.
    pub fire: bool,
}

/// One paddle's position (`0..=255`) + fire button.
#[derive(Debug, Clone, Copy, Default, uniffi::Record)]
pub struct MobilePaddle {
    /// Pot position, `0` (fully clockwise) ..= `255` (fully counter-clockwise).
    pub position: u8,
    /// The paddle's fire button.
    pub fire: bool,
}

/// The six-switch console panel.
// Five independent bools genuinely model five independent physical switches;
// see the identical rationale on `MobileJoystick`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct MobileSwitches {
    /// Game Select (momentary).
    pub select: bool,
    /// Game Reset (momentary).
    pub reset: bool,
    /// `true` = Color, `false` = B&W.
    pub color: bool,
    /// `true` = A ("pro"), `false` = B ("amateur").
    pub left_difficulty: bool,
    /// `true` = A ("pro"), `false` = B ("amateur").
    pub right_difficulty: bool,
}

impl Default for MobileSwitches {
    fn default() -> Self {
        Self {
            select: false,
            reset: false,
            color: true,
            left_difficulty: false,
            right_difficulty: false,
        }
    }
}

/// The complete host-input snapshot for one frame — the mobile-side
/// equivalent of `rusty2600_frontend::input::InputState`.
///
/// `joystick0`/`joystick1` and `paddle0..=paddle3` (rather than
/// `[MobileJoystick; 2]`/`[MobilePaddle; 4]`) because `UniFFI`'s `Record`
/// derive doesn't support fixed-size array fields — only named fields or
/// `Vec<T>`. Named fields keep the generated Kotlin/Swift struct's shape
/// simple (no index-based access into a bridged array type).
#[derive(Debug, Clone, Copy, Default, uniffi::Record)]
pub struct MobileInput {
    /// Port 0 joystick.
    pub joystick0: MobileJoystick,
    /// Port 1 joystick.
    pub joystick1: MobileJoystick,
    /// Paddle A.
    pub paddle0: MobilePaddle,
    /// Paddle B.
    pub paddle1: MobilePaddle,
    /// Paddle C.
    pub paddle2: MobilePaddle,
    /// Paddle D.
    pub paddle3: MobilePaddle,
    /// The console-switch panel.
    pub switches: MobileSwitches,
}

/// One frame's rendered output: an RGBA8 framebuffer (160x192, the same
/// `ATARI_W`/`ATARI_H` the wasm canvas-2D bootstrap uses).
///
/// Also carries this frame's drained, DC-blocked, normalized audio samples
/// (`f32`, `[-1.0, 1.0]`) at the TIA's native ~31.4 kHz rate — the host is
/// responsible for resampling to its own audio output rate (Android's
/// `AudioTrack`/`AAudio`, iOS's `AVAudioEngine`, both support arbitrary
/// input sample rates).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FrameOutput {
    /// RGBA8 framebuffer, `160 * 192 * 4` bytes.
    pub rgba: Vec<u8>,
    /// This frame's drained, DC-blocked, normalized audio samples.
    pub audio_samples: Vec<f32>,
}

/// Everything that can go wrong loading a ROM or a save-state.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    /// `rusty2600_cart::detect` didn't recognize the ROM image.
    #[error("unrecognized or unsupported ROM image")]
    UnrecognizedRom,
    /// A method that needs a running system was called before `load_rom`.
    #[error("no ROM is loaded")]
    NoRomLoaded,
    /// A `SaveState` operation failed (see the wrapped message).
    #[error("save-state error: {0}")]
    SaveState(String),
}

impl From<SaveStateError> for MobileError {
    fn from(e: SaveStateError) -> Self {
        Self::SaveState(e.to_string())
    }
}

const ATARI_W: usize = 160;
const ATARI_H: usize = 192;
const NTSC_VBLANK_LINES: usize = 37;

/// The NTSC 128-entry RGB palette. A deliberate, small duplication of
/// `rusty2600_frontend::palette::Region::Ntsc`'s table (the measured Stella
/// reference palette) — this crate cannot depend on the frontend crate (the
/// crate graph is one-directional), and PAL/SECAM aren't needed for a first
/// Android cut targeting NTSC titles.
#[allow(clippy::unreadable_literal)]
const NTSC_PALETTE: [u32; 128] = [
    0x000000, 0x4A4A4A, 0x6F6F6F, 0x8E8E8E, 0xAAAAAA, 0xC0C0C0, 0xD6D6D6, 0xECECEC, 0x484800,
    0x69690F, 0x86861D, 0xA2A22A, 0xBBBB35, 0xD2D240, 0xE8E84A, 0xFCFC54, 0x7C2C00, 0x904811,
    0xA26221, 0xB47A30, 0xC3903D, 0xD2A44A, 0xDFB755, 0xECC860, 0x901C00, 0xA33915, 0xB55328,
    0xC66C3A, 0xD5824A, 0xE39759, 0xF0AA67, 0xFCBC74, 0x940000, 0xA71A1A, 0xB83232, 0xC84848,
    0xD65C5C, 0xE46F6F, 0xF08080, 0xFC9090, 0x840064, 0x97197A, 0xA8308F, 0xB846A2, 0xC659B3,
    0xD46CC3, 0xE07CD2, 0xEC8CE0, 0x500084, 0x68199A, 0x7D30AD, 0x9246C0, 0xA459D0, 0xB56CE0,
    0xC57CEE, 0xD48CFC, 0x140090, 0x331AA3, 0x4E32B5, 0x6848C6, 0x7F5CD5, 0x956FE3, 0xA980F0,
    0xBC90FC, 0x000094, 0x181AA7, 0x2D32B8, 0x4248C8, 0x545CD6, 0x656FE4, 0x7580F0, 0x8490FC,
    0x001C88, 0x183B9D, 0x2D57B0, 0x4272C2, 0x548AD2, 0x65A0E1, 0x75B5EF, 0x84C8FC, 0x003064,
    0x185080, 0x2D6D98, 0x4288B0, 0x54A0C5, 0x65B7D9, 0x75CCEB, 0x84E0FC, 0x004030, 0x18624E,
    0x2D8169, 0x429E82, 0x54B899, 0x65D1AE, 0x75E7C2, 0x84FCD4, 0x004400, 0x1A661A, 0x328432,
    0x48A048, 0x5CBA5C, 0x6FD26F, 0x80E880, 0x90FC90, 0x143C00, 0x355F18, 0x527E2D, 0x6E9C42,
    0x87B754, 0x9ED065, 0xB4E775, 0xC8FC84, 0x303800, 0x505916, 0x6D762B, 0x88923E, 0xA0AB4F,
    0xB7C25F, 0xCCD86E, 0xE0EC7C, 0x482C00, 0x694D14, 0x866A26, 0xA28638, 0xBB9F47, 0xD2B656,
    0xE8CC63, 0xFCE070,
];

/// The DC-blocker + audio-sample-normalization state, split out of `Emu` so
/// it survives a `load_rom` without being reset every time a new ROM loads
/// (matching `emu_thread`'s own per-session, not per-ROM, DC-blocker
/// lifetime — though in practice a fresh load starting near-silent makes
/// this a minor point either way).
#[derive(Debug, Default)]
struct DcBlocker {
    x: f32,
    y: f32,
}

impl DcBlocker {
    fn process(&mut self, raw_samples: Vec<u8>) -> Vec<f32> {
        let mut out = Vec::with_capacity(raw_samples.len());
        for s in raw_samples {
            let normalized = (f32::from(s) / 15.0) - 1.0;
            let r = 0.995;
            let y = normalized - self.x + r * self.y;
            self.x = normalized;
            self.y = y;
            out.push(y);
        }
        out
    }
}

struct EmuState {
    system: Option<System>,
    rom_tag: u64,
    dc_blocker: DcBlocker,
}

/// A single running emulator instance.
///
/// `#[uniffi::export]`ed methods take `&self` (`UniFFI` objects are always
/// handed to the host behind an `Arc`), so the mutable state lives behind a
/// `Mutex` — this crate is not performance-hot itself; the actual
/// frame-stepping cost is the same `System::step_instruction` loop every
/// other frontend already pays.
#[derive(uniffi::Object)]
pub struct MobileEmulator {
    state: Mutex<EmuState>,
}

#[uniffi::export]
impl MobileEmulator {
    /// Construct a fresh, unloaded emulator instance.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(EmuState {
                system: None,
                rom_tag: 0,
                dc_blocker: DcBlocker::default(),
            }),
        }
    }

    /// Load a ROM image, detecting its bankswitch scheme via
    /// `rusty2600_cart::detect`. `rom_tag` is an opaque, host-supplied ROM
    /// identity (e.g. a hash of `bytes`) used to validate save-states
    /// against the currently-loaded ROM.
    ///
    /// # Errors
    ///
    /// Returns [`MobileError::UnrecognizedRom`] if `rusty2600_cart::detect`
    /// doesn't recognize `bytes`' bankswitch scheme.
    // `bytes` must stay `Vec<u8>` (not `&[u8]`): `UniFFI`'s generated Kotlin/
    // Swift bindings hand this method an owned byte buffer across the FFI
    // boundary, they don't have a borrowed-slice concept to hand over instead.
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_rom(&self, bytes: Vec<u8>, rom_tag: u64) -> Result<(), MobileError> {
        let board = detect(&bytes).ok_or(MobileError::UnrecognizedRom)?;
        let mut system = System::new(0);
        system.bus.board = Some(board);
        system.reset();

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.system = Some(system);
        state.rom_tag = rom_tag;
        state.dc_blocker = DcBlocker::default();
        drop(state);
        Ok(())
    }

    /// Whether a ROM is currently loaded.
    #[must_use]
    pub fn is_rom_loaded(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.system.is_some()
    }

    /// Late-latch `input` into the RIOT/TIA ports (the same convention
    /// every other frontend uses), then drive the system forward one frame
    /// (until the VSYNC 1->0 edge), returning the cropped RGBA framebuffer
    /// and this frame's drained/DC-blocked/normalized audio.
    ///
    /// # Errors
    ///
    /// Returns [`MobileError::NoRomLoaded`] if called before `load_rom`.
    // `swcha`/`swchb` are the actual TIA/RIOT register names (SWitch/SWCH A
    // and B) — renaming them to satisfy the lint would make this less
    // readable against the hardware reference, not more.
    #[allow(clippy::similar_names)]
    pub fn run_frame(&self, input: MobileInput) -> Result<FrameOutput, MobileError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Destructure the whole `EmuState` in one pattern so the borrow
        // checker treats `system`/`dc_blocker` as disjoint fields — matching
        // through `state.dc_blocker`/`state.system` separately (through the
        // `MutexGuard`'s `DerefMut`) doesn't split-borrow as cleanly.
        let EmuState {
            system, dc_blocker, ..
        } = &mut *state;
        let system = system.as_mut().ok_or(MobileError::NoRomLoaded)?;

        let (swcha, swchb) = riot_ports(&input);
        system.bus.riot.pins[0] = swcha;
        system.bus.riot.pins[1] = swchb;
        let (inpt4, inpt5) = fire_inputs(&input);
        system.bus.tia.inpt[4] = inpt4;
        system.bus.tia.inpt[5] = inpt5;

        let mut old_vsync = system.bus.tia.objects.vsync;
        for _ in 0..200_000u32 {
            system.step_instruction();
            let vsync = system.bus.tia.objects.vsync;
            if (old_vsync & 0x02 != 0) && (vsync & 0x02 == 0) {
                break;
            }
            old_vsync = vsync;
        }

        let mut rgba = vec![0u8; ATARI_W * ATARI_H * 4];
        let video = &system.bus.tia.video_buffer;
        for y in 0..ATARI_H {
            // Same VSYNC+VBLANK crop the wasm canvas-2D bootstrap and
            // `emu_thread::run_frame` both apply — the raw `video_buffer`
            // starts at scanline 0, not the first visible line.
            let sl = y + NTSC_VBLANK_LINES;
            for x in 0..ATARI_W {
                let src = sl * 160 + x;
                let color_idx = video.get(src).copied().unwrap_or(0);
                let rgb = NTSC_PALETTE[usize::from(color_idx >> 1)];
                let off = (y * ATARI_W + x) * 4;
                // NTSC_PALETTE entries are 24-bit RGB (`0x00RRGGBB`); each
                // shifted-and-masked byte below always fits in a `u8`.
                #[allow(clippy::cast_possible_truncation)]
                {
                    rgba[off] = (rgb >> 16) as u8;
                    rgba[off + 1] = (rgb >> 8) as u8;
                    rgba[off + 2] = rgb as u8;
                }
                rgba[off + 3] = 255;
            }
        }

        let raw_samples = core::mem::take(&mut system.bus.tia.audio_buffer);
        let audio_samples = dc_blocker.process(raw_samples);
        drop(state);

        Ok(FrameOutput {
            rgba,
            audio_samples,
        })
    }

    /// Capture the current system as a save-state blob (`[1.1.0]`
    /// `SaveState`, tagged with the ROM tag `load_rom` was given).
    ///
    /// # Errors
    ///
    /// Returns [`MobileError::NoRomLoaded`] if called before `load_rom`.
    pub fn save_state(&self) -> Result<Vec<u8>, MobileError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let system = state.system.as_ref().ok_or(MobileError::NoRomLoaded)?;
        Ok(SaveState::capture(system, state.rom_tag).encode())
    }

    /// Restore a save-state blob captured for the currently-loaded ROM.
    ///
    /// # Errors
    ///
    /// Returns a [`MobileError::SaveState`] if `bytes` is malformed or was
    /// captured for a different ROM (its embedded ROM tag doesn't match the
    /// tag the currently-loaded ROM was given via `load_rom`).
    // See `load_rom`'s identical rationale: `UniFFI` hands this method an
    // owned buffer across the FFI boundary, so `&[u8]` isn't an option here.
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_state(&self, bytes: Vec<u8>) -> Result<(), MobileError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let restored = SaveState::restore(&bytes, state.rom_tag)?;
        state.system = Some(restored);
        drop(state);
        Ok(())
    }
}

impl Default for MobileEmulator {
    fn default() -> Self {
        Self::new()
    }
}

// `swcha`/`swchb` are the actual TIA/RIOT register names; see `run_frame`'s
// identical rationale for keeping them as-is.
#[allow(clippy::similar_names)]
const fn riot_ports(input: &MobileInput) -> (u8, u8) {
    let swcha = (swcha_nibble(input.joystick0) << 4) | swcha_nibble(input.joystick1);
    let swchb = swchb_byte(input.switches);
    (swcha, swchb)
}

const fn swcha_nibble(j: MobileJoystick) -> u8 {
    let mut n = 0b1111u8;
    if j.up {
        n &= !0b0001;
    }
    if j.down {
        n &= !0b0010;
    }
    if j.left {
        n &= !0b0100;
    }
    if j.right {
        n &= !0b1000;
    }
    n
}

const fn swchb_byte(s: MobileSwitches) -> u8 {
    let mut b = 0b1111_1111u8;
    if s.reset {
        b &= !0b0000_0001;
    }
    if s.select {
        b &= !0b0000_0010;
    }
    if !s.color {
        b &= !0b0000_1000;
    }
    if !s.left_difficulty {
        b &= !0b0100_0000;
    }
    if !s.right_difficulty {
        b &= !0b1000_0000;
    }
    b
}

const fn fire_inputs(input: &MobileInput) -> (u8, u8) {
    const fn level(pressed: bool) -> u8 {
        if pressed { 0x00 } else { 0x80 }
    }
    (level(input.joystick0.fire), level(input.joystick1.fire))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny synthetic 4K program: `LDA $0280` (SWCHA mirror); `STA $80`
    /// (RIOT RAM); `JMP $1000` (loop) — enough to prove `run_frame` actually
    /// executes real 6507 instructions and its framebuffer changes as the
    /// TIA runs, without needing a real commercial ROM.
    fn synthetic_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x1000];
        rom[0x000] = 0xAD;
        rom[0x001] = 0x80;
        rom[0x002] = 0x02;
        rom[0x003] = 0x85;
        rom[0x004] = 0x80;
        rom[0x005] = 0x4C;
        rom[0x006] = 0x00;
        rom[0x007] = 0x10;
        rom[0xFFC] = 0x00;
        rom[0xFFD] = 0x10;
        rom
    }

    #[test]
    fn load_rejects_garbage() {
        let emu = MobileEmulator::new();
        assert!(matches!(
            emu.load_rom(vec![1, 2, 3], 0),
            Err(MobileError::UnrecognizedRom)
        ));
    }

    #[test]
    fn run_frame_without_rom_errors() {
        let emu = MobileEmulator::new();
        assert!(matches!(
            emu.run_frame(MobileInput::default()),
            Err(MobileError::NoRomLoaded)
        ));
    }

    #[test]
    fn load_run_frame_produces_correctly_sized_output() {
        let emu = MobileEmulator::new();
        emu.load_rom(synthetic_rom(), 0x00C0_FFEE).unwrap();
        assert!(emu.is_rom_loaded());
        let out = emu.run_frame(MobileInput::default()).unwrap();
        assert_eq!(out.rgba.len(), ATARI_W * ATARI_H * 4);
    }

    /// `rusty2600_cart::Cartridge` is an enum sized to its LARGEST variant
    /// (`BankF4`'s inline 32 KiB ROM array) regardless of which board is
    /// actually loaded — `System` embeds it, so every `System::clone()`
    /// (inside `SaveState::capture`/`restore`) copies a several-tens-of-KB
    /// struct. A debug-build test calling `save_state`/`load_state` a few
    /// times overflows the default ~2 MiB test-thread stack — the same real
    /// bug `rusty2600-netplay`'s rollback-desync test hit and fixed the same
    /// way. Run on an explicit larger-stack thread instead.
    fn run_on_big_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(f)
            .expect("spawning the test thread should succeed")
            .join()
            .expect("the test thread should not panic");
    }

    #[test]
    fn save_load_state_round_trips() {
        run_on_big_stack(|| {
            let emu = MobileEmulator::new();
            emu.load_rom(synthetic_rom(), 0x00C0_FFEE).unwrap();
            emu.run_frame(MobileInput::default()).unwrap();
            let blob = emu.save_state().unwrap();
            emu.run_frame(MobileInput::default()).unwrap();
            emu.load_state(blob).unwrap();
            // A no-op assertion that load_state didn't error is the real
            // test here; a stronger byte-identity check would need
            // `System: PartialEq`.
        });
    }

    #[test]
    fn load_state_rejects_wrong_rom_tag() {
        run_on_big_stack(|| {
            let emu = MobileEmulator::new();
            emu.load_rom(synthetic_rom(), 1).unwrap();
            let blob = emu.save_state().unwrap();
            let emu2 = MobileEmulator::new();
            emu2.load_rom(synthetic_rom(), 2).unwrap();
            assert!(matches!(
                emu2.load_state(blob),
                Err(MobileError::SaveState(_))
            ));
        });
    }

    #[test]
    fn swcha_nibble_matches_active_low_convention() {
        let idle = MobileJoystick::default();
        assert_eq!(swcha_nibble(idle), 0b1111);
        let up = MobileJoystick {
            up: true,
            ..Default::default()
        };
        assert_eq!(swcha_nibble(up), 0b1110);
    }
}
