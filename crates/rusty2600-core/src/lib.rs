//! `rusty2600-core` — the Bus + the master-clock lockstep scheduler. The single
//! crate that knows about every chip; it re-exports their public types so
//! downstream consumers depend on `rusty2600-core`, not the chip crates
//! directly.
//!
//! The timebase is integer TIA color clocks with the 6507 on every third one and
//! the `WSYNC`/`RDY` beam-stall freezing the CPU — see [`scheduler`]. The Bus
//! owns the TIA (video + audio), the RIOT (RAM + I/O + timer), and the cart
//! board; there is no separate WRAM (the 2600's only RAM is in the RIOT) — see
//! [`bus`].

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

pub mod bus;
pub mod save_state;
pub mod scheduler;

// Re-export the chip crates (the public surface).
pub use rusty2600_cart as cart;
pub use rusty2600_cpu as cpu;
pub use rusty2600_riot as riot;
pub use rusty2600_tia as tia;

pub use bus::{AudioBus, Bus, VideoBus, WriteEvent, WriteLog};
pub use save_state::{SaveState, SaveStateError};
pub use scheduler::System;

// Re-export the cart tiering types so the test-harness honesty gate (and any
// downstream consumer) reaches them through the core, not the chip crate.
pub use rusty2600_cart::{Board, Tier, detect};
