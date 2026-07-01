//! Register-file layout.
//!
//! The ARM7TDMI Thumb-1 register file is 16 general registers, `R0..=R12`
//! general purpose, `R13` the stack pointer, `R14` the link register, `R15`
//! the program counter.
//!
//! ## The stored-PC invariant
//!
//! Ported faithfully from Gopher2600's own bookkeeping: the ARM7TDMI's
//! 3-stage pipeline means an instruction, while executing, reads `R15` as
//! "the address of this instruction plus 4" (documented in the
//! "ARM7TDMI-S Technical Reference Manual r4p3", quoted throughout
//! `thumb.rs`). Rather than track a separate "operand PC" alongside a
//! conventional "next fetch address" PC, this crate mirrors the reference
//! implementation's simpler scheme: the STORED value of `registers[REG_PC]`
//! is always `(next fetch address) + 2`.
//!
//! `Arm7Tdmi::step` bumps it by another `+2` before executing (to
//! `fetch_addr + 4`, the value instructions expect), then either leaves it
//! there (no branch — the next step's `fetch_addr' = registers[PC] - 2`
//! naturally advances by 2) or lets the executed instruction overwrite it
//! with a real target (a branch — already computed using the same `+4`
//! convention, so no further adjustment is needed). [`crate::Arm7Tdmi::register`]
//! and [`crate::Arm7Tdmi::set_register`] normalize this away for external
//! callers, who only ever see architectural (non-pipelined) PC values.

/// Number of general registers (`R0..=R15`).
pub const NUM_REGISTERS: usize = 16;

/// Stack pointer (`R13`).
pub const REG_SP: usize = 13;
/// Link register (`R14`).
pub const REG_LR: usize = 14;
/// Program counter (`R15`).
pub const REG_PC: usize = 15;
