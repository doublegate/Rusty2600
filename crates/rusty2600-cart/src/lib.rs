//! `rusty2600-cart` — the cartridge bankswitch-board model.
//!
//! The VCS address bus only exposes a 4 KiB cartridge window (`$1000..=$1FFF`).
//! Anything bigger than 4 KiB — and a great many "exactly 4 KiB plus extra RAM
//! or a coprocessor" carts — bankswitches by watching for reads/writes of magic
//! "hotspot" addresses inside that window. There are DOZENS of schemes, so they
//! are **tiered** for honesty (see [`Tier`] + ADR 0003 / `docs/cart.md`):
//!
//! - `2K` (mirrored `4K`), `4K` plain — `Core` (zero board-specific hotspot
//!   logic; every hotspot-driven scheme below is `Curated` or `BestEffort`,
//!   never `Core`).
//! - `CV` (Commavid), `F8`/`F6`/`F4` (Atari standard-bank), `FA`/`CBS RAM+`,
//!   Superchip (`F8SC`/`F6SC`/`F4SC`, `+128 B` on-cart RAM) — `Curated`.
//! - `E0` (Parker Bros), `E7` (M-network), `FE` (Activision `SCABS`), `3F`
//!   (Tigervision), `3E` (Boulder Dash) / `3E+`, `DPC` (Pitfall II), `DPC+` /
//!   `CDF`/`CDFJ`, and the remaining long tail — `BestEffort` until each gets
//!   a redistributable fixture + register-decode tests (see `docs/cart.md`'s
//!   scheme catalogue for the authoritative per-scheme tier).
//!
//! Part of the one-directional chip-crate graph (see `docs/architecture.md`):
//! this crate has NO video / audio / cpu dependency (the TIA's memory bus
//! depends on *this* crate, not the reverse). It is `no_std` plus `alloc` so it
//! cross-compiles to a bare-metal target; only the frontend carries `std` and
//! `unsafe`.

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

mod serde_bytes_array;

use alloc::boxed::Box;

/// Accuracy-evidence tier for a supported bankswitch scheme.
///
/// The tier is an **honesty marker**, not a behavioural one: runtime behaviour
/// is identical regardless of tier — it records only how much external evidence
/// backs the board's correctness, so accuracy claims stay precise as the
/// long-tail scheme set grows. The honesty gate (`tests/mapper_tier_honesty.rs`)
/// forbids a `BestEffort` board ever backing the accuracy oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Tier {
    /// Spec-implemented + oracle-gated (`AccuracyCoin`-equivalent / commercial
    /// ROM byte-identity). The bedrock schemes.
    Core,
    /// Long-tail board added with concrete game demand plus a redistributable
    /// fixture or spec; register-decode unit-tested and boot-smoked.
    Curated,
    /// Reference-ported long-tail with no redistributable fixture;
    /// register-decode tested only, and **structurally never accuracy-gated**.
    BestEffort,
}

impl Tier {
    /// Human-readable tier name (docs generation, UI badges, logs).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Curated => "Curated",
            Self::BestEffort => "BestEffort",
        }
    }

    /// Whether this tier is covered by the accuracy oracle gate. `Core` and
    /// `Curated` are; `BestEffort` is not. This is the load-bearing predicate
    /// the honesty gate asserts.
    #[must_use]
    pub const fn is_accuracy_gated(self) -> bool {
        matches!(self, Self::Core | Self::Curated)
    }
}

/// A cartridge bankswitch board.
///
/// The board owns the ROM image (and any on-cart RAM / coprocessor) and decodes
/// every CPU access in the `$1000..=$1FFF` cartridge window. Schemes switch
/// banks by detecting "hotspot" addresses on either a read or a write, so both
/// `cpu_read` and `cpu_write` must run the bank logic even when the access is
/// nominally a fetch.
pub trait Board {
    /// CPU-side read through the board's current bank mapping. `addr` is the
    /// full 13-bit CPU address; the board masks to its window. Must also apply
    /// any read-triggered hotspot bank switch (e.g. F8's `$1FF8/$1FF9`).
    fn cpu_read(&mut self, addr: u16) -> u8;

    /// CPU-side write. Drives write-triggered hotspots (3F/3E bank registers,
    /// Superchip RAM writes) and on-cart RAM.
    fn cpu_write(&mut self, addr: u16, val: u8);

    /// This board's accuracy tier (the honesty marker).
    fn tier(&self) -> Tier;

    /// Per-CPU-cycle coprocessor hook for DPC-style boards (the DPC/DPC+ music
    /// fetchers + the ARM in DPC+ advance on the CPU clock). Default no-op so
    /// the overwhelming majority of plain-ROM boards pay nothing.
    fn tick(&mut self) {}

    /// Drive any coprocessor that is clocked independently of the CPU (e.g. the
    /// DPC random-number / oscillator clock). Default no-op.
    fn tick_coprocessor(&mut self) {}
}

/// 2 KiB ROM, mirrored into the upper half of the 4 KiB window. Core tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Rom2K {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x0800],
}

impl Rom2K {
    /// Build from a 2 KiB image. Returns `None` if the slice is not 2 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x0800] = rom.try_into().ok()?;
        Some(Self { rom: bytes })
    }
}

impl Board for Rom2K {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.rom[(addr & 0x07FF) as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {
        // ROM-only board: writes are ignored (open bus).
    }
    fn tier(&self) -> Tier {
        Tier::Core
    }
}

/// 4 KiB ROM, the full unbanked cartridge window. Core tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Rom4K {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x1000],
}

impl Rom4K {
    /// Build from a 4 KiB image. Returns `None` if the slice is not 4 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x1000] = rom.try_into().ok()?;
        Some(Self { rom: bytes })
    }
}

impl Board for Rom4K {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.rom[(addr & 0x0FFF) as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::Core
    }
}

/// Atari F8: 8 KiB ROM as two 4 KiB banks, switched by accessing the hotspots
/// `$1FF8` (select bank 0) / `$1FF9` (select bank 1). Curated tier (matches
/// `docs/cart.md` and the research-report tier split; `T-0401-008`
/// reconciled the earlier stray `Core` placement).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankF8 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x2000],
    bank: u8,
}

impl BankF8 {
    /// The F8 bank-0 / bank-1 select hotspots.
    const HOTSPOT_BANK0: u16 = 0x1FF8;
    const HOTSPOT_BANK1: u16 = 0x1FF9;

    /// Build from an 8 KiB image. Returns `None` if the slice is not 8 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x2000] = rom.try_into().ok()?;
        // Power-on bank is unspecified by hardware; reset vector lives in the
        // last bank, so games arrange for that bank to be selected after RESET.
        Some(Self {
            rom: bytes,
            bank: 1,
        })
    }

    /// Apply a hotspot if `addr` is one of the bank-select addresses. Both reads
    /// and writes trigger it, so this is called from both access paths.
    // Not `const`: real F8 boards add Superchip-RAM windows + a wider hotspot
    // map here, so const-ness would have to be reverted.
    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1FFF {
            Self::HOTSPOT_BANK0 => self.bank = 0,
            Self::HOTSPOT_BANK1 => self.bank = 1,
            _ => {}
        }
    }
}

impl Board for BankF8 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let off = usize::from(self.bank) * 0x1000 + (addr & 0x0FFF) as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// Atari F6: 16 KiB ROM as four 4 KiB banks, switched by accessing the hotspots
/// `$1FF6` through `$1FF9`. Curated tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankF6 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x4000],
    bank: u8,
}

impl BankF6 {
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x4000] = rom.try_into().ok()?;
        Some(Self {
            rom: bytes,
            bank: 3,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1FFF {
            0x1FF6 => self.bank = 0,
            0x1FF7 => self.bank = 1,
            0x1FF8 => self.bank = 2,
            0x1FF9 => self.bank = 3,
            _ => {}
        }
    }
}

impl Board for BankF6 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let off = usize::from(self.bank) * 0x1000 + (addr & 0x0FFF) as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// Atari F4: 32 KiB ROM as eight 4 KiB banks, switched by accessing the hotspots
/// `$1FF4` through `$1FFB`. Curated tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankF4 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x8000],
    bank: u8,
}

impl BankF4 {
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x8000] = rom.try_into().ok()?;
        Some(Self {
            rom: bytes,
            bank: 7,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1FFF {
            0x1FF4 => self.bank = 0,
            0x1FF5 => self.bank = 1,
            0x1FF6 => self.bank = 2,
            0x1FF7 => self.bank = 3,
            0x1FF8 => self.bank = 4,
            0x1FF9 => self.bank = 5,
            0x1FFA => self.bank = 6,
            0x1FFB => self.bank = 7,
            _ => {}
        }
    }
}

impl Board for BankF4 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let off = usize::from(self.bank) * 0x1000 + (addr & 0x0FFF) as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// CommaVid `CV`: 2 KiB ROM + 1 KiB on-cart RAM, no bank switching (a single
/// fixed 4 KiB window). Curated tier.
///
/// Address map within the `$1000..=$1FFF` window (confirmed against Stella's
/// `CartCV`/`CartEnhanced`, `RAM_HIGH_WP = true` i.e. the write port is the
/// numerically-higher mirror):
/// - `$1000..=$13FF` — RAM **read** port (1 KiB, mirrors the same RAM the
///   write port below addresses).
/// - `$1400..=$17FF` — RAM **write** port.
/// - `$1800..=$1FFF` — 2 KiB ROM (mirrored if the source image is smaller).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankCV {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x0800],
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x0400],
}

impl BankCV {
    /// Build from a 2 KiB ROM-only image, or a 4 KiB image whose first 2 KiB
    /// is initial RAM content (the "MagiCard saved program listing" case
    /// Stella's `CartCV` also supports) and second 2 KiB is the real ROM.
    /// Returns `None` for any other size.
    #[must_use]
    pub fn new(image: &[u8]) -> Option<Self> {
        match image.len() {
            0x0800 => Some(Self {
                rom: image.try_into().ok()?,
                ram: [0; 0x0400],
            }),
            0x1000 => {
                let mut ram = [0u8; 0x0400];
                ram.copy_from_slice(&image[..0x0400]);
                Some(Self {
                    rom: image[0x0800..0x1000].try_into().ok()?,
                    ram,
                })
            }
            _ => None,
        }
    }
}

impl Board for BankCV {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr & 0x1FFF {
            a @ 0x1000..=0x13FF => self.ram[(a & 0x03FF) as usize],
            0x1400..=0x1FFF => {
                let a = addr & 0x1FFF;
                if a < 0x1800 {
                    // The write port ($1400-$17FF) reads back open-bus-ish;
                    // real hardware doesn't drive a defined value here, so
                    // return 0 rather than fabricate RAM contents.
                    0
                } else {
                    self.rom[(a & 0x07FF) as usize]
                }
            }
            _ => unreachable!(),
        }
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        if let 0x1400..=0x17FF = addr & 0x1FFF {
            self.ram[(addr & 0x03FF) as usize] = val;
        }
    }
    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// CBS's `FA` ("RAM Plus"): 12 KiB ROM as three 4 KiB banks (`$1FF8`/`$1FF9`/
/// `$1FFA`), plus 256 B on-cart RAM. Curated tier.
///
/// Confirmed against Stella's `CartFA`/`CartEnhanced` (`RAM_SIZE = 0x100`,
/// `RAM_HIGH_WP` unset so it takes the base class default of `false` — write
/// port is the numerically-LOWER mirror, unlike `CV`): `$1000..=$10FF` is the
/// RAM **write** port, `$1100..=$11FF` is the RAM **read** port, both inside
/// whichever 4 KiB bank is currently selected (the RAM overlays the low 256 B
/// of ROM in that bank — real hardware, the ROM underneath is simply
/// unreachable there).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankFA {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x3000],
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x100],
    bank: u8,
}

impl BankFA {
    const HOTSPOT_BASE: u16 = 0x1FF8;

    /// Build from a 12 KiB image. Returns `None` if the slice is not 12 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            ram: [0; 0x100],
            bank: 2,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        let a = addr & 0x1FFF;
        if (Self::HOTSPOT_BASE..=Self::HOTSPOT_BASE + 2).contains(&a) {
            self.bank = (a - Self::HOTSPOT_BASE) as u8;
        }
    }
}

impl Board for BankFA {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let a = addr & 0x0FFF;
        if (0x0100..0x0200).contains(&a) {
            self.ram[(a & 0x00FF) as usize]
        } else {
            self.rom[usize::from(self.bank) * 0x1000 + a as usize]
        }
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.hotspot(addr);
        let a = addr & 0x0FFF;
        if a < 0x0100 {
            self.ram[a as usize] = val;
        }
    }
    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// An enum wrapping all supported boards, enabling static dispatch and
/// `no_std`-compatible serialization without trait objects.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Cartridge {
    /// 2 KiB core mapper
    Rom2K(Rom2K),
    /// 4 KiB core mapper
    Rom4K(Rom4K),
    /// F8 bankswitched mapper (Curated tier)
    BankF8(BankF8),
    /// F6 bankswitched mapper
    BankF6(BankF6),
    /// F4 bankswitched mapper
    BankF4(BankF4),
    /// CommaVid CV: 2 KiB ROM + 1 KiB RAM, no bank switching (Curated tier)
    BankCV(BankCV),
    /// CBS FA/RAM Plus: 12 KiB ROM + 256 B RAM (Curated tier)
    BankFA(BankFA),
}

impl Board for Cartridge {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match self {
            Self::Rom2K(b) => b.cpu_read(addr),
            Self::Rom4K(b) => b.cpu_read(addr),
            Self::BankF8(b) => b.cpu_read(addr),
            Self::BankF6(b) => b.cpu_read(addr),
            Self::BankF4(b) => b.cpu_read(addr),
            Self::BankCV(b) => b.cpu_read(addr),
            Self::BankFA(b) => b.cpu_read(addr),
        }
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        match self {
            Self::Rom2K(b) => b.cpu_write(addr, val),
            Self::Rom4K(b) => b.cpu_write(addr, val),
            Self::BankF8(b) => b.cpu_write(addr, val),
            Self::BankF6(b) => b.cpu_write(addr, val),
            Self::BankF4(b) => b.cpu_write(addr, val),
            Self::BankCV(b) => b.cpu_write(addr, val),
            Self::BankFA(b) => b.cpu_write(addr, val),
        }
    }

    fn tier(&self) -> Tier {
        match self {
            Self::Rom2K(b) => b.tier(),
            Self::Rom4K(b) => b.tier(),
            Self::BankCV(b) => b.tier(),
            Self::BankF8(b) => b.tier(),
            Self::BankF6(b) => b.tier(),
            Self::BankF4(b) => b.tier(),
            Self::BankFA(b) => b.tier(),
        }
    }

    fn tick(&mut self) {
        match self {
            Self::Rom2K(b) => b.tick(),
            Self::Rom4K(b) => b.tick(),
            Self::BankF8(b) => b.tick(),
            Self::BankF6(b) => b.tick(),
            Self::BankF4(b) => b.tick(),
            Self::BankCV(b) => b.tick(),
            Self::BankFA(b) => b.tick(),
        }
    }

    fn tick_coprocessor(&mut self) {
        match self {
            Self::Rom2K(b) => b.tick_coprocessor(),
            Self::Rom4K(b) => b.tick_coprocessor(),
            Self::BankF8(b) => b.tick_coprocessor(),
            Self::BankF6(b) => b.tick_coprocessor(),
            Self::BankF4(b) => b.tick_coprocessor(),
            Self::BankCV(b) => b.tick_coprocessor(),
            Self::BankFA(b) => b.tick_coprocessor(),
        }
    }
}

/// Detect the bankswitch scheme from a ROM image and build the board.
///
/// Today this only resolves the Core-tier sized boards by length; the full
/// scheme catalogue (hotspot-pattern + DB-assisted detection) lands later. Each
/// unimplemented branch is annotated with its INTENDED tier so the honesty gate
/// stays truthful as boards land.
///
/// Returns `None` for an unrecognized size / scheme.
#[must_use]
pub fn detect(rom: &[u8]) -> Option<Cartridge> {
    match rom.len() {
        // 2 KiB / 4 KiB: default to plain ROM (Core). `BankCV` (CommaVid) is
        // the SAME two sizes (2 KiB ROM-only, or 4 KiB "2K RAM-image + 2K
        // ROM") — real disambiguation needs a ROM-DB / hotspot-access-pattern
        // check (CV never strobes a bankswitch hotspot at all, so there's no
        // hotspot signature to look for; only usage — e.g. actually reading
        // from $1000-$13FF or writing to $1400-$17FF during boot — would
        // out it). Defaulting to plain ROM here is deliberately the SAFE
        // choice: CommaVid only shipped 2 known titles (Magicard,
        // Video Life), so misdetecting the overwhelmingly-more-common
        // plain-ROM case would be far worse than the reverse.
        // TODO(T-0401-009): ROM-DB-assisted CV detection for 2K/4K images.
        0x0800 => Rom2K::new(rom).map(Cartridge::Rom2K),
        0x1000 => Rom4K::new(rom).map(Cartridge::Rom4K),
        0x2000 => {
            // 8 KiB: default to F8 (Curated). Disambiguation from E0 (Parker Bros,
            // BestEffort) / FE (Activision SCABS, BestEffort) / 3F-with-8K
            // (Tigervision, BestEffort) needs hotspot-pattern + ROM-DB detection.
            // TODO(T-0401-001): E0 / FE / 3F (BestEffort) detection for 8 KiB images.
            BankF8::new(rom).map(Cartridge::BankF8)
        }
        // 12 KiB: CBS's FA/RAM Plus (Curated) — this size is unambiguous
        // (nothing else in the catalogue is 12 KiB), so no disambiguation
        // needed. NOTE: an earlier version of this comment incorrectly said
        // "E7" here — E7 is 16 KiB (docs/cart.md), not 12; fixed.
        0x3000 => BankFA::new(rom).map(Cartridge::BankFA),
        0x4000 => {
            // 16 KiB: default to F6 (Curated, the far more common Atari-
            // standard scheme — dozens of Atari-published titles). E7
            // (M-Network, also Curated) is the SAME size and needs hotspot-
            // pattern / ROM-DB disambiguation, same class of ambiguity as
            // the 8 KiB F8/E0/FE/3F case above.
            // TODO(T-0401-002): E7 detection for 16 KiB images.
            BankF6::new(rom).map(Cartridge::BankF6)
        }
        0x8000 => BankF4::new(rom).map(Cartridge::BankF4),
        // TODO(T-0401-003): Superchip variants F8SC/F6SC/F4SC (+128 B RAM, Curated).
        // TODO(T-0401-004): 3F (Tigervision) / 3E (Boulder Dash) / 3E+ (BestEffort).
        // TODO(T-0401-005): DPC (Pitfall II, Curated) via tick_coprocessor.
        // TODO(T-0401-006): DPC+ (BestEffort) via tick_coprocessor.
        // TODO(T-0401-007): pirate / homebrew BMC schemes (BestEffort).
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rom4k_reads_window() {
        let mut img = [0u8; 0x1000];
        img[0x0FFF] = 0xAB;
        let mut board = Rom4K::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1FFF), 0xAB);
        assert_eq!(board.tier(), Tier::Core);
    }

    #[test]
    fn rom2k_mirrors() {
        let mut img = [0u8; 0x0800];
        img[0x0000] = 0xCD;
        let mut board = Rom2K::new(&img).unwrap();
        // Both halves of the 4 KiB window read the same 2 KiB image.
        assert_eq!(board.cpu_read(0x1000), 0xCD);
        assert_eq!(board.cpu_read(0x1800), 0xCD);
    }

    #[test]
    fn f8_switches_on_hotspot() {
        let mut img = [0u8; 0x2000];
        img[0x0000] = 0x11; // bank 0, offset 0
        img[0x1000] = 0x22; // bank 1, offset 0
        let mut board = BankF8::new(&img).unwrap();
        board.cpu_read(0x1FF8); // select bank 0
        assert_eq!(board.cpu_read(0x1000), 0x11);
        board.cpu_read(0x1FF9); // select bank 1
        assert_eq!(board.cpu_read(0x1000), 0x22);
        assert_eq!(board.tier(), Tier::Curated);
    }

    #[test]
    fn detect_picks_sized_boards() {
        assert!(detect(&[0u8; 0x0800]).is_some());
        assert!(detect(&[0u8; 0x1000]).is_some());
        assert!(detect(&[0u8; 0x2000]).is_some());
        assert!(detect(&[0u8; 0x1234]).is_none());
    }

    #[test]
    fn besteffort_is_never_accuracy_gated() {
        assert!(Tier::Core.is_accuracy_gated());
        assert!(Tier::Curated.is_accuracy_gated());
        assert!(!Tier::BestEffort.is_accuracy_gated());
    }
    #[test]
    fn bankf6_hotspots() {
        let mut img = [0u8; 0x4000];
        img[0x0FFF] = 0xAA;
        img[0x1FFF] = 0xBB;
        img[0x2FFF] = 0xCC;
        img[0x3FFF] = 0xDD;
        let mut board = BankF6::new(&img).unwrap();
        // default bank is 3
        assert_eq!(board.cpu_read(0x1FFF), 0xDD);
        board.cpu_read(0x1FF6);
        assert_eq!(board.cpu_read(0x1FFF), 0xAA);
        board.cpu_read(0x1FF7);
        assert_eq!(board.cpu_read(0x1FFF), 0xBB);
        assert_eq!(board.tier(), Tier::Curated);
    }

    #[test]
    fn bankf4_hotspots() {
        let mut img = [0u8; 0x8000];
        img[0x0FFF] = 0xAA;
        img[0x7FFF] = 0xBB;
        let mut board = BankF4::new(&img).unwrap();
        // default bank is 7
        assert_eq!(board.cpu_read(0x1FFF), 0xBB);
        board.cpu_read(0x1FF4);
        assert_eq!(board.cpu_read(0x1FFF), 0xAA);
        assert_eq!(board.tier(), Tier::Curated);
    }

    #[test]
    fn cv_rom_and_ram_ports() {
        let mut img = [0u8; 0x0800];
        img[0x07FF] = 0x55; // last byte of the 2K ROM
        let mut board = BankCV::new(&img).unwrap();
        // ROM lives at $1800-$1FFF.
        assert_eq!(board.cpu_read(0x1FFF), 0x55);
        assert_eq!(board.tier(), Tier::Curated);

        // Write through the high ($1400-$17FF) port, read back through the
        // low ($1000-$13FF) port — same underlying 1 KiB RAM.
        board.cpu_write(0x1400, 0x42);
        assert_eq!(board.cpu_read(0x1000), 0x42);

        // The write port doesn't accept reads as RAM contents.
        board.cpu_write(0x1401, 0x99);
        assert_ne!(board.cpu_read(0x1401), 0x99);
    }

    #[test]
    fn cv_4k_image_seeds_initial_ram() {
        let mut img = [0u8; 0x1000];
        img[0x0000] = 0xAB; // initial RAM byte 0
        img[0x0800] = 0xCD; // first byte of the real 2K ROM half
        let mut board = BankCV::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1000), 0xAB);
        assert_eq!(board.cpu_read(0x1800), 0xCD);
    }

    #[test]
    fn fa_bank_switch_and_ram_ports() {
        let mut img = [0u8; 0x3000];
        img[0x1000 + 0x0FFF] = 0x11; // bank 1, last byte
        img[0x2000 + 0x0FFF] = 0x22; // bank 2, last byte (default bank)
        let mut board = BankFA::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::Curated);

        // Default bank is 2 (reset vector convention, matching BankF4/BankF6).
        assert_eq!(board.cpu_read(0x1FFF), 0x22);
        board.cpu_read(0x1FF9); // select bank 1
        assert_eq!(board.cpu_read(0x1FFF), 0x11);

        // RAM: write-low ($1000-$10FF), read-high ($1100-$11FF), same 256 B.
        board.cpu_write(0x1000, 0x77);
        assert_eq!(board.cpu_read(0x1100), 0x77);
    }

    #[test]
    fn detect_resolves_fa_and_e7_sized_ambiguity_defaults_to_f6() {
        let fa = detect(&[0u8; 0x3000]).unwrap();
        assert!(matches!(fa, Cartridge::BankFA(_)));
        // 16 KiB defaults to F6 until E7 gets ROM-DB disambiguation.
        let sixteen_k = detect(&[0u8; 0x4000]).unwrap();
        assert!(matches!(sixteen_k, Cartridge::BankF6(_)));
    }
}
