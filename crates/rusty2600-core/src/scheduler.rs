//! The master-clock lockstep scheduler — the heart of the emulator.
//!
//! **Timing master: the TIA color clock @ 3.579545 MHz (NTSC).** The 6507 CPU
//! advances on **every third** color clock (the VCS divides the color clock by
//! 3 to make the CPU's φ0) — LOCKSTEP, not catch-up: a mid-instruction TIA
//! register write takes effect at the exact color clock it's written on, not
//! after the whole instruction "finishes."
//!
//! The CPU drives this directly: [`System::step_instruction`] runs one full
//! 6507 instruction via [`rusty2600_cpu::Cpu::step`], and every cycle THAT
//! instruction consumes calls back into [`CpuView::tick_cycle`] (via
//! [`rusty2600_cpu::CpuBus::tick_cycle`]) to advance the TIA by exactly 3
//! color clocks, the RIOT by one tick, and the cart coprocessor by one tick —
//! cycle by cycle as the instruction executes, not all at once when it
//! returns. (An earlier version of this scheduler called `cpu.tick()` once per
//! *color clock* expecting it to consume exactly one CPU cycle, but the CPU
//! actually ran the whole instruction synchronously per call — racing far
//! ahead of the color clock it was supposed to be locked to. That mismatch is
//! why object-positioning routines, which time themselves against the color
//! clock via `WSYNC` + a cycle-counted delay, never reached realistic
//! positions. See the regression test below.)
//!
//! **The WSYNC / RDY beam-stall.** When the program strobes `WSYNC`, the TIA
//! pulls `RDY` low, and the 6507 freezes BEFORE its next cycle (typically the
//! next instruction's opcode fetch) until the TIA releases RDY at the end of
//! HBLANK. The TIA owns the signal (`Tia::rdy_stall`); [`CpuView::rdy_stall`]
//! exposes it to the CPU crate, which spins on [`CpuView::tick_cycle`] while
//! it's asserted — the color clock (and RIOT/cart) keep advancing, only the
//! CPU is frozen.
//!
//! Determinism contract: same seed + ROM + input ⇒ bit-identical AV. The
//! per-power-on CPU/color-clock phase alignment comes from a SEEDED PRNG
//! (never the OS RNG) applied as a one-time 0..3 extra-color-clock offset
//! before the CPU's first cycle. See `docs/scheduler.md`.

use rusty2600_cart::Board;
use rusty2600_cpu::{Cpu, CpuBus};

use crate::bus::Bus;

/// Color clocks per CPU cycle (the VCS divides the 3.58 MHz color clock by 3).
const CPU_DIVISOR: u8 = 3;

/// Adapts the [`Bus`] to the CPU's narrow [`CpuBus`] view for the duration of a
/// CPU step. Keeps the CPU crate free of any console-specific bus type. Also
/// carries a `&mut` to the owning [`System`]'s running color-clock counter so
/// [`Self::tick_cycle`] can keep it accurate without `CpuView` owning it.
struct CpuView<'a> {
    bus: &'a mut Bus,
    color_clocks: &'a mut u64,
}

impl CpuBus for CpuView<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        self.bus.cpu_read(addr)
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.bus.cpu_write(addr, val);
    }

    fn tick_cycle(&mut self) {
        self.bus.tia.tick_color_clock();
        self.bus.tia.tick_color_clock();
        self.bus.tia.tick_color_clock();
        self.bus.riot.tick();
        if let Some(board) = self.bus.board.as_mut() {
            board.tick();
        }
        *self.color_clocks = self.color_clocks.wrapping_add(u64::from(CPU_DIVISOR));
    }

    fn rdy_stall(&self) -> bool {
        self.bus.tia.rdy_stall()
    }
}

/// Owns the run loop and the lockstep timebase.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct System {
    /// The 6507 CPU.
    pub cpu: Cpu,
    /// The Bus — owns the TIA, RIOT, cart-via-board, controllers, open bus.
    pub bus: Bus,
    /// Per-power-on CPU/color-clock phase alignment, from a SEEDED PRNG (never
    /// the OS RNG): a one-time 0..3 extra color-clock offset applied before
    /// the CPU's first cycle, so power-on alignment is deterministic per seed.
    phase: u8,
    /// Total color clocks elapsed since power-on.
    color_clocks: u64,
}

/// A minimal deterministic byte generator for seeding power-on RAM/register
/// state (ADR 0006) — SplitMix64, chosen only for being a tiny, dependency-
/// free, well-known-good bit mixer; not a cryptographic or statistical-
/// quality requirement, just "same seed -> same bytes, different seed ->
/// different bytes" (the determinism contract, ADR 0004).
struct SplitMix64(u64);

impl SplitMix64 {
    const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl System {
    /// Power on with a determinism seed (drives the phase alignment AND the
    /// power-on RAM/register randomization, ADR 0006 — real hardware powers
    /// up with indeterminate RAM/register contents, but "indeterminate" must
    /// still be a deterministic function of the seed, never the OS RNG, per
    /// ADR 0004).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        let mut rng = SplitMix64(seed);
        // RIOT RAM (128 B, the console's only general RAM): fill 16 bytes at
        // a time from each `next_u64()` call.
        for chunk in bus.riot.ram.chunks_mut(8) {
            let bytes = rng.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        // A/X/Y: real 6502/6507 reset does NOT touch these, so their power-on
        // value is whatever was last driven on the bus — seed it too rather
        // than leaving it at a fixed 0 every run.
        let axy = rng.next_u64().to_le_bytes();
        cpu.a = axy[0];
        cpu.x = axy[1];
        cpu.y = axy[2];

        Self {
            cpu,
            bus,
            // `seed % CPU_DIVISOR` is in `0..3`, so the narrowing cannot truncate.
            phase: u8::try_from(seed % u64::from(CPU_DIVISOR)).unwrap_or(0),
            color_clocks: 0,
        }
    }

    /// Resets the CPU using the currently installed cartridge/bus, applying the
    /// seeded power-on phase offset first.
    pub fn reset(&mut self) {
        for _ in 0..self.phase {
            self.bus.tia.tick_color_clock();
            self.color_clocks = self.color_clocks.wrapping_add(1);
        }
        let mut view = CpuView {
            bus: &mut self.bus,
            color_clocks: &mut self.color_clocks,
        };
        self.cpu.reset(&mut view);
    }

    /// Run exactly one 6507 instruction to completion and return its cycle
    /// count. This is the scheduler's sole driving primitive: every cycle the
    /// instruction consumes advances the TIA/RIOT/cart in lockstep via
    /// [`CpuView::tick_cycle`] as it goes (see the module doc comment) — by the
    /// time this returns, the whole system (not just the CPU) has caught up to
    /// the instruction's true elapsed time.
    pub fn step_instruction(&mut self) -> u8 {
        let mut view = CpuView {
            bus: &mut self.bus,
            color_clocks: &mut self.color_clocks,
        };
        self.cpu.step(&mut view)
    }

    /// Advance the TIA alone by exactly one color clock, with no CPU
    /// involvement. Useful for tests / tooling that want to observe raw TIA
    /// timing without running a program. Does NOT drive the CPU — pair with
    /// [`Self::step_instruction`] for that.
    pub fn tick_one_color_clock(&mut self) {
        self.bus.tia.tick_color_clock();
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

    // ADR 0006: power-on RAM/register state is seeded-random, not a fixed
    // constant and not the OS RNG — same seed must reproduce byte-identically
    // (the determinism contract, ADR 0004), matching Stella's `ramrandom=
    // <seed>` model rather than true hardware nondeterminism.
    #[test]
    fn seeded_power_on_ram_is_deterministic() {
        let a = System::new(1234);
        let b = System::new(1234);
        assert_eq!(a.bus.riot.ram, b.bus.riot.ram);
        assert_eq!((a.cpu.a, a.cpu.x, a.cpu.y), (b.cpu.a, b.cpu.x, b.cpu.y));
    }

    #[test]
    fn different_seeds_produce_different_power_on_ram() {
        let a = System::new(1);
        let b = System::new(2);
        assert_ne!(
            a.bus.riot.ram, b.bus.riot.ram,
            "different seeds should not coincidentally produce identical RAM"
        );
    }

    // Regression guard against reverting to the old `[0; 128]` /
    // `Cpu::new()`-hardcoded-zero power-on state this ADR replaced.
    #[test]
    fn power_on_state_is_not_all_zero() {
        let sys = System::new(0xDEAD_BEEF);
        assert!(
            sys.bus.riot.ram.iter().any(|&b| b != 0),
            "seeded RAM should not be all-zero"
        );
    }

    #[test]
    fn color_clock_advances() {
        let mut sys = System::new(0);
        sys.tick_one_color_clock();
        assert_eq!(sys.color_clocks(), 1);
    }

    /// Builds a 2 KiB `Rom2K` cart (mirrored across `$1000..=$1FFF`) with
    /// `code` placed at `$1000` and the reset vector pointing at `$1000`.
    fn cart_with_code(code: &[u8]) -> rusty2600_cart::Cartridge {
        let mut img = [0u8; 0x0800];
        img[..code.len()].copy_from_slice(code);
        // $FFFC/$FFFD mask to $1FFC/$1FFD, which lands in the cart's SECOND
        // mirror (the 2 KiB image repeats twice across the 4 KiB window) at
        // image offset 0x7FC/0x7FD.
        img[0x7FC] = 0x00;
        img[0x7FD] = 0x10;
        rusty2600_cart::Rom2K::new(&img)
            .map(rusty2600_cart::Cartridge::Rom2K)
            .expect("2K cart")
    }

    // Regression test for the CPU/TIA desync: an instruction with N real 6502
    // cycles must advance the color clock by exactly 3*N, not 1 (the old
    // per-color-clock-tick model) and not 0 (no advancement at all).
    #[test]
    fn step_instruction_advances_color_clock_by_3x_its_cycle_count() {
        let mut sys = System::new(0);
        sys.bus.board = Some(cart_with_code(&[0xEA])); // NOP, 2 cycles
        sys.reset();
        let before = sys.color_clocks();
        let cycles = sys.step_instruction();
        assert_eq!(cycles, 2);
        assert_eq!(sys.color_clocks() - before, 2 * u64::from(CPU_DIVISOR));
    }

    #[test]
    fn wsync_freezes_the_cpu_but_the_color_clock_keeps_advancing() {
        let mut sys = System::new(0);
        // STA WSYNC ($85 $02), then NOP.
        sys.bus.board = Some(cart_with_code(&[0x85, 0x02, 0xEA]));
        sys.reset();

        sys.step_instruction(); // STA WSYNC: sets rdy_stall, color_clock keeps moving.
        assert!(sys.bus.tia.rdy_stall());

        let scanline_before = sys.bus.tia.scanline;
        let clocks_before_release = sys.color_clocks();
        sys.step_instruction(); // The NOP fetch must spin until HBLANK releases RDY.
        assert!(!sys.bus.tia.rdy_stall());
        // The stall-spin must have carried the beam into the next scanline...
        assert!(sys.bus.tia.scanline > scanline_before);
        // ...consuming far more color clocks than the NOP's own 2 (6 color
        // clocks) would alone — proving the CPU genuinely spun in place rather
        // than racing ahead of the color clock.
        assert!(sys.color_clocks() - clocks_before_release > 6);
    }
}
