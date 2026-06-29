//! `rusty2600-tia` — the TIA (Television Interface Adaptor), the VCS's
//! video **and** audio chip.
//!
//! There is **no framebuffer**: the TIA races the electron beam, generating one
//! pixel of luminance/colour per *color clock* directly from the object
//! registers, and the program rewrites those registers mid-scanline ("racing the
//! beam") to compose the picture. A scanline is 228 color clocks (68 HBLANK + 160
//! visible). The TIA also drives `RDY` low when the CPU writes `WSYNC`, stalling
//! the CPU until the start of the next scanline — that beam-stall is exposed to
//! the scheduler, which owns the actual CPU freeze.
//!
//! Audio lives **here**, not in the RIOT: the two-channel square/noise generator
//! (`AUDC`/`AUDF`/`AUDV` × 2) is part of the TIA silicon. See the [`audio`]
//! submodule.
//!
//! Part of the one-directional chip-crate graph (see `docs/architecture.md`):
//! this crate depends ONLY on `rusty2600-cart` (its memory bus). `#![no_std]` +
//! `alloc` for bare-metal cross-compile; only the frontend carries `std` +
//! `unsafe`.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

pub mod audio;

/// TIA write-register strobe addresses the program uses to compose a frame.
/// (Subset — the full map is in `docs/tia.md`; these anchor the stub.)
pub mod regs {
    /// `VSYNC` — vertical sync set/clear.
    pub const VSYNC: u8 = 0x00;
    /// `VBLANK` — vertical blank set/clear.
    pub const VBLANK: u8 = 0x01;
    /// `WSYNC` — strobe: halt the CPU (RDY low) until the next scanline.
    pub const WSYNC: u8 = 0x02;
    /// `RSYNC` — reset the horizontal color-clock counter.
    pub const RSYNC: u8 = 0x03;
    /// `HMOVE` — apply the horizontal-motion (`HMxx`) registers to objects.
    pub const HMOVE: u8 = 0x2A;
}

/// The player/missile/ball/playfield object registers + colours.
///
/// Stub: fields are real, the beam renderer is a `// TODO`. Pin against the TIA
/// test ROMs FIRST (test-ROM-is-spec), then implement until they pass.
#[derive(Debug, Default, Clone)]
pub struct Objects {
    /// Playfield, 20 bits across `PF0` (high nibble), `PF1`, `PF2`.
    pub pf: u32,
    /// Player 0 / player 1 graphics (`GRP0`/`GRP1`).
    pub grp: [u8; 2],
    /// Number-size / copy register per player (`NUSIZ0`/`NUSIZ1`).
    pub nusiz: [u8; 2],
    /// Object horizontal positions (color-clock 0..=159): P0, P1, M0, M1, ball.
    pub pos: [u8; 5],
    /// Horizontal-motion registers (`HMP0..HMBL`), 4-bit signed, applied on `HMOVE`.
    pub hm: [i8; 5],
    /// Player/playfield/background colours: `COLUP0`, `COLUP1`, `COLUPF`, `COLUBK`.
    pub colu: [u8; 4],
}

/// TIA state: object registers, beam position, the audio unit, and the RDY
/// beam-stall signal the scheduler reads.
#[derive(Debug, Default, Clone)]
pub struct Tia {
    /// Visible/blanking object + colour registers.
    pub objects: Objects,
    /// Two-channel audio generator (lives in the TIA, not the RIOT).
    pub audio: audio::Audio,
    /// Current color clock within the scanline (`0..228`).
    pub color_clock: u16,
    /// Current scanline since the last `VSYNC`.
    pub scanline: u16,
    /// `RDY` line: when `true` the CPU is stalled (set by `WSYNC`, cleared at
    /// HBLANK end). The scheduler reads this to freeze the CPU.
    rdy_stall: bool,
    // TODO(T-PS-020): playfield/player/missile/ball priority + collision latches.
    // TODO(T-PS-021): the emitted (luma, chroma) for the current visible dot.
}

impl Tia {
    /// Construct at power-on. Beam phase alignment, where applicable, comes from
    /// a *seeded* PRNG in the owning `System` (determinism contract — see
    /// `docs/adr/0004`), never the OS RNG.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// CPU write to a TIA register (the program's beam-racing path). `WSYNC`
    /// asserts the RDY stall here.
    // Not `const`: the stub only handles WSYNC; the real decode mutates the
    // object/colour/audio registers, so const-ness would have to be reverted.
    #[allow(clippy::missing_const_for_fn)]
    pub fn write_register(&mut self, reg: u8, val: u8) {
        if reg == regs::WSYNC {
            self.rdy_stall = true;
        }
        // TODO(T-PS-022): decode the full TIA write-register map (objects,
        // colours, HMxx, strobes, and the AUDx audio registers → self.audio).
        let _ = val;
    }

    /// Advance the beam one color clock — the TIA's atomic time step. The
    /// scheduler calls this once per master tick; the CPU steps every third one.
    /// Hot path (the hottest loop in the system): allocation-free.
    pub fn tick_color_clock(&mut self) {
        // TODO(T-PS-023): emit one dot from the object registers, advance the
        // beam counters, and release RDY at the end of HBLANK.
        self.color_clock += 1;
        if self.color_clock >= 228 {
            self.color_clock = 0;
            self.scanline += 1;
            self.rdy_stall = false; // HBLANK over → CPU resumes.
        }
        self.audio.tick();
    }

    /// Whether the TIA is currently holding the CPU via `RDY` (the `WSYNC`
    /// beam-stall). The scheduler reads this every CPU phase to decide whether
    /// to step the CPU. See `docs/scheduler.md`.
    #[must_use]
    pub const fn rdy_stall(&self) -> bool {
        self.rdy_stall
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs() {
        let _ = Tia::new();
    }

    #[test]
    fn wsync_sets_and_hblank_clears_rdy() {
        let mut tia = Tia::new();
        tia.write_register(regs::WSYNC, 0);
        assert!(tia.rdy_stall());
        // Run to the end of the scanline; RDY releases.
        for _ in 0..228 {
            tia.tick_color_clock();
        }
        assert!(!tia.rdy_stall());
    }
}
