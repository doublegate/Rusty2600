//! `RetroAchievements` flat-address -> Atari 2600 CPU-bus address mapping.
//!
//! `RetroAchievements` addresses Atari 2600 memory as a flat space matching
//! the RIOT's 128 bytes of RAM directly: `0x0000..=0x007F` -> CPU bus
//! `$0080..=$00FF` (the console's ONLY RAM — there is no cartridge WRAM
//! window on the 2600 the way there is on the NES). Anything outside that
//! window has no bus equivalent for achievement purposes and maps to
//! `None` (the trampoline reports a short read, which rcheevos treats as
//! the address being invalid past that point).
//!
//! This is kept here, pure and unit-tested, so the memory source stays
//! agnostic: callers supply a `FnMut(u16) -> u8` peeking the CPU bus (e.g.
//! `rusty2600_core::Bus::peek`) and never need to know the RA layout.

/// Size of the 2600's only RAM (the RIOT's 128 bytes).
const RIOT_RAM_LEN: u32 = 0x80;
/// Base of the RIOT RAM window on the 2600 CPU bus.
const RIOT_RAM_BASE: u16 = 0x0080;

/// Translate a `RetroAchievements` flat address to a Rusty2600 CPU-bus address.
///
/// Returns `None` for addresses beyond the RIOT's 128 bytes of RAM — the
/// 2600's only mutable game-state RAM, and so the only thing RA addresses
/// on this console.
#[must_use]
pub const fn ra_addr_to_riot(addr: u32) -> Option<u16> {
    if addr < RIOT_RAM_LEN {
        #[allow(clippy::cast_possible_truncation)]
        let offset = addr as u16;
        Some(RIOT_RAM_BASE + offset)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riot_ram_maps_into_the_0080_window() {
        assert_eq!(ra_addr_to_riot(0x00), Some(0x0080));
        assert_eq!(ra_addr_to_riot(0x01), Some(0x0081));
        assert_eq!(ra_addr_to_riot(0x7F), Some(0x00FF));
    }

    #[test]
    fn out_of_range_is_none() {
        assert_eq!(ra_addr_to_riot(0x80), None);
        assert_eq!(ra_addr_to_riot(0xFFFF_FFFF), None);
    }
}
