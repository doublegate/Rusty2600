import os

code = """
//! The Bus owns everything mutable.

use alloc::boxed::Box;

use rusty2600_cart::Board;
use rusty2600_riot::Riot;
use rusty2600_tia::Tia;
use rusty2600_cpu::CpuBus;

#[derive(Default)]
pub struct Bus {
    pub tia: Tia,
    pub riot: Riot,
    pub board: Option<Box<dyn Board>>,
    pub open_bus: u8,
}

impl core::fmt::Debug for Bus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bus")
            .field("tia", &self.tia)
            .field("riot", &self.riot)
            .field("board", &self.board.as_ref().map(|_| "<dyn Board>"))
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

impl Bus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        
        // 6502 open bus behavior: typically the last value on the data bus is returned.
        // We will read the mapped component and if it's open bus, we return self.open_bus.
        
        let val = if addr & 0x1000 != 0 {
            // A12 = 1 -> Cartridge
            if let Some(board) = &mut self.board {
                board.cpu_read(addr)
            } else {
                self.open_bus
            }
        } else {
            // A12 = 0 -> Console
            if addr & 0x0080 == 0 {
                // A7 = 0 -> TIA
                self.tia.cpu_read(addr)
            } else if addr & 0x0200 == 0 {
                // A7 = 1, A9 = 0 -> RIOT RAM
                self.riot.cpu_read(addr)
            } else {
                // A7 = 1, A9 = 1 -> RIOT I/O and Timers
                self.riot.cpu_read(addr)
            }
        };
        
        self.open_bus = val;
        val
    }

    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        let addr = addr & 0x1FFF;
        self.open_bus = val;
        
        if addr & 0x1000 != 0 {
            // A12 = 1 -> Cartridge
            if let Some(board) = &mut self.board {
                board.cpu_write(addr, val);
            }
        } else {
            // A12 = 0 -> Console
            if addr & 0x0080 == 0 {
                // A7 = 0 -> TIA
                self.tia.cpu_write(addr, val);
            } else if addr & 0x0200 == 0 {
                // A7 = 1, A9 = 0 -> RIOT RAM
                self.riot.cpu_write(addr, val);
            } else {
                // A7 = 1, A9 = 1 -> RIOT I/O and Timers
                self.riot.cpu_write(addr, val);
            }
        }
    }
}

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        self.cpu_read(addr)
    }
    
    fn write(&mut self, addr: u16, val: u8) {
        self.cpu_write(addr, val)
    }
}

pub trait VideoBus {
    fn video_read(&mut self, addr: u16) -> u8;
}

pub trait AudioBus {
    fn audio_sample(&self) -> u8;
}

impl AudioBus for Bus {
    fn audio_sample(&self) -> u8 {
        self.tia.audio.sample()
    }
}
"""

with open("crates/rusty2600-core/src/bus.rs", "w") as f:
    f.write(code)
print("Updated BUS")
