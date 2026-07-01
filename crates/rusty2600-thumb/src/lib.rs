//! `rusty2600-thumb` — an ARM7TDMI Thumb-1 interpreter for the Harmony/Melody
//! coprocessor cartridges (DPC+/CDF/CDFJ/CDFJ+).
//!
//! Ported from Gopher2600's Go implementation
//! (`hardware/memory/cartridge/arm/`), not Stella's C++ `Thumbulator` — its
//! memory-safety-first style (explicit bounds, no raw pointer arithmetic)
//! maps far more naturally onto this project's `#![forbid(unsafe_code)]`
//! house style. Only the ARM7TDMI / Thumb-1 (16-bit encoding) subset is
//! ported: the Harmony/Melody boards this crate targets never execute
//! Thumb-2 (32-bit) instructions, so Gopher2600's `thumb2*.go`
//! (ARMv7-M/Cortex-M0 support) and its `fpu`/`rng`/`timer` peripheral
//! packages are out of scope here — those are either a different chip
//! generation or per-cartridge peripheral registers that belong in a future
//! `Board` implementation, not this interpreter core.
//!
//! This release lands the interpreter core plus conformance tests only. It
//! is NOT yet wired into any `rusty2600-cart` `Board`/`Cartridge` variant —
//! that lands one coprocessor family at a time in the `v1.6.x` patch train
//! (`T-0401-006`).

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

mod cycles;
mod mam;
mod memory;
mod registers;
mod status;
mod thumb;

pub use cycles::{BranchTrail, BusAccess, CycleType};
pub use mam::{Mam, MamMode};
pub use memory::{Fault, ThumbMemory};
pub use registers::{NUM_REGISTERS, REG_LR, REG_PC, REG_SP};
pub use status::Status;

/// Outcome of a single [`Arm7Tdmi::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// The instruction executed normally; the interpreter can keep running.
    Normal,
    /// A `BX`/`BLX` branched to the program's expected return address — the
    /// coprocessor program has finished and control returns to the 6507.
    ProgramEnded,
    /// Execution hit a memory fault (illegal/unimplemented/misaligned/null
    /// access). Carries the fault so the caller can decide how to recover
    /// (e.g. treat it as a benign register read of an unmodeled peripheral,
    /// or surface it as a real bug).
    Fault(Fault),
}

/// A Thumb-1 ARM7TDMI interpreter, generic over the memory it executes
/// against. One instance owns exactly the architectural state ARM7TDMI
/// Thumb mode exposes: 16 general registers and the N/Z/C/V status flags.
///
/// `M` is a type parameter rather than a `dyn Trait` object so this stays
/// `no_std`-friendly without needing `alloc::boxed::Box<dyn ThumbMemory>` —
/// the same reasoning `rusty2600-cart`'s closed `Cartridge` enum uses to
/// avoid `dyn Board`.
pub struct Arm7Tdmi<M: ThumbMemory> {
    registers: [u32; registers::NUM_REGISTERS],
    status: Status,
    mam: Mam,
    cycles: cycles::CycleState,
    /// The address of the instruction currently executing (for fault
    /// reporting) — NOT the same as `registers[REG_PC]`, which during
    /// execution holds `instruction_address + 4` per the ARM7TDMI's
    /// documented "PC reads as current instruction + 4" convention (see
    /// `registers.rs` for the full explanation of this invariant).
    instruction_pc: u32,
    /// The address a `BX`/`BLX` must reach for the interpreter to report
    /// [`StepOutcome::ProgramEnded`] — the address the 6507 originally
    /// called into this coprocessor from.
    expected_return_address: u32,
    mem: M,
}

impl<M: ThumbMemory> Arm7Tdmi<M> {
    /// Build a fresh interpreter and reset it against `mem`'s reset vectors.
    #[must_use]
    pub fn new(mem: M) -> Self {
        let mut arm = Self {
            registers: [0; registers::NUM_REGISTERS],
            status: Status::default(),
            mam: Mam::default(),
            cycles: cycles::CycleState::default(),
            instruction_pc: 0,
            expected_return_address: 0,
            mem,
        };
        arm.reset();
        arm
    }

    /// Reset the interpreter to its power-on state: general registers
    /// cleared, SP/LR/PC loaded from [`ThumbMemory::reset_vectors`].
    pub fn reset(&mut self) {
        self.registers = [0; registers::NUM_REGISTERS];
        self.status = Status::default();
        let (sp, lr, pc) = self.mem.reset_vectors();
        self.registers[registers::REG_SP] = sp;
        self.registers[registers::REG_LR] = lr;
        // `+ 2` establishes the "stored PC is always (next fetch address) + 2"
        // invariant `step()` relies on — see `registers.rs`.
        self.registers[registers::REG_PC] = pc.wrapping_add(2);
        self.expected_return_address = (lr.wrapping_add(2)) & !1;
        self.instruction_pc = pc;
    }

    /// Read a general register (`R0..=R15`) as architectural state — i.e.
    /// with `PC` normalized to the address of the NEXT instruction to fetch,
    /// not the "+4" value the interpreter presents to itself mid-instruction.
    #[must_use]
    pub fn register(&self, index: usize) -> u32 {
        if index == registers::REG_PC {
            self.registers[index].wrapping_sub(2)
        } else {
            self.registers[index]
        }
    }

    /// Overwrite a general register. Setting `REG_PC` re-establishes the
    /// `+2` storage invariant so the next [`Self::step`] fetches from `value`.
    pub fn set_register(&mut self, index: usize, value: u32) {
        if index == registers::REG_PC {
            self.registers[index] = value.wrapping_add(2);
        } else {
            self.registers[index] = value;
        }
    }

    /// The N/Z/C/V status flags.
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// The address of the instruction most recently fetched by [`Self::step`].
    #[must_use]
    pub const fn instruction_pc(&self) -> u32 {
        self.instruction_pc
    }

    /// Access the memory this interpreter executes against (e.g. for a
    /// future `Board` to inspect/mutate cartridge RAM between steps).
    pub fn memory(&mut self) -> &mut M {
        &mut self.mem
    }

    /// Execute exactly one Thumb-1 instruction and return the number of
    /// approximate cycles it consumed alongside the outcome.
    ///
    /// The cycle count is an approximation inherited from the reference
    /// implementation's own MAM-prefetch/flash-wait-state model (see
    /// `cycles.rs`) — Gopher2600 itself notes this model's constants are
    /// unverified ("no idea if this value is sufficient"), so this crate
    /// does not claim cycle-exactness for the coprocessor path, only a
    /// faithful port of the same approximation.
    pub fn step(&mut self) -> (StepOutcome, u32) {
        let fetch_addr = self.registers[registers::REG_PC].wrapping_sub(2);
        self.instruction_pc = fetch_addr;

        let opcode = match self.read16(fetch_addr, false) {
            Ok(v) => v,
            Err(fault) => return (StepOutcome::Fault(fault), 0),
        };

        // "Bump PC for prefetch": the stored PC becomes `fetch_addr + 4`,
        // which is exactly the value Thumb-1 instructions expect to read
        // when they use PC as an operand (see `registers.rs`).
        self.registers[registers::REG_PC] = fetch_addr.wrapping_add(4);
        let pc_before_execute = self.registers[registers::REG_PC];

        self.cycles_begin_instruction();

        // Charge the cost of fetching THIS instruction's own opcode, using
        // whatever cycle type the PREVIOUS instruction left in
        // `prefetch_cycle` (default S; a store sets N) — equivalent to the
        // reference charging this same fetch as a trailing cost of the
        // previous iteration, just attributed here instead (see cycles.rs).
        if matches!(self.cycles.prefetch_cycle, cycles::CycleType::N) {
            self.ncycle(cycles::BusAccess::Prefetch, fetch_addr);
        } else {
            self.scycle(cycles::BusAccess::Prefetch, fetch_addr);
        }
        self.cycles.prefetch_cycle = cycles::CycleType::S;

        let outcome = thumb::execute(self, opcode);

        let branched = self.registers[registers::REG_PC] != pc_before_execute;
        if branched {
            self.fill_pipeline();
        }
        // No branch: leave the stored PC at `fetch_addr + 4` (matching the
        // reference's own bookkeeping) so the NEXT step's
        // `fetch_addr' = registers[PC] - 2` naturally lands on
        // `fetch_addr + 2`, the correct next sequential instruction.

        let cycle_count = self.cycles_end_instruction();

        match outcome {
            Ok(Some(())) => (StepOutcome::Normal, cycle_count),
            Ok(None) => (StepOutcome::ProgramEnded, cycle_count),
            Err(fault) => (StepOutcome::Fault(fault), cycle_count),
        }
    }
}
