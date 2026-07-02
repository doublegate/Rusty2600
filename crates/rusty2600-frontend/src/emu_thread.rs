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

use rusty2600_core::{SaveState, System, detect};

use crate::audio::AudioProducer;
use crate::gfx::{MAX_H, MAX_W};
use crate::input::InputState;
use crate::palette::Region;
use crate::present_buffer::Producer as FrameProducer;
use rusty2600_core::Board;

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
    /// The lock-free audio ring producer (pushes samples to the frontend).
    pub audio_tx: Option<AudioProducer>,
    /// Serialized snapshots of the system state (via [`SaveState`]), stored before
    /// each frame is executed. Used for rewind/run-ahead. Maintains ~600 frames
    /// (10 seconds) of history.
    ///
    /// Kept as encoded bytes rather than raw `System` clones: `Cartridge`'s enum
    /// size is pinned to its largest fixed-size variant (`BankF4`'s 32 KiB ROM
    /// array) regardless of which board is actually loaded, so a raw `.clone()`
    /// pays that cost for every game. Serializing through the real data shrinks
    /// a 2K/4K-cart entry to its true size (see `docs/adr/0007-save-state-versioning.md`).
    pub snapshots: std::collections::VecDeque<Vec<u8>>,
    /// Suppresses rewind capture, set by [`crate::runahead`] around its
    /// hidden/speculative frames.
    ///
    /// While true, [`Self::step_frame`] does not push onto
    /// [`Self::snapshots`] — a speculative frame never actually happened on
    /// the canonical timeline, so it must never enter rewind history.
    pub rewind_capture_suppressed: bool,
    /// Suppresses audio output, set by [`crate::runahead`] around its
    /// persistent + hidden frames.
    ///
    /// While true, [`Self::step_frame`] still drains the TIA's audio buffer
    /// (so it can't leak into a later real call) but skips the DC-blocker
    /// processing and the push to `audio_tx` — only the one frame the user
    /// actually sees/hears (the final speculative frame) should reach the
    /// audio device, or run-ahead would emit N frames of audio per real
    /// ~16.67 ms tick and drift out of sync.
    pub audio_output_suppressed: bool,
    /// Frames completed since power-on/ROM-load — a `frame` term the
    /// debugger's watch-expression engine (`crate::debugger::expr`) can
    /// reference, and a natural counter for future TAS/movie work.
    pub frame_count: u64,
    /// State for the high-pass DC blocker.
    dc_blocker_x: f32,
    dc_blocker_y: f32,
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
            audio_tx: None,
            snapshots: std::collections::VecDeque::with_capacity(600),
            rewind_capture_suppressed: false,
            audio_output_suppressed: false,
            frame_count: 0,
            dc_blocker_x: 0.0,
            dc_blocker_y: 0.0,
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
        system.reset();
        self.system = system;
        self.snapshots.clear();
        self.frame_count = 0;
        self.rom_loaded = true;
        Ok(())
    }

    /// Close the loaded ROM and present a clean blank frame (the RustyNES ROM-close
    /// behavior).
    pub fn close_rom(&mut self) {
        self.system = System::new(0);
        self.snapshots.clear();
        self.frame_count = 0;
        self.rom_loaded = false;
        self.board_tier = None;
        self.framebuffer.iter_mut().for_each(|b| *b = 0);
    }

    /// Rewinds the emulator state by one frame if history is available.
    ///
    /// A corrupt/malformed entry (should be unreachable — these are our own
    /// just-encoded bytes) is treated as "no history available" rather than
    /// panicking, matching the rest of the codebase's never-`unwrap`-on-data
    /// convention.
    pub fn rewind(&mut self) {
        if let Some(bytes) = self.snapshots.pop_back()
            && let Ok(state) = SaveState::decode(&bytes)
        {
            self.system = state.into_system();
        }
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

    /// Run a single frame sequentially and accumulate the beam dots into `self.framebuffer`.
    // Byte-packing casts (RGB channel extraction, sample normalization, scanline-count
    // narrowing from a region constant that's always < 256) are inherently truncating /
    // sign-dropping by design, not bugs.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_lossless
    )]
    pub fn run_frame(&mut self, input: Option<InputState>) {
        if let Some(state) = input {
            let (swcha, swchb) = state.riot_ports();
            self.system.bus.riot.pins[0] = swcha;
            self.system.bus.riot.pins[1] = swchb;
            let (inpt4, inpt5) = state.fire_inputs();
            self.system.bus.tia.inpt[4] = inpt4;
            self.system.bus.tia.inpt[5] = inpt5;
            for (i, paddle) in state.paddles.iter().enumerate() {
                self.system.bus.tia.set_paddle_position(i, paddle.position);
            }
        }

        if !self.rom_loaded || self.paused {
            return;
        }

        // Save snapshot before advancing state
        self.snapshots
            .push_back(SaveState::capture(&self.system, 0).encode());
        if self.snapshots.len() > 600 {
            self.snapshots.pop_front();
        }

        let vblank_lines = match self.region {
            crate::palette::Region::Ntsc => 37,
            _ => 42, // PAL/SECAM standard vblank
        };
        let active_h = self.region.active_height() as u16;

        let mut old_vsync = self.system.bus.tia.objects.vsync;
        let mut instructions = 0;

        // Drive the CPU instruction-by-instruction (see
        // `rusty2600-core::scheduler`'s module doc comment for why this
        // replaced the old per-color-clock-tick loop): each call advances the
        // TIA/RIOT/cart in lockstep internally, cycle by cycle, as the
        // instruction executes. The TIA accumulates its own video/audio dots
        // into `video_buffer`/`audio_buffer` as it goes (it has to — a single
        // instruction can span many color clocks, so this outer loop can no
        // longer sample one dot per iteration the way it used to).
        loop {
            self.system.step_instruction();
            instructions += 1;

            let vsync = self.system.bus.tia.objects.vsync;
            // Frame boundary: when VSYNC transitions from 1 -> 0.
            if (old_vsync & 0x02 != 0) && (vsync & 0x02 == 0) {
                break;
            }
            old_vsync = vsync;

            // Safety timeout in case a game hangs and stops asserting VSYNC.
            if instructions > 200_000 {
                break;
            }
        }

        // Drain this frame's audio samples (DC-blocked + normalized) to the ring.
        let samples = core::mem::take(&mut self.system.bus.tia.audio_buffer);
        if !samples.is_empty() {
            let mut out = Vec::with_capacity(samples.len());
            for s in samples {
                // Map [0, 30] to [-1.0, 1.0].
                let normalized = (s as f32 / 15.0) - 1.0;
                // 1-pole high-pass DC blocker: removes the massive -1.0 offset
                // during TIA silence (s = 0).
                let r = 0.995;
                let y = normalized - self.dc_blocker_x + r * self.dc_blocker_y;
                self.dc_blocker_x = normalized;
                self.dc_blocker_y = y;
                out.push(y);
            }
            if let Some(tx) = &mut self.audio_tx {
                tx.push_samples(&out);
            }
        }

        // Crop the TIA's accumulated video buffer down to the active window
        // and convert color indices to RGBA8 for the present path.
        let video = &self.system.bus.tia.video_buffer;
        for y in 0..active_h as usize {
            let sl = y + vblank_lines as usize;
            for x in 0..160usize {
                let src = sl * 160 + x;
                let color_idx = video.get(src).copied().unwrap_or(0);
                let rgb = self.region.lookup(color_idx >> 1);
                let off = (y * 160 + x) * 4;
                if off + 3 < self.framebuffer.len() {
                    self.framebuffer[off] = (rgb >> 16) as u8;
                    self.framebuffer[off + 1] = (rgb >> 8) as u8;
                    self.framebuffer[off + 2] = rgb as u8;
                    self.framebuffer[off + 3] = 255;
                }
            }
        }

        self.frame_count = self.frame_count.wrapping_add(1);
    }

    /// Extract this frame's presentation output (framebuffer crop + drained,
    /// DC-blocked audio) from `self.system`'s CURRENT state, without
    /// stepping the CPU.
    ///
    /// For a caller (rollback netplay's frontend wiring, `netplay` feature)
    /// that already advanced `self.system` forward by its own means (e.g.
    /// `rusty2600_netplay::RollbackSession::advance_frame`, which owns and
    /// steps its own `System` internally) and only needs the same video/
    /// audio post-processing [`Self::run_frame`] performs internally after
    /// its own stepping loop. A small, deliberate duplication of
    /// `run_frame`'s tail (crop + audio-drain) rather than refactoring
    /// `run_frame` itself to share a common tail method — kept additive to
    /// avoid touching `run_frame`'s/`step_frame`'s existing bodies.
    #[cfg(feature = "netplay")]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_lossless
    )]
    pub fn extract_frame(&mut self) {
        let vblank_lines = match self.region {
            crate::palette::Region::Ntsc => 37,
            _ => 42,
        };
        let active_h = self.region.active_height() as u16;

        let samples = core::mem::take(&mut self.system.bus.tia.audio_buffer);
        if !samples.is_empty() {
            let mut out = Vec::with_capacity(samples.len());
            for s in samples {
                let normalized = (s as f32 / 15.0) - 1.0;
                let r = 0.995;
                let y = normalized - self.dc_blocker_x + r * self.dc_blocker_y;
                self.dc_blocker_x = normalized;
                self.dc_blocker_y = y;
                out.push(y);
            }
            if let Some(tx) = &mut self.audio_tx {
                tx.push_samples(&out);
            }
        }

        let video = &self.system.bus.tia.video_buffer;
        for y in 0..active_h as usize {
            let sl = y + vblank_lines as usize;
            for x in 0..160usize {
                let src = sl * 160 + x;
                let color_idx = video.get(src).copied().unwrap_or(0);
                let rgb = self.region.lookup(color_idx >> 1);
                let off = (y * 160 + x) * 4;
                if off + 3 < self.framebuffer.len() {
                    self.framebuffer[off] = (rgb >> 16) as u8;
                    self.framebuffer[off + 1] = (rgb >> 8) as u8;
                    self.framebuffer[off + 2] = rgb as u8;
                    self.framebuffer[off + 3] = 255;
                }
            }
        }

        self.frame_count = self.frame_count.wrapping_add(1);
    }

    /// Produce exactly one frame's worth of color clocks, accumulating beam dots into
    /// `frames` (the dedicated-emu-thread path).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_lossless
    )]
    pub fn step_frame(&mut self, frames: &FrameProducer, input: Option<(u8, u8, u8, u8, [u8; 4])>) {
        if let Some((swcha, swchb, inpt4, inpt5, paddles)) = input {
            self.system.bus.riot.pins[0] = swcha;
            self.system.bus.riot.pins[1] = swchb;
            self.system.bus.tia.inpt[4] = inpt4;
            self.system.bus.tia.inpt[5] = inpt5;
            for (i, position) in paddles.into_iter().enumerate() {
                self.system.bus.tia.set_paddle_position(i, position);
            }
        }

        if !self.rom_loaded || self.paused {
            return;
        }

        // Save snapshot before advancing state — unless this is one of
        // run-ahead's hidden/speculative frames, which never happened on the
        // canonical timeline and must never enter rewind history.
        if !self.rewind_capture_suppressed {
            self.snapshots
                .push_back(SaveState::capture(&self.system, 0).encode());
            if self.snapshots.len() > 600 {
                self.snapshots.pop_front();
            }
        }

        let mut frame = crate::present_buffer::Frame::black(
            crate::gfx::VCS_W as usize,
            self.region.active_height() as usize,
        );

        let vblank_lines = match self.region {
            crate::palette::Region::Ntsc => 37,
            _ => 42, // PAL/SECAM standard vblank
        };
        let active_h = self.region.active_height() as u16;

        let mut old_vsync = self.system.bus.tia.objects.vsync;
        let mut instructions = 0;

        // See `run_frame`'s doc comment: the CPU drives its own ticking now,
        // so video/audio dots are accumulated by the TIA itself and read back
        // once per frame instead of sampled per color clock from this loop.
        loop {
            self.system.step_instruction();
            instructions += 1;

            let vsync = self.system.bus.tia.objects.vsync;
            // Frame boundary: when VSYNC transitions from 1 -> 0.
            if (old_vsync & 0x02 != 0) && (vsync & 0x02 == 0) {
                break;
            }
            old_vsync = vsync;

            // Safety timeout in case a game hangs and stops asserting VSYNC.
            if instructions > 200_000 {
                break;
            }
        }

        // Always drain the TIA's audio buffer (it must never carry over into
        // the next call), but skip the DC-blocker processing and the push to
        // `audio_tx` for run-ahead's persistent/hidden frames: only the one
        // frame actually presented to the user should ever reach the audio
        // device, or run-ahead would emit multiple frames of audio per real
        // ~16.67 ms tick and drift out of sync (see `audio_output_suppressed`).
        let samples = core::mem::take(&mut self.system.bus.tia.audio_buffer);
        if !self.audio_output_suppressed {
            for s in samples {
                let normalized = (s as f32 / 15.0) - 1.0;
                // DC blocker (1-pole high-pass filter) to remove the massive -1.0 DC offset
                // when TIA outputs silence (s = 0), preventing audio hums or clicks.
                let r = 0.995;
                let y = normalized - self.dc_blocker_x + r * self.dc_blocker_y;
                self.dc_blocker_x = normalized;
                self.dc_blocker_y = y;

                if let Some(tx) = &mut self.audio_tx {
                    tx.push_samples(&[y]);
                }
            }
        }

        let video = &self.system.bus.tia.video_buffer;
        for y in 0..active_h as usize {
            let sl = y + vblank_lines as usize;
            for x in 0..160usize {
                let src = sl * 160 + x;
                let color_idx = video.get(src).copied().unwrap_or(0);
                let rgb = self.region.lookup(color_idx >> 1);
                frame.put_dot(x, y, rgb);
            }
        }

        self.frame_count = self.frame_count.wrapping_add(1);

        // Publish the produced frame
        frames.publish(frame);
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
/// so the read is wait-free without a mutex. A follow-up (`T-0501-010`) added a
/// second `AtomicU32` carrying the four paddle position bytes the same way.
#[derive(Debug, Default)]
pub struct SharedInput {
    /// Packed `[SWCHA, SWCHB, INPT4, INPT5]` — the emu-thread unpacks this.
    packed: AtomicU32,
    /// Packed `[paddle0, paddle1, paddle2, paddle3]` positions (`0..=255`
    /// each) — kept in a second atomic rather than widening `packed` past
    /// 32 bits, matching this struct's own established wait-free-without-a-
    /// mutex convention.
    paddles: AtomicU32,
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
        let packed = (u32::from(swcha) << 24)
            | (u32::from(swchb) << 16)
            | (u32::from(inpt4) << 8)
            | u32::from(inpt5);
        self.packed.store(packed, Ordering::Release);

        let p = state.paddles;
        let packed_paddles = (u32::from(p[0].position) << 24)
            | (u32::from(p[1].position) << 16)
            | (u32::from(p[2].position) << 8)
            | u32::from(p[3].position);
        self.paddles.store(packed_paddles, Ordering::Release);
    }

    /// Load the latched host input (emu thread): `(SWCHA, SWCHB, INPT4,
    /// INPT5, [paddle0..=paddle3 positions])`.
    // `packed` is a deliberately bit-packed u32 (4 bytes via SWCHA/SWCHB/INPT4/INPT5 shifts);
    // truncating `as u8` here intentionally takes the low byte of each known field, not a bug.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn load(&self) -> (u8, u8, u8, u8, [u8; 4]) {
        let packed = self.packed.load(Ordering::Acquire);
        let swcha = (packed >> 24) as u8;
        let swchb = (packed >> 16) as u8;
        let inpt4 = (packed >> 8) as u8;
        let inpt5 = packed as u8;

        let pp = self.paddles.load(Ordering::Acquire);
        let paddles = [
            (pp >> 24) as u8,
            (pp >> 16) as u8,
            (pp >> 8) as u8,
            pp as u8,
        ];

        (swcha, swchb, inpt4, inpt5, paddles)
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
    fn shared_input_roundtrips_paddle_positions() {
        let si = SharedInput::new();
        let mut st = InputState::default();
        st.paddles[0].position = 10;
        st.paddles[1].position = 20;
        st.paddles[2].position = 200;
        st.paddles[3].position = 255;
        si.store(st);
        let (.., paddles) = si.load();
        assert_eq!(paddles, [10, 20, 200, 255]);
    }

    #[test]
    fn emu_core_advances_timebase_when_running() {
        let (tx, _rx) = crate::present_buffer::channel();
        let mut core = EmuCore::new(0);
        core.rom_loaded = true;
        let before = core.system.color_clocks();
        core.step_frame(&tx, None);
        assert!(core.system.color_clocks() > before);
    }

    #[test]
    fn rewind_restores_a_prior_color_clock_count() {
        let (tx, _rx) = crate::present_buffer::channel();
        let mut core = EmuCore::new(0);
        core.rom_loaded = true;
        core.step_frame(&tx, None);
        let before_second_frame = core.system.color_clocks();
        core.step_frame(&tx, None);
        assert!(core.system.color_clocks() > before_second_frame);

        core.rewind();
        assert_eq!(core.system.color_clocks(), before_second_frame);
    }

    #[test]
    fn rewind_with_no_history_is_a_no_op() {
        let mut core = EmuCore::new(0);
        let before = core.system.color_clocks();
        core.rewind();
        assert_eq!(core.system.color_clocks(), before);
    }
}
