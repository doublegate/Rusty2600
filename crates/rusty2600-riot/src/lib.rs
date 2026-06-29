//! `rusty2600-riot` — the MOS 6532 RIOT (RAM-I/O-Timer).
//!
//! The 6532 supplies three things the VCS needs outside the TIA:
//!
//! - **128 bytes of RAM** — the console's *only* general RAM (there is no
//!   separate WRAM; the CPU stack overlaps this 128-byte region).
//! - **Two 8-bit I/O ports** — `SWCHA` (the two joystick directions) and
//!   `SWCHB` (the console switches: select, reset, difficulty, colour/B-W),
//!   each with a data-direction register.
//! - **An interval timer** — write `TIM1T` / `TIM8T` / `TIM64T` / `T1024T` to
//!   load the counter at a 1 / 8 / 64 / 1024 CPU-cycle prescale; read `INTIM`
//!   for the current value and `INSTAT` for the timer flags.
//!
//! **Audio is NOT here** — the VCS's two sound channels live in the TIA
//! (`rusty2600-tia::audio`). The 6532 has no sound hardware.
//!
//! Part of the one-directional chip-crate graph (see `docs/architecture.md`):
//! this crate is independent — no video / audio / cart dependency. It is
//! `no_std` plus `alloc` for bare-metal cross-compile; only the frontend
//! carries `std` and `unsafe`.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

/// The interval-timer prescale (CPU cycles per timer decrement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prescale {
    /// `TIM1T` — decrement every CPU cycle.
    By1 = 1,
    /// `TIM8T` — every 8 cycles.
    By8 = 8,
    /// `TIM64T` — every 64 cycles.
    By64 = 64,
    /// `T1024T` — every 1024 cycles (the power-on default).
    #[default]
    By1024 = 1024,
}

/// The 6532 interval timer.
#[derive(Debug, Default, Clone)]
pub struct Timer {
    /// `INTIM` — the current counter value.
    pub value: u8,
    /// The active prescale.
    pub prescale: Prescale,
    /// Cycles accumulated toward the next decrement.
    elapsed: u16,
    // TODO(T-PS-041): INSTAT underflow flag + the post-underflow 1-cycle mode.
}

/// MOS 6532 RIOT state.
///
/// Holds the RAM, the two I/O ports + their direction regs, and the interval
/// timer. Stub: fields are real, the access decode is a `// TODO`. Pin against
/// the test ROMs FIRST (test-ROM-is-spec), then implement until they pass.
#[derive(Debug, Clone)]
pub struct Riot {
    /// 128 bytes of RAM — the console's only general RAM (stack overlaps it).
    pub ram: [u8; 128],
    /// `SWCHA` (joystick directions) / `SWCHB` (console switches) port latches.
    pub ports: [u8; 2],
    /// Data-direction registers for the two ports (`SWACNT` / `SWBCNT`).
    pub ddr: [u8; 2],
    /// The interval timer.
    pub timer: Timer,
}

impl Default for Riot {
    fn default() -> Self {
        Self {
            ram: [0; 128],
            ports: [0xFF; 2], // pulled-up inputs read high when nothing is pressed.
            ddr: [0; 2],
            timer: Timer::default(),
        }
    }
}

impl Riot {
    /// Construct at power-on. RAM power-on contents are randomized from a
    /// *seeded* PRNG by the owning `System` (determinism contract — see
    /// `docs/adr/0004`), never the OS RNG; this bare constructor zero-inits RAM.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the interval timer by one CPU cycle. Hot path: allocation-free.
    // Not `const`: this stub will gain real prescale/underflow branching, so
    // marking it const now would have to be reverted (clippy::missing_const_for_fn
    // can't see the planned mutation).
    #[allow(clippy::missing_const_for_fn)]
    pub fn tick(&mut self) {
        // TODO(T-PS-040): prescale the timer, decrement `INTIM`, set the
        // INSTAT underflow flag, and switch to the post-underflow 1-cycle mode.
        self.timer.elapsed = self.timer.elapsed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs() {
        let riot = Riot::new();
        assert_eq!(riot.ram.len(), 128);
        // Idle inputs float high.
        assert_eq!(riot.ports[0], 0xFF);
    }
}
