//! The dedicated emulation thread + the shared-state handles.
//!
//! On native the emulator runs on its OWN thread (`emu-thread`); the winit
//! thread only does UI + present. The two communicate through:
//!
//! - `Arc<Mutex<EmuCore>>` — the [`EmuHandle`]: the winit thread takes a BRIEF
//!   lock to copy the display buffer / read debugger state, never holding it
//!   inside the egui closure.
//! - [`SharedInput`] — a lock-free input snapshot the emu-thread latches LATE
//!   (just before producing each frame), so host input has minimal latency and
//!   the determinism contract (latched input is part of the frame's inputs)
//!   holds.
//! - the [`crate::present_buffer`] producer (frames out) + the
//!   [`crate::audio_ring`] producer (samples out).
//!
//! Lifted in SHAPE from RustyNES `emu_thread.rs`. v0.1 provides the handle types
//! + the run-loop skeleton; the actual frame production is a `// TODO`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use rusty2600_core::{System, detect};

use crate::gfx::{MAX_H, MAX_W};
use crate::input::InputState;
use crate::palette::Region;
use crate::present_buffer::Producer as FrameProducer;

/// The owned emulator state the emu-thread drives and the winit thread peeks.
///
/// Holds the [`System`] (CPU + Bus + scheduler) plus the frontend-owned
/// presentation region. Rate control + run-ahead deliberately live OUTSIDE the
/// core (in [`crate::audio_ring`] / a future `runahead` module) — never here in
/// a way that perturbs synthesis.
pub struct EmuCore {
    /// The core: 6507 + Bus (TIA / RIOT / cart) + the lockstep scheduler.
    pub system: System,
    /// The broadcast region (selects palette + line count). Frontend-owned.
    pub region: Region,
    /// Whether the emulator is paused (the winit thread sets this via the menu).
    pub paused: bool,
    /// Whether a ROM is loaded (the run loop idles until one is).
    pub rom_loaded: bool,
    /// The frontend-side RGBA8 display buffer (sized to the PAL/SECAM worst case;
    /// the active sub-rect is `region.active_height()` rows tall). The present path
    /// copies it out under one brief lock without touching the core internals.
    framebuffer: Vec<u8>,
    /// The board's accuracy tier label, cached for the status bar (the 2600 `Board`
    /// trait has no name; `Tier::name` is the honesty marker).
    board_tier: Option<&'static str>,
}

impl EmuCore {
    /// Power on with a determinism seed (drives the scheduler's phase alignment —
    /// see `rusty2600_core::System::new`).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            system: System::new(seed),
            region: Region::default(),
            paused: false,
            rom_loaded: false,
            framebuffer: vec![0u8; (MAX_W * MAX_H * 4) as usize],
            board_tier: None,
        }
    }

    /// Load a raw ROM image: detect the bankswitch board and install it on the Bus.
    /// On success the system is reset to a clean power-on with the board attached.
    ///
    /// # Errors
    /// Returns an [`EmuError`] if the image is empty or no board scheme is detected.
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), EmuError> {
        if rom.is_empty() {
            return Err(EmuError::Empty);
        }
        let board = detect(rom).ok_or(EmuError::Unsupported)?;
        self.board_tier = Some(board.tier().name());
        // Fresh power-on with the board installed (a real reset-vector fetch lands
        // with the CPU model; the skeleton just attaches the board so the Bus routes
        // cart accesses to it).
        let mut system = System::new(0);
        system.bus.board = Some(board);
        self.system = system;
        self.rom_loaded = true;
        Ok(())
    }

    /// Close the loaded ROM and present a clean blank frame (the RustyNES ROM-close
    /// behavior).
    pub fn close_rom(&mut self) {
        self.system = System::new(0);
        self.rom_loaded = false;
        self.board_tier = None;
        self.framebuffer.iter_mut().for_each(|b| *b = 0);
    }

    /// The loaded board's accuracy-tier label, if any (for the status bar).
    #[must_use]
    pub const fn board_tier(&self) -> Option<&'static str> {
        self.board_tier
    }

    /// The active display dimensions `(w, h)` for the current region.
    #[must_use]
    pub const fn fb_dims(&self) -> (u32, u32) {
        (crate::gfx::VCS_W, self.region.active_height())
    }

    /// The current RGBA8 framebuffer slice (the active region's `w*h*4` bytes), for
    /// the present path. The caller copies this under the brief emu lock, then drops
    /// the lock before rendering.
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        let (w, h) = self.fb_dims();
        let len = (w * h * 4) as usize;
        &self.framebuffer[..len.min(self.framebuffer.len())]
    }

    /// Advance one video frame (run the scheduler a region's worth of color clocks).
    /// The synchronous (non-threaded) present path calls this each frame.
    ///
    /// v0.1: the chips are skeletons, so this advances the timebase but the TIA emits
    /// no pixels yet — the framebuffer stays cleared (no TIA output).
    pub fn run_frame(&mut self) {
        if !self.rom_loaded || self.paused {
            return;
        }
        let clocks = u64::from(self.region.lines_per_frame()) * 228;
        for _ in 0..clocks {
            self.system.tick_one_color_clock();
        }
        // TODO(T-PS-061): write each visible dot through `present_buffer` (TIA emit ->
        // palette RGB) into `framebuffer`, and push the TIA audio sample per CPU cycle.
    }

    /// Produce exactly one frame's worth of color clocks, accumulating beam dots into
    /// `frames` (the dedicated-emu-thread path). v0.1 delegates to [`EmuCore::run_frame`];
    /// the per-dot accumulation into `frames` is a `// TODO`.
    pub fn step_frame(&mut self, _frames: &FrameProducer) {
        self.run_frame();
    }
}

/// ROM-load / emulation errors surfaced to the UI.
#[derive(Debug, thiserror::Error)]
pub enum EmuError {
    /// The ROM image was empty.
    #[error("empty ROM image")]
    Empty,
    /// No bankswitch scheme was detected for the image size.
    #[error("unsupported / undetected 2600 ROM (no board scheme matched)")]
    Unsupported,
}

/// The shared handle: a clonable `Arc<Mutex<EmuCore>>`.
///
/// The winit thread takes a BRIEF lock (copy the display buffer, read debugger
/// state) and ALWAYS drops it before entering the egui closure — never holds the
/// emu lock across UI work.
pub type EmuHandle = Arc<Mutex<EmuCore>>;

/// A lock-free input snapshot the emu-thread latches each frame.
///
/// The winit thread writes the latest host input; the emu-thread reads it late
/// (just before producing a frame). v0.1 packs the [`InputState`] into the two
/// RIOT port bytes + the two fire inputs (4 bytes) behind a single `AtomicU32`,
/// so the read is wait-free without a mutex.
///
/// TODO(T-PS-062): widen to carry the analog paddle bytes too (a second atomic).
#[derive(Debug, Default)]
pub struct SharedInput {
    /// Packed `[SWCHA, SWCHB, INPT4, INPT5]` — the emu-thread unpacks this.
    packed: AtomicU32,
}

impl SharedInput {
    /// Construct an idle snapshot (all inputs released).
    #[must_use]
    pub fn new() -> Self {
        let s = Self::default();
        s.store(InputState::default());
        s
    }

    /// Store the latest host input (winit thread).
    pub fn store(&self, state: InputState) {
        let (swcha, swchb) = state.riot_ports();
        let (inpt4, inpt5) = state.fire_inputs();
        let packed = u32::from(swcha) << 24
            | u32::from(swchb) << 16
            | u32::from(inpt4) << 8
            | u32::from(inpt5);
        self.packed.store(packed, Ordering::Relaxed);
    }

    /// Read the latched `(SWCHA, SWCHB, INPT4, INPT5)` bytes (emu-thread).
    #[must_use]
    pub fn load_ports(&self) -> (u8, u8, u8, u8) {
        let p = self.packed.load(Ordering::Relaxed);
        let byte = |shift: u32| ((p >> shift) & 0xFF) as u8;
        (byte(24), byte(16), byte(8), byte(0))
    }
}

/// Drive one cooperative emulation pass (the body the dedicated emu-thread will loop).
///
/// v0.1 runs it INLINE rather than on a real OS thread: `rusty2600_core::System` is
/// not yet `Send` (it boxes a `dyn Board` without a `Send` bound), so
/// `std::thread::spawn` would not accept the [`EmuHandle`].
///
/// Latches `input` late, then produces one frame into `frames`. This keeps the
/// SharedInput / EmuHandle / FrameProducer wiring real and exercised, so the
/// real thread spawn is a localized change once the core gains the `Send` bound.
///
/// TODO(T-PS-063): add a `Send` bound to `cart::Board`, then move this body into
/// a named `emu-thread` (`std::thread::Builder`) driven by a produce-interval
/// pacing timer + best-effort per-thread priority elevation (RustyNES
/// `elevate_thread_priority`) + a clean shutdown flag.
pub fn drive_one_pass(handle: &EmuHandle, input: &SharedInput, frames: &FrameProducer) {
    // Late-latch the input the winit thread published.
    let (swcha, swchb, _inpt4, _inpt5) = input.load_ports();
    let _ = (swcha, swchb);
    if let Ok(mut core) = handle.lock() {
        core.step_frame(frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_input_roundtrips_idle() {
        let si = SharedInput::new();
        let (swcha, swchb, i4, i5) = si.load_ports();
        assert_eq!(swcha, 0xFF);
        assert_eq!(i4, 0x80);
        assert_eq!(i5, 0x80);
        let _ = swchb;
    }

    #[test]
    fn shared_input_reflects_a_press() {
        let si = SharedInput::new();
        let mut st = InputState::default();
        st.joysticks[0].fire = true;
        si.store(st);
        assert_eq!(si.load_ports().2, 0x00, "P0 fire => INPT4 active-low");
    }

    #[test]
    fn emu_core_advances_timebase_when_running() {
        let (tx, _rx) = crate::present_buffer::channel();
        let mut core = EmuCore::new(0);
        core.rom_loaded = true;
        let before = core.system.color_clocks();
        core.step_frame(&tx);
        assert!(core.system.color_clocks() > before);
    }
}
