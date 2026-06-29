//! The master-clock lockstep scheduler — the heart of the emulator.
//!
//! **Timing master: the TIA color clock @ 3.579545 MHz (NTSC).** Resolution is
//! integer color clocks. `tick_one_color_clock()` advances the TIA exactly one
//! color clock, and the 6507 CPU advances on **every third** color clock (the
//! VCS divides the color clock by 3 to make the CPU's φ0). This is the closest
//! port of `RustyNES`'s PPU-dot `tick_one_dot()`: LOCKSTEP, not catch-up — which
//! is why a mid-scanline TIA register write composes the very next dot without
//! any per-quirk patch.
//!
//! **The WSYNC / RDY beam-stall.** When the program strobes `WSYNC`, the TIA
//! pulls `RDY` low, and the 6507 freezes on its current cycle until the TIA
//! releases RDY at the end of HBLANK (the start of the next scanline). The TIA
//! owns the signal (`Tia::rdy_stall`); the scheduler reads it and simply skips
//! the CPU step while it is asserted. The color clock keeps advancing — only the
//! CPU is frozen.
//!
//! Determinism contract: same seed + ROM + input ⇒ bit-identical AV. The
//! per-power-on CPU/color-clock phase alignment comes from a SEEDED PRNG (never
//! the OS RNG). See `docs/scheduler.md`.

use rusty2600_cpu::{Cpu, CpuBus};

use crate::bus::Bus;

/// Color clocks per CPU cycle (the VCS divides the 3.58 MHz color clock by 3).
const CPU_DIVISOR: u8 = 3;

/// Adapts the [`Bus`] to the CPU's narrow [`CpuBus`] view for the duration of a
/// CPU step. Keeps the CPU crate free of any console-specific bus type.
struct CpuView<'a>(&'a mut Bus);

impl CpuBus for CpuView<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        self.0.cpu_read(addr)
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.0.cpu_write(addr, val);
    }
}

/// Owns the run loop and the lockstep timebase.
#[derive(Debug)]
pub struct System {
    /// The 6507 CPU.
    pub cpu: Cpu,
    /// The Bus — owns the TIA, RIOT, cart-via-board, controllers, open bus.
    pub bus: Bus,
    /// Per-power-on CPU/color-clock phase alignment, from a SEEDED PRNG (never
    /// the OS RNG). Selects which of the three color clocks carries the CPU
    /// step, so power-on alignment is deterministic per seed.
    phase: u8,
    /// Total color clocks elapsed since power-on.
    color_clocks: u64,
}

impl System {
    /// Power on with a determinism seed (drives the phase alignment).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(),
            // `seed % CPU_DIVISOR` is in `0..3`, so the narrowing cannot truncate.
            phase: u8::try_from(seed % u64::from(CPU_DIVISOR)).unwrap_or(0),
            color_clocks: 0,
        }
    }

    /// Advance exactly one TIA color clock, stepping the CPU on every third one
    /// — unless the TIA is holding `RDY` (the `WSYNC` beam-stall), in which case
    /// the CPU stays frozen while the color clock keeps running. The RIOT timer
    /// advances on the CPU's cycle, and the cart coprocessor (DPC-style) ticks
    /// alongside.
    ///
    /// Hot path — the hottest loop in the system (runs at 3.58 MHz):
    /// allocation-free.
    pub fn tick_one_color_clock(&mut self) {
        self.bus.tia.tick_color_clock();

        // The CPU's φ0 fires once per `CPU_DIVISOR` color clocks, offset by the
        // seeded power-on phase.
        if (self.color_clocks % u64::from(CPU_DIVISOR)) == u64::from(self.phase) {
            if self.bus.tia.rdy_stall() {
                // RDY held: the CPU is frozen this cycle (WSYNC beam-stall).
                // TODO(T-PS-061): model the exact mid-instruction freeze point.
            } else {
                let mut view = CpuView(&mut self.bus);
                self.cpu.tick(&mut view);
                self.bus.riot.tick();
                if let Some(board) = self.bus.board.as_mut() {
                    board.tick();
                }
            }
        }

        self.color_clocks = self.color_clocks.wrapping_add(1);
    }

    /// Total color clocks since power-on (for tracing / the golden-log differ).
    #[must_use]
    pub const fn color_clocks(&self) -> u64 {
        self.color_clocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_phase_is_deterministic() {
        let a = System::new(42);
        let b = System::new(42);
        assert_eq!(a.phase, b.phase);
    }

    #[test]
    fn color_clock_advances() {
        let mut sys = System::new(0);
        sys.tick_one_color_clock();
        assert_eq!(sys.color_clocks(), 1);
    }
}
