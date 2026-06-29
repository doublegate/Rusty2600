//! `rusty2600-cpu` — MOS 6507 (the Atari 2600 / VCS CPU).
//!
//! The 6507 is a cost-reduced 6502 in a 28-pin DIP: the core 6502 instruction
//! decode + register file are unchanged, but the package brings out only **13
//! address pins (A0..=A12 → 8 KiB visible)** and **no IRQ / NMI pins are wired**
//! (the VCS has nothing to drive them). RDY *is* wired, and the TIA's WSYNC
//! beam-stall asserts it — see the scheduler in `rusty2600-core`.
//!
//! The 6502 *core* here stays complete (all addressing modes + the full
//! documented opcode set) and is expected to grow the **undocumented opcodes**
//! too: the 2600's tiny ROM budget made commercial games lean on them heavily,
//! so they are spec, not optional. Behaviour is pinned against the Klaus
//! functional-test and 6502 golden-log suites (test-ROM-is-spec).
//!
//! Part of the one-directional chip-crate graph (see `docs/architecture.md`):
//! this crate depends on nothing console-specific (`core` + `alloc` +
//! `bitflags` only). `#![no_std]` + `alloc` so it cross-compiles to a bare-metal
//! target; only the frontend carries `std` + `unsafe`.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use bitflags::bitflags;

bitflags! {
    /// The 6502 processor status register `P`.
    ///
    /// Bit 5 (`UNUSED`) reads as 1 on real silicon; `BREAK` only exists in the
    /// value pushed to the stack by `PHP` / an interrupt, never as a physical
    /// latch. Both are modelled as flags so push/pull round-trips bit-exactly.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Status: u8 {
        /// Carry.
        const CARRY = 0b0000_0001;
        /// Zero.
        const ZERO = 0b0000_0010;
        /// Interrupt-disable (inert on the 2600 — no IRQ line is wired).
        const INTERRUPT_DISABLE = 0b0000_0100;
        /// Decimal mode (BCD `ADC`/`SBC`; the 6502 implements it, unlike the 2A03).
        const DECIMAL = 0b0000_1000;
        /// Break (set only in the pushed copy).
        const BREAK = 0b0001_0000;
        /// Unused — reads as 1.
        const UNUSED = 0b0010_0000;
        /// Overflow.
        const OVERFLOW = 0b0100_0000;
        /// Negative.
        const NEGATIVE = 0b1000_0000;
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::UNUSED | Self::INTERRUPT_DISABLE
    }
}

/// The narrow bus the CPU borrows during `tick`.
///
/// The 6507 only drives `A0..=A12`, so the implementation masks the address to
/// 13 bits (`addr & 0x1FFF`) before presenting it here; the bus owner (the
/// `rusty2600-core` `Bus`) decodes TIA / RIOT / RAM / cart from that 8 KiB
/// window. Open-bus reads return the last value on the data bus — the bus owner
/// models that, not the CPU.
pub trait CpuBus {
    /// Read one byte at the (already 13-bit-masked) address.
    fn read(&mut self, addr: u16) -> u8;
    /// Write one byte at the (already 13-bit-masked) address.
    fn write(&mut self, addr: u16, val: u8);
}

/// MOS 6507 register file + internal sequencing state.
///
/// Stub: the register file is real, the execution engine is a `// TODO`. Pin
/// behaviour against the Klaus functional test FIRST (test-ROM-is-spec), then
/// implement until it passes.
#[derive(Debug, Default, Clone)]
pub struct Cpu {
    /// Accumulator.
    pub a: u8,
    /// Index register X.
    pub x: u8,
    /// Index register Y.
    pub y: u8,
    /// Stack pointer (offset into the `$0100` page — but the 2600 has no `$0100`
    /// page in RAM; the stack overlaps the RIOT's 128-byte RAM mirror).
    pub sp: u8,
    /// Program counter.
    pub pc: u16,
    /// Processor status `P`.
    pub p: Status,
    /// Total CPU cycles elapsed since power-on (for the golden-log differ).
    pub cycles: u64,
    // TODO(T-PS-001): instruction-decode sequencer + per-cycle micro-op state.
    // TODO(T-PS-002): the `rdy` (RDY/WSYNC) stall latch the scheduler asserts.
}

impl Cpu {
    /// Construct at power-on. Register power-on values are randomized from a
    /// *seeded* PRNG by the owning `System` (determinism contract — see
    /// `docs/adr/0004`), never the OS RNG; this bare constructor zero-inits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the RESET sequence: load `PC` from the `$FFFC/$FFFD` vector. (The
    /// 6507 has no NMI vector wiring on the VCS; only RESET + the inert IRQ/BRK
    /// vector at `$FFFE/$FFFF` matter.)
    pub fn reset(&mut self, bus: &mut impl CpuBus) {
        let lo = u16::from(bus.read(0xFFFC & 0x1FFF));
        let hi = u16::from(bus.read(0xFFFD & 0x1FFF));
        self.pc = (hi << 8) | lo;
        self.sp = 0xFD;
        self.p = Status::default();
        // TODO(T-PS-001): consume the 7 RESET cycles on the real timing.
    }

    /// Execute one instruction, advancing `bus` per accessed cycle. Hot path
    /// (runs at color-clock / 3): allocation-free, no per-instruction `Box`/dyn.
    // Not `const`: the stub ignores `bus`, but the real fetch/decode/execute
    // drives it every cycle — const-ness would have to be reverted immediately.
    #[allow(clippy::missing_const_for_fn)]
    pub fn tick(&mut self, bus: &mut impl CpuBus) {
        // TODO(T-PS-001): fetch/decode/execute one opcode (documented + the
        // undocumented set the 2600 library relies on), masking every effective
        // address to 13 bits before it reaches `bus`.
        let _ = bus;
        self.cycles = self.cycles.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial flat-memory bus for unit tests.
    struct FlatBus {
        mem: [u8; 0x2000],
    }

    impl CpuBus for FlatBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[(addr & 0x1FFF) as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[(addr & 0x1FFF) as usize] = val;
        }
    }

    #[test]
    fn constructs() {
        let cpu = Cpu::new();
        assert_eq!(cpu.a, 0);
        assert!(cpu.p.contains(Status::UNUSED));
    }

    #[test]
    fn reset_loads_vector() {
        let mut bus = FlatBus { mem: [0; 0x2000] };
        bus.mem[0x1FFC] = 0x34;
        bus.mem[0x1FFD] = 0x12;
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.sp, 0xFD);
    }
}
