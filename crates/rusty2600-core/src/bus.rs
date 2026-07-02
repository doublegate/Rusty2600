//! The Bus owns everything mutable.

use alloc::vec::Vec;

use rusty2600_cart::Board;
use rusty2600_cpu::CpuBus;
use rusty2600_riot::Riot;
use rusty2600_tia::Tia;

/// A single CPU write observed by the debugger's optional [`WriteLog`],
/// tagged with the TIA beam position at the moment it happened.
#[derive(Debug, Clone, Copy)]
pub struct WriteEvent {
    /// The scanline the write landed on.
    pub scanline: u16,
    /// The color clock within that scanline.
    pub color_clock: u16,
    /// The 13-bit CPU address written to.
    pub addr: u16,
    /// The byte written.
    pub value: u8,
}

/// The debugger's optional per-write log (`rusty2600-frontend`'s
/// `debug-hooks` feature enables it while the debugger overlay is open).
///
/// Disabled by default — near-zero cost when off (one `bool` check per
/// write). Deliberately `#[serde(skip)]` on [`Bus`]: this is debug-tooling
/// state, not part of the emulator's real state, and must never end up in a
/// save-state (see `docs/adr/0007-save-state-versioning.md`).
#[derive(Debug, Default, Clone)]
pub struct WriteLog {
    /// Whether writes are currently being recorded.
    pub enabled: bool,
    events: Vec<WriteEvent>,
}

impl WriteLog {
    /// Caps memory even if a caller forgets to [`Self::clear`] between
    /// frames — the oldest event is dropped once this many are held.
    const MAX_EVENTS: usize = 4096;

    /// The events recorded since the last [`Self::clear`], oldest first.
    #[must_use]
    pub fn events(&self) -> &[WriteEvent] {
        &self.events
    }

    /// Drops all recorded events (called once per frame by the debugger).
    pub fn clear(&mut self) {
        self.events.clear();
    }

    fn record(&mut self, ev: WriteEvent) {
        if !self.enabled {
            return;
        }
        if self.events.len() >= Self::MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(ev);
    }
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
/// The main system bus for Rusty2600, holding the chips.
pub struct Bus {
    /// The TIA video/audio chip.
    pub tia: Tia,
    /// The RIOT RAM/Timer/IO chip.
    pub riot: Riot,
    /// The cartridge board (mapper).
    pub board: Option<rusty2600_cart::Cartridge>,
    /// Open bus value (last driven value).
    pub open_bus: u8,
    /// The debugger's optional write log — see [`WriteLog`].
    #[serde(skip)]
    pub write_log: WriteLog,
}

impl core::fmt::Debug for Bus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bus")
            .field("tia", &self.tia)
            .field("riot", &self.riot)
            .field("board", &self.board.as_ref().map(|_| "<Cartridge>"))
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

impl Bus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Side-effect-free read, for debugger/tooling use only: a real
    /// `cpu_read` can trigger bankswitch hotspots, RIOT's INTIM
    /// read-clears-underflow-flag behavior, and cart `snoop_read` side
    /// effects, none of which a memory-viewer peek should ever cause. Reads
    /// via a full clone of `self` (cheap relative to a UI refresh cadence,
    /// and correctly avoids the `unsafe`-free crate's inability to alias a
    /// `&mut` for a "no-op" read) so the real system state is untouched.
    ///
    /// For more than a byte or two, prefer [`Self::peek_range`] — it clones
    /// `self` ONCE and reads every byte from that one clone, instead of
    /// paying a full `Bus` clone (TIA + RIOT + the cart's ROM/RAM) per byte.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.clone().cpu_read(addr)
    }

    /// Side-effect-free read of `len` consecutive addresses starting at
    /// `base` (wrapping at 16 bits), for a debugger memory viewer or a
    /// disassembly window. Clones `self` once, then reads every byte from
    /// that single clone — any bankswitch hotspot triggered by reading byte
    /// N is visible to byte N+1's read (an honest reflection of "whatever
    /// the bank state currently is," same caveat any bank-switched-system
    /// memory viewer has), but the REAL system is never touched.
    #[must_use]
    pub fn peek_range(&self, base: u16, len: u16) -> alloc::vec::Vec<u8> {
        let mut clone = self.clone();
        (0..len)
            .map(|i| clone.cpu_read(base.wrapping_add(i)))
            .collect()
    }

    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;

        // 6502 open bus behavior: typically the last value on the data bus is returned.
        // We will read the mapped component and if it's open bus, we return self.open_bus.

        let val = if addr & 0x1000 != 0 {
            // A12 = 1 -> Cartridge
            if let Some(board) = &mut self.board {
                let val = board.cpu_read(addr);
                self.apply_oob_pokes();
                val
            } else {
                self.open_bus
            }
        } else {
            // A12 = 0 -> Console
            let val = if addr & 0x0080 == 0 {
                // A7 = 0 -> TIA
                self.tia.cpu_read(addr)
            } else if addr & 0x0200 == 0 {
                // A7 = 1, A9 = 0 -> RIOT RAM
                self.riot.cpu_read(addr)
            } else {
                // A7 = 1, A9 = 1 -> RIOT I/O and Timers
                self.riot.cpu_read(addr)
            };
            // See snoop_write's rationale: UA/0840/FE bankswitch on reads
            // the console routes to TIA/RIOT, observing the resulting value
            // (never redirecting it).
            if let Some(board) = &mut self.board {
                board.snoop_read(addr, val);
            }
            val
        };

        self.open_bus = val;
        val
    }

    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        let addr = addr & 0x1FFF;
        self.open_bus = val;

        self.write_log.record(WriteEvent {
            scanline: self.tia.scanline,
            color_clock: self.tia.color_clock,
            addr,
            value: val,
        });

        if addr & 0x1000 != 0 {
            // A12 = 1 -> Cartridge
            if let Some(board) = &mut self.board {
                board.cpu_write(addr, val);
                self.apply_oob_pokes();
            }
        } else {
            // A12 = 0 -> Console. Real cart edge connectors are wired to every
            // address line, not just A12: 3F/3E/UA/0840/FE all bankswitch on
            // writes the console routes to TIA/RIOT, so the board gets a
            // look too (default no-op for the overwhelming majority of boards
            // that only care about their own $1000+ window).
            if let Some(board) = &mut self.board {
                board.snoop_write(addr, val);
            }
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

    /// Apply any out-of-band RIOT-RAM pokes the board staged this access
    /// (see `rusty2600_cart::Board::take_oob_pokes`'s doc comment — used by
    /// `BankAr`'s dummy-BIOS load handoff). Bypasses `Riot::cpu_write`
    /// deliberately: these are direct RAM patches with no console-visible
    /// bus cycle of their own, mirroring Stella's `System::pokeOob`.
    fn apply_oob_pokes(&mut self) {
        if let Some(board) = &mut self.board {
            for (addr, val) in board.take_oob_pokes() {
                self.riot.ram[(addr & 0x7F) as usize] = val;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_does_not_mutate_riot_ram() {
        let mut bus = Bus::new();
        bus.cpu_write(0x0080, 0x42);
        let before = bus.riot.ram;
        assert_eq!(bus.peek(0x0080), 0x42);
        assert_eq!(bus.riot.ram, before, "peek must not mutate RIOT RAM");
    }

    #[test]
    fn peek_does_not_advance_cart_bank_state() {
        // Plain F8 (8 KiB, 2x4K banks); default bank is 1 (the last bank).
        let mut rom = [0u8; 0x2000];
        rom[0x0000] = 0x11; // bank 0, offset 0
        rom[0x1000] = 0x22; // bank 1, offset 0
        let mut bus = Bus::new();
        bus.board = rusty2600_cart::detect(&rom);
        assert_eq!(bus.peek(0x1000), 0x22, "starts on bank 1 (the default)");
        bus.peek(0x1FF8); // would select bank 0 if peek had side effects
        assert_eq!(
            bus.peek(0x1000),
            0x22,
            "peek must not trigger bankswitch hotspots"
        );
    }

    #[test]
    fn write_log_disabled_by_default_records_nothing() {
        let mut bus = Bus::new();
        bus.cpu_write(0x00, 0x02); // VSYNC
        assert!(bus.write_log.events().is_empty());
    }

    #[test]
    fn write_log_records_when_enabled() {
        let mut bus = Bus::new();
        bus.write_log.enabled = true;
        bus.cpu_write(0x06, 0xAB); // COLUP0
        let events = bus.write_log.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].addr, 0x06);
        assert_eq!(events[0].value, 0xAB);
    }

    #[test]
    fn write_log_clear_drops_all_events() {
        let mut bus = Bus::new();
        bus.write_log.enabled = true;
        bus.cpu_write(0x06, 0xAB);
        bus.cpu_write(0x07, 0xCD);
        bus.write_log.clear();
        assert!(bus.write_log.events().is_empty());
    }

    #[test]
    fn write_log_caps_at_max_events() {
        let mut bus = Bus::new();
        bus.write_log.enabled = true;
        for i in 0..5000u16 {
            bus.cpu_write(0x06, u8::try_from(i % 256).unwrap_or(0));
        }
        assert_eq!(bus.write_log.events().len(), WriteLog::MAX_EVENTS);
    }
}
