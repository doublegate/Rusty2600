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
        self.timer.elapsed = 0;
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

    #[test]
    fn timer_instat_and_post_underflow() {
        let mut riot = Riot::new();
        riot.cpu_write(0x295, 0); // TIM8T = 0

        for _ in 0..7 {
            riot.tick();
            // Should hold at 0 for one interval duration
            assert_eq!(riot.cpu_read(0x284), 0);
        }
        riot.tick(); // tick 8: underflows

        // Check INSTAT without clearing the flag
        assert_eq!(riot.cpu_read(0x285) & 0xC0, 0xC0);

        // Read INTIM to clear flag
        assert_eq!(riot.cpu_read(0x284), 0xFF);

        // INSTAT should now be 0
        assert_eq!(riot.cpu_read(0x285) & 0xC0, 0x00);

        // Next tick should decrement by 1 because it's in post-underflow (1T mode)
        riot.tick();
        assert_eq!(riot.cpu_read(0x284), 0xFE);
    }
}
