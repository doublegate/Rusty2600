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
    /// Superchip 128 B on-cart RAM (F8SC), enabled via [`Self::with_superchip`].
    /// Always allocated (128 B is negligible) so `Option<[u8; N]>` never needs
    /// its own serde impl; `superchip` gates whether it's actually addressed.
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
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
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the F8SC Superchip 128 B RAM overlay (confirmed against
    /// Stella's `CartF8`/`CartEnhanced`: write-low `$1000..=$107F`, read-high
    /// `$1080..=$10FF`, inside whichever bank is selected — same
    /// write-low/read-high convention as `BankFA`, half its RAM size).
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
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
        let a = addr & 0x0FFF;
        if self.superchip && (0x0080..0x0100).contains(&a) {
            return self.ram[(a & 0x007F) as usize];
        }
        let off = usize::from(self.bank) * 0x1000 + a as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.hotspot(addr);
        if self.superchip {
            let a = addr & 0x0FFF;
            if a < 0x0080 {
                self.ram[a as usize] = val;
            }
        }
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
    /// F6SC Superchip 128 B RAM overlay; see [`BankF8`]'s field of the same
    /// name for the write-low/read-high convention this follows.
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
}

impl BankF6 {
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x4000] = rom.try_into().ok()?;
        Some(Self {
            rom: bytes,
            bank: 3,
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the F6SC Superchip 128 B RAM overlay (see [`BankF8::with_superchip`]).
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
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
        let a = addr & 0x0FFF;
        if self.superchip && (0x0080..0x0100).contains(&a) {
            return self.ram[(a & 0x007F) as usize];
        }
        let off = usize::from(self.bank) * 0x1000 + a as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.hotspot(addr);
        if self.superchip {
            let a = addr & 0x0FFF;
            if a < 0x0080 {
                self.ram[a as usize] = val;
            }
        }
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
    /// F4SC Superchip 128 B RAM overlay; see [`BankF8`]'s field of the same
    /// name for the write-low/read-high convention this follows.
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
}

impl BankF4 {
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bytes: [u8; 0x8000] = rom.try_into().ok()?;
        Some(Self {
            rom: bytes,
            bank: 7,
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the F4SC Superchip 128 B RAM overlay (see [`BankF8::with_superchip`]).
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
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
        let a = addr & 0x0FFF;
        if self.superchip && (0x0080..0x0100).contains(&a) {
            return self.ram[(a & 0x007F) as usize];
        }
        let off = usize::from(self.bank) * 0x1000 + a as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.hotspot(addr);
        if self.superchip {
            let a = addr & 0x0FFF;
            if a < 0x0080 {
                self.ram[a as usize] = val;
            }
        }
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

/// E7 (M-Network): 16 KiB ROM as eight 2 KiB banks + 2 KiB on-cart RAM, the
/// most complex classic bankswitch scheme (Kevin Horton's description, cited
/// verbatim in Stella's `CartE7.hxx` and followed here). The CPU's 4 KiB
/// window splits into two 2 KiB segments:
///
/// - **Lower** (`$1000..=$17FF`, selectable via `$1FE0..=$1FE7`): banks 0-6
///   are plain ROM; bank **7 means "switch to RAM instead"** (not real bank
///   7 ROM data) — `$1000..=$13FF` write port / `$1400..=$17FF` read port,
///   both aliasing the SAME underlying 1 KiB (matching this crate's existing
///   write-low/read-high convention for [`BankFA`]/[`BankCV`]).
/// - **Upper** (`$1800..=$1FFF`): NOT a single fixed bank — `$1800..=$19FF`
///   is a separate, always-active 256 B RAM window (write `$1800..=$18FF` /
///   read `$1900..=$19FF`, aliased the same way) selected via
///   `$1FE8..=$1FEB` (one of 4 sub-banks); `$1A00..=$1FFF` is the true fixed
///   region, always the LAST 2 KiB bank's ROM data (so the reset vector is
///   always reachable regardless of which lower bank is selected).
///
/// Curated tier. Only the 16 KiB / 8-bank configuration is implemented (the
/// only one `docs/cart.md` commits to); Stella also supports rarer 8 KiB /
/// 12 KiB / 6-bank M-Network variants, out of scope here. **Not wired into
/// `detect()`**: 16 KiB is the SAME size as [`BankF6`], so — like
/// Superchip — this needs ROM-DB/hotspot-pattern disambiguation
/// (`T-0401-009`) before automatic dispatch is safe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankE7 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x4000],
    /// The lower segment's 1 KiB RAM (active only when `bank0 == RAM_BANK`).
    #[serde(with = "serde_bytes_array")]
    ram_big: [u8; 0x400],
    /// The always-active 256 B-per-sub-bank RAM pool (4 sub-banks, 1 KiB total).
    #[serde(with = "serde_bytes_array")]
    ram_small: [u8; 0x400],
    /// Lower-segment bank select, 0-6 = ROM bank, 7 = RAM mode.
    bank0: u8,
    /// Which of the 4 always-active 256 B RAM sub-banks is mapped in.
    ram_bank: u8,
}

impl BankE7 {
    /// The pseudo-bank-index meaning "RAM instead of ROM" in the lower segment.
    const RAM_BANK: u8 = 7;
    const HOTSPOT_ROM_BASE: u16 = 0x1FE0;
    const HOTSPOT_RAM_BASE: u16 = 0x1FE8;

    /// Build from a 16 KiB image. Returns `None` if the slice is not 16 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            ram_big: [0; 0x400],
            ram_small: [0; 0x400],
            bank0: 0,
            ram_bank: 0,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn check_switch_bank(&mut self, addr: u16) {
        let a = addr & 0x1FFF;
        if (Self::HOTSPOT_ROM_BASE..=Self::HOTSPOT_ROM_BASE + 7).contains(&a) {
            self.bank0 = (a - Self::HOTSPOT_ROM_BASE) as u8;
        } else if (Self::HOTSPOT_RAM_BASE..=Self::HOTSPOT_RAM_BASE + 3).contains(&a) {
            self.ram_bank = (a - Self::HOTSPOT_RAM_BASE) as u8;
        }
    }
}

impl Board for BankE7 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.check_switch_bank(addr);
        let a = addr & 0x0FFF;
        if a < 0x0800 {
            if self.bank0 == Self::RAM_BANK {
                self.ram_big[(a & 0x03FF) as usize]
            } else {
                self.rom[usize::from(self.bank0) * 0x800 + a as usize]
            }
        } else if a < 0x0A00 {
            self.ram_small[usize::from(self.ram_bank) * 0x100 + (a & 0x00FF) as usize]
        } else {
            self.rom[Self::RAM_BANK as usize * 0x800 + (a & 0x07FF) as usize]
        }
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.check_switch_bank(addr);
        let a = addr & 0x0FFF;
        if a < 0x0400 && self.bank0 == Self::RAM_BANK {
            self.ram_big[a as usize] = val;
        } else if (0x0800..0x0900).contains(&a) {
            self.ram_small[usize::from(self.ram_bank) * 0x100 + (a & 0x00FF) as usize] = val;
        }
        // $1400-$17FF (RAM read-port alias), $1900-$19FF (RAM read-port
        // alias), and $1A00-$1FFF (fixed ROM) writes are all no-ops.
    }

    fn tier(&self) -> Tier {
        Tier::Curated
    }
}

/// DPC (Pitfall II's "Display Processor Chip"): 8 KiB program ROM as two
/// 4 KiB F8-style banks (`$1FF8`/`$1FF9` hotspots, same convention as
/// [`BankF8`]) + a 2 KiB fixed display-data ROM + 8 hardware "data fetchers"
/// + an LFSR random-number generator, all memory-mapped at
/// `$1000..=$107F` (`$1000..=$103F` read port, `$1040..=$107F` write port).
/// Curated tier.
///
/// Confirmed against Stella's `CartDPC.cxx`/`.hxx` and Gopher2600's
/// `mapper_dpc.go` (both independently cite David P. Crane's US Patent
/// 4,644,495 — cross-checked here since they diverge on two points, resolved
/// by favoring Stella as the more mature oracle, see below). Data fetchers
/// 0-4 drive graphics reads (Pitfall II's level layout / vine / well art
/// uses these directly, plus fetchers 0-3 for the RNG-driven level
/// generation); fetchers 5-7 additionally support "music mode" for the
/// cartridge's own analog audio-mixing hardware.
///
/// **Deliberate residual (documented, not a bug):** Rusty2600's audio bus is
/// entirely TIA-owned (`docs/architecture.md`) with no cart-audio mixing
/// path, so DF5-7's registers (top/bottom/low/hi/music-mode) are modeled
/// faithfully and the `MUSIC` read (`$1004..=$1007`) returns the correct
/// additive amplitude mix for whatever state they're currently in, but the
/// real hardware's ~20 KHz-oscillator-driven *automatic* advance of DF5-7
/// while in music mode is not implemented (it only ever affects analog
/// audio output this emulator has no path for, and doing it properly needs
/// console-clock-rate awareness the `Board` trait doesn't expose). Every
/// other patent-described behavior — the RNG, DF0-4, bankswitching, and
/// DF5-7's register semantics when accessed as plain (non-auto-advancing)
/// fetchers — is implemented bit-exact.
///
/// Function-select bits 3-6 (nibble-swap / byte-reverse / ROR / ROL variants
/// of the display-AND-flag read, `$1018..=$1037`) return 0, matching
/// Stella's `CartDPC::peek` exactly (its `switch(function)` has no cases for
/// them). Gopher2600 additionally implements ROR/ROL here, which Stella does
/// not; Stella is the better-established oracle for Pitfall II specifically
/// (Pitfall II's vine-swing animation predates and does not depend on this),
/// so this board follows Stella's simpler, narrower model rather than
/// Gopher2600's.
///
/// Real-world dumps often carry extra trailing bytes beyond the canonical
/// 10 KiB (8 KiB program + 2 KiB display) — Gopher2600's own `mapper_dpc.go`
/// notes this is "random data from the cartridge's RNG" left over from the
/// dumping process, not part of the cartridge; `new()` accepts any image
/// `>= 10 KiB` and only reads the first 10 KiB, same tolerance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankDpc {
    #[serde(with = "serde_bytes_array")]
    program: [u8; 0x2000],
    #[serde(with = "serde_bytes_array")]
    display: [u8; 0x0800],
    bank: u8,
    tops: [u8; 8],
    bottoms: [u8; 8],
    low: [u8; 8],
    hi: [u8; 8],
    flags: [bool; 8],
    /// Music-mode enable for data fetchers 5, 6, 7 (indices 0, 1, 2 here).
    music_mode: [bool; 3],
    /// The DPC's LFSR register. Must never be left at 0 across a reset — real
    /// hardware and both reference emulators initialize it non-zero.
    random_number: u8,
}

impl BankDpc {
    const HOTSPOT_BANK0: u16 = 0x1FF8;
    const HOTSPOT_BANK1: u16 = 0x1FF9;
    /// Canonical DPC image size: 8 KiB program + 2 KiB display data.
    const IMAGE_SIZE: usize = 0x2800;

    /// Build from a DPC image. Accepts any slice `>= 10 KiB`, reading only
    /// the first 10 KiB (see the trailing-garbage-tolerance note above);
    /// returns `None` if shorter.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() < Self::IMAGE_SIZE {
            return None;
        }
        let mut program = [0u8; 0x2000];
        program.copy_from_slice(&rom[..0x2000]);
        let mut display = [0u8; 0x0800];
        display.copy_from_slice(&rom[0x2000..Self::IMAGE_SIZE]);
        Some(Self {
            program,
            display,
            bank: 1,
            tops: [0; 8],
            bottoms: [0; 8],
            low: [0; 8],
            hi: [0; 8],
            flags: [false; 8],
            music_mode: [false; 3],
            random_number: 1,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1FFF {
            Self::HOTSPOT_BANK0 => self.bank = 0,
            Self::HOTSPOT_BANK1 => self.bank = 1,
            _ => {}
        }
    }

    /// Clock the RNG's shift register. Per the patent (col 7, ln 58-62,
    /// fig 8): the input bit is the NOT of the XOR of bits 7, 5, 4, and 3;
    /// clocked on literally every cartridge access (both peeks and pokes,
    /// register or plain ROM) — confirmed unconditional in both Stella and
    /// Gopher2600's reference implementations.
    const fn clock_rng(&mut self) {
        let r = self.random_number;
        let bit = (((r >> 3) & 1) ^ ((r >> 4) & 1) ^ ((r >> 5) & 1) ^ ((r >> 7) & 1)) ^ 1;
        self.random_number = (r << 1) | bit;
    }

    /// Decrement a data fetcher's 11-bit `hi:low` counter by one, borrowing
    /// from `hi` on underflow; music-mode fetchers reload `low` from `top`
    /// on that borrow (patent col 5 ln 65 - col 6 ln 3, col 7 ln 14-19).
    const fn fetcher_clock(&mut self, i: usize) {
        self.low[i] = self.low[i].wrapping_sub(1);
        if self.low[i] == 0xFF {
            self.hi[i] = self.hi[i].wrapping_sub(1);
            if i >= 5 && self.music_mode[i - 5] {
                self.low[i] = self.tops[i];
            }
        }
    }

    /// Update a fetcher's flag register against its current `low` value
    /// (patent col 6 ln 7-12): sets on reaching `top`, clears on reaching
    /// `bottom`, otherwise holds its prior value.
    const fn fetcher_set_flag(&mut self, i: usize) {
        if self.low[i] == self.tops[i] {
            self.flags[i] = true;
        } else if self.low[i] == self.bottoms[i] {
            self.flags[i] = false;
        }
    }

    /// The 2 KiB display image is addressed from its END (memtop-relative):
    /// gfx offset = `2047 - (hi:low as an 11-bit value)`.
    const fn gfx_addr(&self, i: usize) -> usize {
        let counter = ((self.hi[i] as u16) << 8 | self.low[i] as u16) & 0x07FF;
        (counter ^ 0x07FF) as usize
    }
}

impl Board for BankDpc {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        self.clock_rng();
        let a = addr & 0x0FFF;
        if a >= 0x0080 {
            return self.program[usize::from(self.bank) * 0x1000 + a as usize];
        }
        let index = (a & 0x07) as usize;
        let function = (a >> 3) & 0x07;
        self.fetcher_set_flag(index);

        let result = match function {
            0 => {
                if index < 4 {
                    self.random_number
                } else {
                    let mut mix = 0u8;
                    if self.music_mode[0] && self.flags[5] {
                        mix += 4;
                    }
                    if self.music_mode[1] && self.flags[6] {
                        mix += 5;
                    }
                    if self.music_mode[2] && self.flags[7] {
                        mix += 6;
                    }
                    mix
                }
            }
            1 => self.display[self.gfx_addr(index)],
            2 => {
                if self.flags[index] {
                    self.display[self.gfx_addr(index)]
                } else {
                    0
                }
            }
            7 => u8::from(self.flags[index]) * 0xFF,
            // Nibble-swap / byte-reverse / ROR / ROL variants: unimplemented,
            // matching Stella's CartDPC::peek exactly (see the type doc).
            _ => 0,
        };

        if index < 5 || !self.music_mode[index - 5] {
            self.fetcher_clock(index);
        }
        result
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.hotspot(addr);
        self.clock_rng();
        let a = addr & 0x0FFF;
        if !(0x0040..0x0080).contains(&a) {
            return;
        }
        let index = (a & 0x07) as usize;
        let function = (a >> 3) & 0x07;
        match function {
            0 => {
                self.tops[index] = val;
                self.flags[index] = false;
            }
            1 => self.bottoms[index] = val,
            2 => {
                if index >= 5 && self.music_mode[index - 5] {
                    self.low[index] = self.tops[index];
                } else {
                    self.low[index] = val;
                }
            }
            3 => {
                self.hi[index] = val;
                if index >= 5 {
                    self.music_mode[index - 5] = (val & 0x10) != 0;
                }
            }
            6 => self.random_number = 1,
            _ => {}
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
    /// DPC (Pitfall II): 8 KiB program + 2 KiB display ROM + 8 data fetchers
    /// + RNG (Curated tier)
    BankDpc(BankDpc),
    /// E7 (M-Network): 16 KiB ROM, 8×2K banks + 2 KiB RAM (Curated tier)
    BankE7(BankE7),
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
            Self::BankDpc(b) => b.cpu_read(addr),
            Self::BankE7(b) => b.cpu_read(addr),
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
            Self::BankDpc(b) => b.cpu_write(addr, val),
            Self::BankE7(b) => b.cpu_write(addr, val),
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
            Self::BankDpc(b) => b.tier(),
            Self::BankE7(b) => b.tier(),
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
            Self::BankDpc(b) => b.tick(),
            Self::BankE7(b) => b.tick(),
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
            Self::BankDpc(b) => b.tick_coprocessor(),
            Self::BankE7(b) => b.tick_coprocessor(),
        }
    }
}

/// Returns `true` if `needle` occurs anywhere in `haystack`. Naive scan —
/// ROM images are at most a few hundred KiB and this runs once at load time,
/// so there's no need for a smarter substring-search algorithm.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Port of Stella's `CartDetector::isProbablySC`: a Superchip cart's ROM
/// image repeats each 4 KiB bank's first 128 bytes into the next 128 bytes
/// (the RAM-shadow region, electrically unreachable on real Superchip
/// hardware — but the assembler/burner still wrote data there, since it
/// didn't know the target board would be Superchip). Checked per-4 KiB-bank
/// across the whole image, matching Stella exactly (`rom.len()` must already
/// be a multiple of 4 KiB — true for all of F8SC/F6SC/F4SC's sizes).
fn is_probably_superchip(rom: &[u8]) -> bool {
    rom.chunks_exact(0x1000)
        .all(|bank| bank[0x00..0x80] == bank[0x80..0x100])
}

/// Port of Stella's `CartDetector::isProbablyCV`: search for either known
/// CommaVid RAM-access opcode signature (attributed to the MESS project).
/// Only two commercial CV titles ever shipped (Magicard, Video Life), so
/// this is a small, exact signature list rather than a general heuristic.
fn is_probably_cv(rom: &[u8]) -> bool {
    const STA_F3FF_X: [u8; 3] = [0x9D, 0xFF, 0xF3]; // STA $F3FF,X (Magicard)
    const STA_F400_Y: [u8; 3] = [0x99, 0x00, 0xF4]; // STA $F400,Y (Video Life)
    contains_bytes(rom, &STA_F3FF_X) || contains_bytes(rom, &STA_F400_Y)
}

/// Port of Stella's `CartDetector::isProbablyE7` (the 8-bank / 16 KiB
/// configuration `BankE7` implements): search for known bankswitch-hotspot
/// opcode encodings targeting `$FE0..=$FE7` — both the `$1FEx` and mirrored
/// `$FFEx` absolute-addressing forms, since the assembler could encode
/// either (attributed to the MESS project via Stella).
fn is_probably_e7(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 7] = [
        [0xAD, 0xE2, 0xFF], // LDA $FFE2
        [0xAD, 0xE5, 0xFF], // LDA $FFE5
        [0xAD, 0xE5, 0x1F], // LDA $1FE5
        [0xAD, 0xE7, 0x1F], // LDA $1FE7
        [0x0C, 0xE7, 0x1F], // NOP $1FE7
        [0x8D, 0xE7, 0xFF], // STA $FFE7
        [0x8D, 0xE7, 0x1F], // STA $1FE7
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
}

/// Detect the bankswitch scheme from a ROM image and build the board.
///
/// Same-size same-catalogue collisions (CV vs plain 2K/4K, Superchip vs
/// plain F8/F6/F4, E7 vs plain F6) are resolved with hotspot-pattern
/// heuristics ported from Stella's `CartDetector` (`T-0401-009`) — checked
/// BEFORE falling back to the more common plain scheme, so a real CV/
/// Superchip/E7 image is never silently misdetected. Schemes Rusty2600
/// hasn't implemented yet (E0/FE/3F at 8 KiB, DPC+/BMC, etc.) still return
/// `None`, keeping the honesty gate truthful — see each `TODO` below.
///
/// Returns `None` for an unrecognized size / scheme.
#[must_use]
pub fn detect(rom: &[u8]) -> Option<Cartridge> {
    match rom.len() {
        // 2 KiB / 4 KiB: `BankCV` (CommaVid) is the SAME two sizes (2 KiB
        // ROM-only, or 4 KiB "2K RAM-image + 2K ROM"); checked first via its
        // hotspot-pattern signature, falling back to plain ROM (Core) —
        // CommaVid only shipped 2 known titles (Magicard, Video Life), so
        // this ordering is deliberately safe either way.
        0x0800 if is_probably_cv(rom) => BankCV::new(rom).map(Cartridge::BankCV),
        0x0800 => Rom2K::new(rom).map(Cartridge::Rom2K),
        0x1000 if is_probably_cv(rom) => BankCV::new(rom).map(Cartridge::BankCV),
        0x1000 => Rom4K::new(rom).map(Cartridge::Rom4K),
        0x2000 => {
            // 8 KiB: Superchip (F8SC) checked first via its RAM-shadow
            // signature, falling back to plain F8 (Curated). Disambiguation
            // from E0 (Parker Bros, BestEffort) / FE (Activision SCABS,
            // BestEffort) / 3F-with-8K (Tigervision, BestEffort) still needs
            // hotspot-pattern + ROM-DB detection — those schemes aren't
            // implemented yet, so this only ever resolves F8/F8SC.
            // TODO(T-0401-001): E0 / FE / 3F (BestEffort) detection for 8 KiB images.
            if is_probably_superchip(rom) {
                BankF8::new(rom)
                    .map(BankF8::with_superchip)
                    .map(Cartridge::BankF8)
            } else {
                BankF8::new(rom).map(Cartridge::BankF8)
            }
        }
        // 10 KiB (+ up to 256 B of tolerated trailing dump garbage, see
        // BankDpc's doc comment): DPC (Pitfall II, Curated). Unambiguous —
        // nothing else in the catalogue is 10 KiB.
        0x2800..=0x2900 => BankDpc::new(rom).map(Cartridge::BankDpc),
        // 12 KiB: CBS's FA/RAM Plus (Curated) — this size is unambiguous
        // (nothing else in the catalogue is 12 KiB), so no disambiguation
        // needed. NOTE: an earlier version of this comment incorrectly said
        // "E7" here — E7 is 16 KiB (docs/cart.md), not 12; fixed.
        0x3000 => BankFA::new(rom).map(Cartridge::BankFA),
        0x4000 => {
            // 16 KiB: E7 (M-Network, Curated) checked first via its
            // bankswitch-hotspot signature; Superchip (F6SC) checked next
            // via its RAM-shadow signature; falls back to plain F6
            // (Curated, the far more common Atari-standard scheme — dozens
            // of Atari-published titles).
            if is_probably_e7(rom) {
                BankE7::new(rom).map(Cartridge::BankE7)
            } else if is_probably_superchip(rom) {
                BankF6::new(rom)
                    .map(BankF6::with_superchip)
                    .map(Cartridge::BankF6)
            } else {
                BankF6::new(rom).map(Cartridge::BankF6)
            }
        }
        0x8000 => {
            // 32 KiB: Superchip (F4SC) checked first via its RAM-shadow
            // signature, falling back to plain F4 (Curated).
            if is_probably_superchip(rom) {
                BankF4::new(rom)
                    .map(BankF4::with_superchip)
                    .map(Cartridge::BankF4)
            } else {
                BankF4::new(rom).map(Cartridge::BankF4)
            }
        }
        // T-0401-003 (DONE): Superchip variants F8SC/F6SC/F4SC — dispatched
        // above via is_probably_superchip().
        // TODO(T-0401-004): 3F (Tigervision) / 3E (Boulder Dash) / 3E+ (BestEffort).
        // T-0401-005 (DONE): DPC (Pitfall II, Curated) — see the 0x2800..=0x2900 arm above.
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
    fn f8sc_superchip_ram_write_low_read_high() {
        let mut img = [0u8; 0x2000];
        img[0x1000 + 0x0090] = 0x99; // bank 1, RAM-window ROM byte must NOT surface once superchip is on
        let mut board = BankF8::new(&img).unwrap().with_superchip();
        assert_eq!(board.tier(), Tier::Curated);
        board.cpu_write(0x1000, 0x42); // write-low $1000-$107F
        assert_eq!(board.cpu_read(0x1080), 0x42); // read-high $1080-$10FF, same 128 B
        // Outside the RAM window, ROM still reads through normally.
        assert_eq!(board.cpu_read(0x1200), 0x00);
    }

    #[test]
    fn f8_without_superchip_ignores_ram_window() {
        // Plain (non-SC) F8 must not accidentally expose a RAM overlay.
        let mut img = [0u8; 0x2000];
        img[0x1000 + 0x0080] = 0x55;
        let mut board = BankF8::new(&img).unwrap();
        board.cpu_write(0x1000, 0xFF); // would corrupt RAM if superchip leaked through
        assert_eq!(board.cpu_read(0x1080), 0x55); // still plain ROM read
    }

    #[test]
    fn f6sc_superchip_ram_write_low_read_high() {
        let img = [0u8; 0x4000];
        let mut board = BankF6::new(&img).unwrap().with_superchip();
        board.cpu_write(0x1010, 0x33);
        assert_eq!(board.cpu_read(0x1090), 0x33);
    }

    #[test]
    fn f4sc_superchip_ram_write_low_read_high() {
        let img = [0u8; 0x8000];
        let mut board = BankF4::new(&img).unwrap().with_superchip();
        board.cpu_write(0x1020, 0x64);
        assert_eq!(board.cpu_read(0x10A0), 0x64);
    }

    #[test]
    fn detect_resolves_fa_size_unambiguously() {
        let fa = detect(&[0u8; 0x3000]).unwrap();
        assert!(matches!(fa, Cartridge::BankFA(_)));
    }

    #[test]
    fn detect_resolves_cv_via_hotspot_signature_else_plain_rom() {
        let mut img = [0u8; 0x0800];
        img[0x0100] = 0x99; // STA $F400,Y (Video Life's CV signature)
        img[0x0101] = 0x00;
        img[0x0102] = 0xF4;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankCV(_)));

        // Without the signature, the same size resolves to plain ROM.
        assert!(matches!(
            detect(&[0u8; 0x0800]).unwrap(),
            Cartridge::Rom2K(_)
        ));
    }

    #[test]
    fn detect_engages_superchip_only_when_the_ram_shadow_signature_matches() {
        // 8 KiB, both 4 KiB banks exhibit the Superchip signature (each
        // bank's first 128 bytes duplicated into its next 128).
        let mut sc_img = [0u8; 0x2000];
        for i in 0..0x80 {
            sc_img[i] = (i as u8).wrapping_add(1);
            sc_img[0x80 + i] = sc_img[i];
            sc_img[0x1000 + i] = (i as u8).wrapping_add(7);
            sc_img[0x1000 + 0x80 + i] = sc_img[0x1000 + i];
        }
        let mut sc_cart = detect(&sc_img).unwrap();
        assert!(matches!(sc_cart, Cartridge::BankF8(_)));
        sc_cart.cpu_write(0x1000, 0x42);
        assert_eq!(
            sc_cart.cpu_read(0x1080),
            0x42,
            "superchip RAM must be engaged"
        );

        // Same size, but the shadow bytes deliberately DON'T match --
        // must fall back to plain (non-superchip) F8.
        let mut plain_img = [0u8; 0x2000];
        plain_img[0x00] = 0x11;
        plain_img[0x80] = 0x22;
        let mut plain_cart = detect(&plain_img).unwrap();
        assert!(matches!(plain_cart, Cartridge::BankF8(_)));
        plain_cart.cpu_write(0x1000, 0x99);
        assert_eq!(
            plain_cart.cpu_read(0x1080),
            0x00,
            "plain F8 RAM writes are no-ops"
        );
    }

    #[test]
    fn detect_resolves_e7_via_hotspot_signature_else_plain_f6() {
        let mut img = [0u8; 0x4000];
        img[0x0100] = 0xAD; // LDA $1FE7 (an E7 bankswitch-hotspot access)
        img[0x0101] = 0xE7;
        img[0x0102] = 0x1F;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankE7(_)));

        // Without an E7 or Superchip signature, 16 KiB falls back to F6.
        let mut plain_img = [0u8; 0x4000];
        plain_img[0x00] = 0x11;
        plain_img[0x80] = 0x22; // deliberately mismatched -> not Superchip
        assert!(matches!(detect(&plain_img).unwrap(), Cartridge::BankF6(_)));
    }

    #[test]
    fn dpc_new_rejects_undersized_images() {
        assert!(BankDpc::new(&[0u8; 0x2800 - 1]).is_none());
    }

    #[test]
    fn dpc_f8_style_bank_switch() {
        let mut img = [0u8; 0x2800];
        img[0x0FFF] = 0x11; // bank 0, last byte
        img[0x1FFF] = 0x22; // bank 1, last byte
        let mut board = BankDpc::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::Curated);
        board.cpu_read(0x1FF8); // select bank 0
        assert_eq!(board.cpu_read(0x1FFF), 0x11);
        board.cpu_read(0x1FF9); // select bank 1
        assert_eq!(board.cpu_read(0x1FFF), 0x22);
    }

    #[test]
    fn dpc_rng_advances_and_never_settles_at_zero() {
        let img = [0u8; 0x2800];
        let mut board = BankDpc::new(&img).unwrap();
        let mut prev = board.cpu_read(0x1000);
        let mut changed = false;
        for _ in 0..16 {
            let next = board.cpu_read(0x1000);
            assert_ne!(next, 0, "DPC RNG must never settle at 0");
            changed |= next != prev;
            prev = next;
        }
        assert!(changed, "RNG must advance across repeated accesses");
    }

    #[test]
    fn dpc_rng_reset_hotspot_reproduces_a_fresh_boards_sequence() {
        let img = [0u8; 0x2800];
        let mut board = BankDpc::new(&img).unwrap();
        board.cpu_read(0x1000);
        board.cpu_read(0x1000);
        board.cpu_write(0x1070, 0); // RNG reset hotspot (any value written)
        let mut fresh = BankDpc::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1000), fresh.cpu_read(0x1000));
    }

    #[test]
    fn dpc_display_fetcher_reads_and_clocks_counter() {
        let mut img = [0u8; 0x2800];
        // Fetcher 0 defaults hi=0, low=0 -> gfx_addr = 2047 (memtop-relative).
        img[0x2000 + 0x07FF] = 0xAA;
        // After one clock (low/hi both wrap to 0xFF), gfx_addr = 0.
        img[0x2000] = 0xBB;
        let mut board = BankDpc::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1008), 0xAA); // DF0 display read
        assert_eq!(board.cpu_read(0x1008), 0xBB); // counter clocked in between
    }

    #[test]
    fn dpc_flag_tracks_top_and_bottom_bounds() {
        let img = [0u8; 0x2800];
        let mut board = BankDpc::new(&img).unwrap();
        board.cpu_write(0x1040, 0x00); // DF0 top = 0 (also clears the flag)
        board.cpu_write(0x1048, 0x05); // DF0 bottom = 5
        board.cpu_write(0x1050, 0x00); // DF0 low = 0 (== top)
        assert_eq!(board.cpu_read(0x1038), 0xFF, "low == top must set the flag");
    }

    #[test]
    fn dpc_music_mode_additive_amplitude_mix() {
        let img = [0u8; 0x2800];
        let mut board = BankDpc::new(&img).unwrap();
        board.cpu_write(0x105D, 0x10); // DF5 hi write: enable music mode
        board.cpu_write(0x105E, 0x10); // DF6 hi write: enable music mode
        // DF7's music mode stays off. All three fetchers default
        // top=bottom=low=0, so reading each flag register sets flags[i]=true.
        board.cpu_read(0x103D); // DF5 flag
        board.cpu_read(0x103E); // DF6 flag
        board.cpu_read(0x103F); // DF7 flag
        // DF5 (weight 4) + DF6 (weight 5); DF7 contributes 0 since its music
        // mode is off, even though its flag is also set.
        assert_eq!(board.cpu_read(0x1004), 9);
    }

    #[test]
    fn detect_resolves_dpc_and_tolerates_trailing_dump_garbage() {
        let exact = detect(&[0u8; 0x2800]).unwrap();
        assert!(matches!(exact, Cartridge::BankDpc(_)));
        // Real-world dumps often carry trailing garbage past the canonical
        // 10 KiB (see BankDpc's doc comment) -- must still resolve.
        let padded = detect(&[0u8; 0x2800 + 255]).unwrap();
        assert!(matches!(padded, Cartridge::BankDpc(_)));
        // 12 KiB stays unambiguously FA, unaffected by the DPC tolerance window.
        let fa = detect(&[0u8; 0x3000]).unwrap();
        assert!(matches!(fa, Cartridge::BankFA(_)));
    }

    #[test]
    fn e7_bank_switch_lower_segment() {
        let mut img = [0u8; 0x4000];
        img[3 * 0x800] = 0x33; // bank 3, first byte
        img[5 * 0x800] = 0x55; // bank 5, first byte
        let mut board = BankE7::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::Curated);
        board.cpu_read(0x1FE3); // select bank 3
        assert_eq!(board.cpu_read(0x1000), 0x33);
        board.cpu_read(0x1FE5); // select bank 5
        assert_eq!(board.cpu_read(0x1000), 0x55);
    }

    #[test]
    fn e7_ram_bank_write_low_read_high() {
        let img = [0u8; 0x4000];
        let mut board = BankE7::new(&img).unwrap();
        board.cpu_read(0x1FE7); // select bank 7 -> RAM mode for the lower segment
        board.cpu_write(0x1000, 0x77); // write port
        assert_eq!(board.cpu_read(0x1400), 0x77); // read port, same underlying byte
    }

    #[test]
    fn e7_writes_to_read_ports_are_ignored() {
        let img = [0u8; 0x4000];
        let mut board = BankE7::new(&img).unwrap();
        board.cpu_read(0x1FE7); // RAM mode
        board.cpu_write(0x1400, 0xAA); // write to the READ port: must be a no-op
        assert_eq!(board.cpu_read(0x1000), 0x00);
    }

    #[test]
    fn e7_small_ram_window_write_low_read_high_with_subbank_select() {
        let img = [0u8; 0x4000];
        let mut board = BankE7::new(&img).unwrap();
        board.cpu_read(0x1FE9); // select RAM sub-bank 1
        board.cpu_write(0x1800, 0x22);
        assert_eq!(board.cpu_read(0x1900), 0x22);
        board.cpu_read(0x1FE8); // switch to sub-bank 0: independent storage
        assert_eq!(board.cpu_read(0x1900), 0x00);
    }

    #[test]
    fn e7_upper_segment_always_maps_the_last_bank() {
        let mut img = [0u8; 0x4000];
        img[7 * 0x800 + 0x200] = 0x99; // last bank, offset matching CPU addr $1A00
        let mut board = BankE7::new(&img).unwrap();
        // Regardless of which lower bank (or RAM mode) is selected...
        board.cpu_read(0x1FE3);
        assert_eq!(board.cpu_read(0x1A00), 0x99);
        board.cpu_read(0x1FE7); // RAM mode for the LOWER segment
        assert_eq!(board.cpu_read(0x1A00), 0x99); // upper segment unaffected
    }
}
