//! `rusty2600-cpu` — MOS 6507 (the Atari 2600 / VCS CPU).
//!
//! The 6507 is a cost-reduced 6502 in a 28-pin DIP: the core 6502 instruction
//! decode + register file are unchanged, but the package brings out only **13
//! address pins (A0..=A12 → 8 KiB visible)** and **no IRQ / NMI pins are wired**
//! (the VCS has nothing to drive them). RDY *is* wired, and the TIA's WSYNC
//! beam-stall asserts it — see the scheduler in `rusty2600-core`.

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

mod bus;
mod cpu;
mod status;

pub use bus::CpuBus;
pub use cpu::Cpu;
pub use status::Status;
