import os

code = """
//! `rusty2600-riot` — the MOS 6532 RIOT (RAM-I/O-Timer).

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

/// The interval-timer prescale (CPU cycles per timer decrement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prescale {
    By1 = 1,
    By8 = 8,
    By64 = 64,
    #[default]
    By1024 = 1024,
}

#[derive(Debug, Default, Clone)]
pub struct Timer {
    pub value: u8,
    pub prescale: Prescale,
    elapsed: u16,
    underflow: bool,
    post_underflow: bool,
}

#[derive(Debug, Clone)]
pub struct Riot {
    pub ram: [u8; 128],
    pub ports: [u8; 2],
    pub ddr: [u8; 2],
    pub timer: Timer,
}

impl Default for Riot {
    fn default() -> Self {
        Self {
            ram: [0; 128],
            ports: [0xFF; 2],
            ddr: [0; 2],
            timer: Timer::default(),
        }
    }
}

impl Riot {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self) {
        self.timer.elapsed = self.timer.elapsed.wrapping_add(1);
        
        let target = if self.timer.post_underflow { 1 } else { self.timer.prescale as u16 };
        
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
    
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr & 0x297 {
            // RAM
            0x0080..=0x00FF => self.ram[(addr & 0x7F) as usize],
            
            // I/O & Timer
            0x0280 => self.ports[0],
            0x0281 => self.ddr[0],
            0x0282 => self.ports[1],
            0x0283 => self.ddr[1],
            0x0284 => {
                self.timer.underflow = false; // reading INTIM clears underflow
                self.timer.value
            },
            0x0285 => {
                let status = if self.timer.underflow { 0xC0 } else { 0x00 };
                self.timer.underflow = false; // reading INSTAT clears underflow
                status
            },
            _ => 0,
        }
    }
    
    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        match addr & 0x297 {
            // RAM
            0x0080..=0x00FF => self.ram[(addr & 0x7F) as usize] = val,
            
            // I/O & Timer
            0x0280 => self.ports[0] = val,
            0x0281 => self.ddr[0] = val,
            0x0282 => self.ports[1] = val,
            0x0283 => self.ddr[1] = val,
            
            0x0294 => self.set_timer(val, Prescale::By1),
            0x0295 => self.set_timer(val, Prescale::By8),
            0x0296 => self.set_timer(val, Prescale::By64),
            0x0297 => self.set_timer(val, Prescale::By1024),
            _ => {}
        }
    }
    
    fn set_timer(&mut self, val: u8, prescale: Prescale) {
        self.timer.value = val;
        self.timer.prescale = prescale;
        self.timer.elapsed = 0;
        self.timer.underflow = false;
        self.timer.post_underflow = false;
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
    }
    
    #[test]
    fn timer_ticks_and_underflows() {
        let mut riot = Riot::new();
        riot.cpu_write(0x294, 2); // TIM1T = 2
        
        riot.tick(); // elapsed 1, val 2
        assert_eq!(riot.cpu_read(0x284), 1);
        riot.tick(); // elapsed 1, val 1
        assert_eq!(riot.cpu_read(0x284), 0);
        riot.tick(); // elapsed 1, val 0 -> underflow to FF
        assert_eq!(riot.cpu_read(0x284), 0xFF);
    }
}
"""

with open("crates/rusty2600-riot/src/lib.rs", "w") as f:
    f.write(code)
print("Updated RIOT")
