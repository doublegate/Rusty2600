//! `rusty2600-riot` — the MOS 6532 RIOT (RAM-I/O-Timer).

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

mod serde_bytes_array;

/// The interval-timer prescale (CPU cycles per timer decrement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Prescale {
    /// Divide by 1
    By1 = 1,
    /// Divide by 8
    By8 = 8,
    /// Divide by 64
    By64 = 64,
    /// Divide by 1024
    #[default]
    By1024 = 1024,
}

/// The RIOT interval timer.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Timer {
    /// The current timer value.
    pub value: u8,
    /// The current prescale.
    pub prescale: Prescale,
    elapsed: u16,
    underflow: bool,
    post_underflow: bool,
    /// `true` for the one cycle on which [`Riot::tick`] just performed the
    /// 0->0xFF wrap (set in `tick`, cleared at the start of the NEXT
    /// `tick`). Mirrors Stella's `myWrappedThisCycle`: a read of INTIM on
    /// this exact cycle must not immediately revert `post_underflow` (the
    /// interrupt/fast-mode condition it just observed), matching real 6532
    /// silicon where the flag transition and the read happen on the same
    /// clock edge.
    wrapped_this_cycle: bool,
}

/// The MOS 6532 RIOT chip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Riot {
    /// 128 bytes of RAM.
    #[serde(with = "serde_bytes_array")]
    pub ram: [u8; 128],
    /// I/O ports.
    pub ports: [u8; 2],
    /// Data Direction Registers.
    pub ddr: [u8; 2],
    /// The interval timer.
    pub timer: Timer,
    /// External pins state
    pub pins: [u8; 2],
}

impl Default for Riot {
    fn default() -> Self {
        Self {
            ram: [0; 128],
            ports: [0xFF; 2],
            ddr: [0; 2],
            timer: Timer::default(),
            pins: [0xFF; 2],
        }
    }
}

impl Riot {
    /// Creates a new RIOT.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the RIOT by one CPU cycle.
    pub fn tick(&mut self) {
        self.timer.wrapped_this_cycle = false;
        self.timer.elapsed = self.timer.elapsed.wrapping_add(1);

        let target = if self.timer.post_underflow {
            1
        } else {
            self.timer.prescale as u16
        };

        if self.timer.elapsed >= target {
            self.timer.elapsed = 0;

            if self.timer.value == 0 {
                self.timer.value = 0xFF;
                self.timer.underflow = true;
                self.timer.post_underflow = true;
                self.timer.wrapped_this_cycle = true;
            } else {
                self.timer.value = self.timer.value.wrapping_sub(1);
            }
        }
    }

    /// CPU reads from the RIOT.
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        if addr & 0x0200 == 0 {
            // A9 = 0 -> RAM
            self.ram[(addr & 0x7F) as usize]
        } else {
            // A9 = 1 -> I/O & Timer
            if addr & 0x04 == 0 {
                // A2 = 0 -> I/O ports
                match addr & 0x03 {
                    0 => (self.ports[0] & self.ddr[0]) | (self.pins[0] & !self.ddr[0]),
                    1 => self.ddr[0],
                    2 => (self.ports[1] & self.ddr[1]) | (self.pins[1] & !self.ddr[1]),
                    3 => self.ddr[1],
                    _ => unreachable!(),
                }
            } else {
                // A2 = 1 -> Timer
                if addr & 0x01 == 0 {
                    // A0 = 0 -> INTIM (Read Timer)
                    self.timer.underflow = false; // Reading INTIM clears timer interrupt flag
                    // Reading INTIM also reverts the decrement rate from the
                    // post-underflow divide-by-1 mode back to the normal
                    // prescale interval (confirmed against Stella's
                    // `M6532::peek`'s `myInterruptFlag &= ~TimerBit` on an
                    // INTIM read, which is the SAME flag `updateEmulation`
                    // gates its fast-vs-prescaled decrement on) -- UNLESS
                    // the underflow happened on this exact cycle, matching
                    // Stella's `!myWrappedThisCycle` guard (the flag must
                    // not un-fire on the very access that just observed it).
                    if !self.timer.wrapped_this_cycle {
                        self.timer.post_underflow = false;
                    }
                    self.timer.value
                } else {
                    // A0 = 1 -> INSTAT (Read Timer Status)
                    if self.timer.underflow { 0xC0 } else { 0x00 }
                }
            }
        }
    }

    /// CPU writes to the RIOT.
    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        if addr & 0x0200 == 0 {
            // A9 = 0 -> RAM
            self.ram[(addr & 0x7F) as usize] = val;
        } else {
            // A9 = 1 -> I/O & Timer
            if addr & 0x04 == 0 {
                // A2 = 0 -> I/O ports
                match addr & 0x03 {
                    0 => self.ports[0] = val,
                    1 => self.ddr[0] = val,
                    2 => self.ports[1] = val,
                    3 => self.ddr[1] = val,
                    _ => unreachable!(),
                }
            } else {
                // A2 = 1 -> Timer Write
                let prescale = match addr & 0x03 {
                    0 => Prescale::By1,
                    1 => Prescale::By8,
                    2 => Prescale::By64,
                    3 => Prescale::By1024,
                    _ => unreachable!(),
                };
                self.set_timer(val, prescale);
            }
        }
    }

    fn set_timer(&mut self, val: u8, prescale: Prescale) {
        self.timer.value = val;
        self.timer.prescale = prescale;
        // The cycle a TIMxT write happens on counts as the FIRST cycle of the
        // first interval, not cycle zero of a fresh count: real 6532 silicon
        // decrements INTIM just ONE cycle after the write, not a full
        // `prescale` cycles later (confirmed against Stella's
        // `M6532::setTimerRegister`, which sets `mySubTimer = myDivider - 1`
        // on write). Starting `elapsed` at `prescale - 1` reproduces that: the
        // very next `tick()` call reaches `target` and decrements.
        //
        // Getting this wrong doesn't just mis-time reads of INTIM — every
        // TIMxT write silently costs `prescale - 1` extra cycles on its first
        // interval, which throws off any downstream polling loop timed
        // against it (e.g. an end-of-frame overscan wait), a real symptom we
        // traced to visible frame-length jitter.
        self.timer.elapsed = (prescale as u16) - 1;
        self.timer.underflow = false;
        self.timer.post_underflow = false; // Exits 1T mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs() {
        let riot = Riot::new();
        assert_eq!(riot.ram.len(), 128);
        assert_eq!(riot.ports[0], 0xFF);
        assert_eq!(riot.pins[0], 0xFF);
    }

    #[test]
    fn ram_mirroring() {
        let mut riot = Riot::new();
        riot.cpu_write(0x0080, 42); // RAM start
        assert_eq!(riot.cpu_read(0x0080), 42);
        assert_eq!(riot.cpu_read(0x0180), 42); // Mirrored
        assert_eq!(riot.cpu_read(0x00FF), 0); // Unrelated RAM end
    }

    #[test]
    fn io_ports_with_ddr() {
        let mut riot = Riot::new();
        riot.pins[0] = 0b1010_1010; // External input
        riot.ddr[0] = 0b1111_0000; // High nibble output, low nibble input
        riot.ports[0] = 0b1100_1100; // Output register

        // Read should combine output and input based on DDR
        assert_eq!(riot.cpu_read(0x0280), 0b1100_1010);
    }

    #[test]
    fn timer_ticks_and_underflows() {
        let mut riot = Riot::new();
        riot.cpu_write(0x294, 2); // TIM1T = 2 (A2=1, prescale By1)

        riot.tick(); // elapsed 1, val 1
        assert_eq!(riot.cpu_read(0x284), 1);
        riot.tick(); // elapsed 1, val 0
        assert_eq!(riot.cpu_read(0x284), 0);
        riot.tick(); // elapsed 1, underflow to FF
        assert_eq!(riot.cpu_read(0x284), 0xFF);
    }

    // Regression test for the "first decrement fires one cycle after the
    // write, not a full prescale interval later" fix: verified against
    // Stella's `M6532::setTimerRegister` (`mySubTimer = myDivider - 1`).
    // TIM8T=1 (prescale 8): the write consumes the cycle it happens on as
    // though 7 of the 8 sub-cycles already elapsed, so the FIRST decrement
    // (1 -> 0) fires after just 1 tick; every decrement after that is a full
    // 8-cycle interval apart.
    #[test]
    fn timer_first_decrement_fires_one_cycle_after_write() {
        let mut riot = Riot::new();
        riot.cpu_write(0x295, 1); // TIM8T = 1

        riot.tick();
        assert_eq!(
            riot.cpu_read(0x284),
            0,
            "first decrement should fire after 1 cycle"
        );
    }

    // The DirtyHairy/Stella RIOT model this project targets (docs/riot.md,
    // ref-docs open questions): the earliest a program can ever read INTIM
    // after a TIMxT write is one CPU cycle later (a write and a subsequent
    // read are always different instructions), and at that first opportunity
    // INTIM already reads back as `written_value - 1`, for every prescale —
    // not just the By1 case `timer_first_decrement_fires_one_cycle_after_write`
    // already covers. Pin all four prescales explicitly.
    #[test]
    fn read_after_write_is_value_minus_one_for_every_prescale() {
        for (addr, prescale_name) in [
            (0x294, "TIM1T"),
            (0x295, "TIM8T"),
            (0x296, "TIM64T"),
            (0x297, "T1024T"),
        ] {
            let mut riot = Riot::new();
            riot.cpu_write(addr, 10);
            riot.tick();
            assert_eq!(
                riot.cpu_read(0x284),
                9,
                "{prescale_name}: INTIM one cycle after writing 10 should read 9"
            );
        }
    }

    #[test]
    fn timer_instat_and_post_underflow() {
        let mut riot = Riot::new();
        riot.cpu_write(0x295, 1); // TIM8T = 1

        riot.tick(); // 1 cycle after write: fires immediately, value -> 0.
        assert_eq!(riot.cpu_read(0x284), 0);

        for _ in 0..7 {
            riot.tick();
            // Holds at 0 for one full interval duration (8 cycles) before underflowing.
            assert_eq!(riot.cpu_read(0x284), 0);
        }
        riot.tick(); // The 8th tick since hitting 0: underflows.

        // Check INSTAT without clearing the flag
        assert_eq!(riot.cpu_read(0x285) & 0xC0, 0xC0);

        // Read INTIM to clear flag
        assert_eq!(riot.cpu_read(0x284), 0xFF);

        // INSTAT should now be 0
        assert_eq!(riot.cpu_read(0x285) & 0xC0, 0x00);

        // Next tick should decrement by 1 because it's in post-underflow (1T mode):
        // the INTIM read above landed on the SAME cycle the underflow fired
        // (`wrapped_this_cycle`), so it must not revert the fast decrement rate --
        // matching Stella's `!myWrappedThisCycle` guard on its equivalent flag.
        riot.tick();
        assert_eq!(riot.cpu_read(0x284), 0xFE);
    }

    // Regression test for T-0601-008 (found via a Gopher2600/Stella
    // differential probe against Pitfall II): a program that reads INTIM
    // repeatedly across MANY LATER cycles -- not the one cycle the
    // underflow itself fired on, unlike `timer_instat_and_post_underflow`
    // above -- must see the timer revert to its normal prescale rate.
    // Confirmed against Stella's `M6532::peek`: reading INTIM clears
    // `myInterruptFlag`'s `TimerBit`, and `updateEmulation` gates the fast
    // (divide-by-1) decrement on that SAME flag being set, so a read one or
    // more FULL ticks after the wrap reverts the rate. Before this fix,
    // Rusty2600's `post_underflow` flag only cleared on a fresh `TIMxT`
    // write, never on a read, so the timer stayed in fast mode forever
    // once it underflowed once -- exactly the bug that stalled Pitfall II
    // in its boot-time RIOT-timer wait loop indefinitely.
    #[test]
    fn intim_read_on_a_later_cycle_reverts_post_underflow_to_prescale() {
        let mut riot = Riot::new();
        riot.cpu_write(0x295, 1); // TIM8T = 1
        riot.tick(); // fires immediately: value -> 0
        assert_eq!(riot.cpu_read(0x284), 0);
        for _ in 0..8 {
            riot.tick(); // underflows on the 8th of these: value -> 0xFF, post_underflow = true
        }
        assert_eq!(riot.cpu_read(0x284), 0xFF);

        // Advance one MORE full tick (now definitely a later cycle, not the
        // one the wrap fired on) before reading INTIM again.
        riot.tick(); // still fast mode here: 0xFF -> 0xFE
        assert_eq!(
            riot.cpu_read(0x284),
            0xFE,
            "still in fast mode for this tick"
        );

        // Reading INTIM now (a later cycle) must revert the rate: the NEXT
        // tick should decrement by the full TIM8T prescale (8), not by 1.
        riot.tick();
        assert_eq!(
            riot.cpu_read(0x284),
            0xFE,
            "first tick after the reverting read should NOT decrement yet (8-cycle interval)"
        );
        for _ in 0..6 {
            riot.tick();
            assert_eq!(
                riot.cpu_read(0x284),
                0xFE,
                "still within the 8-cycle interval"
            );
        }
        riot.tick(); // the 8th tick since reverting: one prescaled decrement
        assert_eq!(
            riot.cpu_read(0x284),
            0xFD,
            "after 8 ticks at the reverted prescale, exactly one decrement"
        );
    }
}
