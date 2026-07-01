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

    /// CPU-side write through the board's current bank mapping (`addr & 0x1000
    /// != 0`, i.e. cart-window addresses only). Drives write-triggered
    /// hotspots inside the window and on-cart RAM.
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

    /// Observe a CPU write to a NON-cart-window address (`addr & 0x1000 ==
    /// 0`, i.e. TIA/RIOT space) before the Bus routes it there. Real 2600
    /// cartridge edge connectors are wired to every address line, not just
    /// A12 — several classic BestEffort schemes bankswitch on writes the
    /// console itself thinks are plain TIA/RIOT accesses: 3F/3E (Tigervision)
    /// trigger on any write whose low byte is `$3F`/`$3E`, UA on `$220`/`$240`,
    /// 0840 on `$800`/`$840` — all deep in TIA/RIOT-mirrored space, not
    /// `$1000+`. Default no-op so the overwhelming majority of boards (which
    /// only care about their own `$1000..=$1FFF` window) pay nothing.
    fn snoop_write(&mut self, addr: u16, val: u8) {
        let _ = (addr, val);
    }

    /// Observe a CPU read of a NON-cart-window address (`addr & 0x1000 ==
    /// 0`), called AFTER the Bus computes the value TIA/RIOT would return
    /// (passed as `val`) — the board only OBSERVES, it never redirects the
    /// read; UA/0840 just need the access address, while FE additionally
    /// needs the observed value itself (a JSR return-address byte pushed to
    /// the stack, which happens to sit at `$01FE`, encodes which bank to
    /// switch to). Default no-op, same reasoning as [`Self::snoop_write`].
    fn snoop_read(&mut self, addr: u16, val: u8) {
        let _ = (addr, val);
    }
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

/// F0 (Dynacom Megaboy): 64 KiB ROM as sixteen 4 KiB banks, switched by a
/// single SEQUENTIAL-ADVANCE hotspot — any access to `$1FF0` moves to the
/// next bank (wrapping 15 -> 0). Unlike every other F-series scheme, the
/// game can't jump to an arbitrary bank; its code layout must visit banks
/// in order. BestEffort tier (register-decode + boot-smoke only).
///
/// Stored as a `Vec<u8>` rather than a `[u8; 0x10000]`: this struct lives
/// inside the `Cartridge` enum, which is sized to its LARGEST variant — an
/// inline 64 KiB array there would inflate every stack frame that moves a
/// `Cartridge`/`Bus`/`System` by value (this crate `forbid`s `unsafe`, so
/// there's no zero-copy way to keep a compile-time-sized array off the
/// stack during construction either). `Bank3F`/`Bank3E` use the same
/// pattern for the same reason.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankF0 {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
}

impl BankF0 {
    const HOTSPOT: u16 = 0x1FF0;

    /// Build from a 64 KiB image. Returns `None` if the slice is not 64 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() != 0x10000 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank: 15,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        if addr & 0x1FFF == Self::HOTSPOT {
            self.bank = (self.bank + 1) & 0x0F;
        }
    }
}

impl Board for BankF0 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let off = usize::from(self.bank) * 0x1000 + (addr & 0x0FFF) as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
}

/// E0 (Parker Brothers): 8 KiB ROM split into four 1 KiB segments. The first
/// three segments are each INDEPENDENTLY selectable among all 8 possible
/// 1 KiB banks (`$1FE0..=$1FE7` for segment 0, `$1FE8..=$1FEF` for segment 1,
/// `$1FF0..=$1FF7` for segment 2 — the low 3 bits of the address pick the
/// bank); the fourth segment is permanently fixed to the last 1 KiB bank.
/// This is the most address-hungry classic scheme — real hardware compares
/// only 3 bits per hotspot range, so the effective bank count per segment is
/// always 8 regardless of image size. BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankE0 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x2000],
    /// Selected bank (0-7) for segments 0-2; segment 3 is always bank 7.
    segments: [u8; 3],
}

impl BankE0 {
    /// Reset default per Stella's `CartridgeE0::reset` (non-randomized path).
    const DEFAULT_SEGMENTS: [u8; 3] = [4, 5, 6];
    const FIXED_LAST_BANK: u8 = 7;

    /// Build from an 8 KiB image. Returns `None` if the slice is not 8 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            segments: Self::DEFAULT_SEGMENTS,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        let a = addr & 0x1FFF;
        if (0x1FE0..=0x1FE7).contains(&a) {
            self.segments[0] = (a & 0x0007) as u8;
        } else if (0x1FE8..=0x1FEF).contains(&a) {
            self.segments[1] = (a & 0x0007) as u8;
        } else if (0x1FF0..=0x1FF7).contains(&a) {
            self.segments[2] = (a & 0x0007) as u8;
        }
    }

    fn bank_for(&self, a: u16) -> u8 {
        match a >> 10 {
            0 => self.segments[0],
            1 => self.segments[1],
            2 => self.segments[2],
            _ => Self::FIXED_LAST_BANK,
        }
    }
}

impl Board for BankE0 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.hotspot(addr);
        let a = addr & 0x0FFF;
        let off = usize::from(self.bank_for(a)) * 0x400 + (a & 0x03FF) as usize;
        self.rom[off]
    }
    fn cpu_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
}

/// 3F (Tigervision): bank switched by writing the desired bank number to
/// ANY address whose low byte is `$3F` — including plain zero-page writes
/// like `STA $3F`, since the cart's address decode only checks the low 6
/// bits and doesn't care about A12+ or whether the console itself thinks
/// the write landed in TIA/RIOT space. This is why [`Board::snoop_write`]
/// exists: the Bus must forward writes OUTSIDE the cart window too. The low
/// 2 KiB of the CPU window is the selected bank; the high 2 KiB is always
/// fixed to the LAST bank. BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bank3F {
    rom: alloc::vec::Vec<u8>,
    bank_count: u8,
    bank: u8,
}

impl Bank3F {
    /// Build from any image that's an exact multiple of 2 KiB (Tigervision
    /// carts ranged from a few KiB up to 512 KiB). Returns `None` otherwise.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.is_empty() || rom.len() % 0x0800 != 0 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank_count: (rom.len() / 0x0800) as u8,
            bank: 0,
        })
    }
}

impl Board for Bank3F {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        let bank = if a < 0x0800 {
            self.bank
        } else {
            self.bank_count - 1
        };
        self.rom[usize::from(bank) * 0x0800 + (a & 0x07FF) as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_write(&mut self, addr: u16, val: u8) {
        if addr & 0x00FF == 0x003F {
            self.bank = val % self.bank_count;
        }
    }
}

/// 3E (Tigervision + RAM, Boulder Dash): identical bankswitching to
/// [`Bank3F`] via `$3F` (ROM bank select), plus a second hotspot at `$3E`
/// that instead selects a 1 KiB RAM bank into the low segment (write-low /
/// read-high within that segment, matching this crate's established
/// convention). BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bank3E {
    rom: alloc::vec::Vec<u8>,
    rom_bank_count: u8,
    ram: alloc::vec::Vec<u8>,
    ram_bank_count: u8,
    /// `None` = ROM bank selected in the low segment; `Some(n)` = RAM bank `n`.
    ram_bank: Option<u8>,
    rom_bank: u8,
}

impl Bank3E {
    const RAM_BANK_SIZE: usize = 0x0400; // 1 KiB (write-low $000-$3FF / read-high $400-$7FF)

    /// Build from any image that's an exact multiple of 2 KiB, with `ram_kib`
    /// KiB of on-cart RAM (Boulder Dash shipped 32 KiB RAM). Returns `None`
    /// for a non-2-KiB-multiple image or non-1-KiB-multiple RAM size.
    #[must_use]
    pub fn new(rom: &[u8], ram_kib: usize) -> Option<Self> {
        if rom.is_empty() || rom.len() % 0x0800 != 0 || ram_kib == 0 {
            return None;
        }
        let ram_bytes = ram_kib * 1024;
        if ram_bytes % Self::RAM_BANK_SIZE != 0 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            rom_bank_count: (rom.len() / 0x0800) as u8,
            ram: alloc::vec![0; ram_bytes],
            ram_bank_count: (ram_bytes / Self::RAM_BANK_SIZE) as u8,
            ram_bank: None,
            rom_bank: 0,
        })
    }
}

impl Board for Bank3E {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        if a >= 0x0800 {
            let bank = self.rom_bank_count - 1;
            return self.rom[usize::from(bank) * 0x0800 + (a & 0x07FF) as usize];
        }
        if let Some(ram_bank) = self.ram_bank {
            self.ram[usize::from(ram_bank) * Self::RAM_BANK_SIZE + (a & 0x03FF) as usize]
        } else {
            self.rom[usize::from(self.rom_bank) * 0x0800 + a as usize]
        }
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        let a = addr & 0x0FFF;
        if let Some(ram_bank) = self.ram_bank {
            if a < 0x0400 {
                self.ram[usize::from(ram_bank) * Self::RAM_BANK_SIZE + a as usize] = val;
            }
        }
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_write(&mut self, addr: u16, val: u8) {
        match addr & 0x00FF {
            0x003F => {
                self.rom_bank = val % self.rom_bank_count;
                self.ram_bank = None;
            }
            0x003E => {
                self.ram_bank = Some(val % self.ram_bank_count);
            }
            _ => {}
        }
    }
}

/// Shared write/read logic for the "CPUWIZ" homebrew F-series-successor
/// family (EF/BF/DF): a direct-select hotspot range (unlike F0's
/// sequential-advance) plus an optional 128 B Superchip RAM overlay at the
/// SAME `$1000..=$107F` window Superchip always uses. `bank_count`,
/// `hotspot_base`, and total ROM size differ per scheme; the mechanics are
/// otherwise identical, so this one function backs all three boards below
/// rather than three near-duplicate copies.
fn ef_family_read(rom: &[u8], ram: &[u8; 0x80], superchip: bool, bank: u8, addr: u16) -> u8 {
    let a = addr & 0x0FFF;
    if superchip && (0x0080..0x0100).contains(&a) {
        return ram[(a & 0x007F) as usize];
    }
    rom[usize::from(bank) * 0x1000 + a as usize]
}

fn ef_family_write(ram: &mut [u8; 0x80], superchip: bool, addr: u16, val: u8) {
    if superchip {
        let a = addr & 0x0FFF;
        if a < 0x0080 {
            ram[a as usize] = val;
        }
    }
}

fn ef_family_hotspot(bank: &mut u8, hotspot_base: u16, bank_count: u8, addr: u16) {
    let a = addr & 0x1FFF;
    if (hotspot_base..hotspot_base + u16::from(bank_count)).contains(&a) {
        *bank = (a - hotspot_base) as u8;
    }
}

/// EF (CPUWIZ): 64 KiB ROM as sixteen 4 KiB banks, direct-select hotspots
/// `$1FE0..=$1FEF` (unlike [`BankF0`]'s sequential-advance single hotspot at
/// the same size). EFSC adds the standard 128 B Superchip RAM overlay via
/// [`Self::with_superchip`]. Default start bank is 0 (Stella's
/// `CartridgeEnhanced` default — EF/BF/DF don't override it the way the
/// classic F-series boards override to the last bank). BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankEF {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
}

impl BankEF {
    const HOTSPOT_BASE: u16 = 0x1FE0;
    const BANK_COUNT: u8 = 16;

    /// Build from a 64 KiB image. Returns `None` if the slice is not 64 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() != 0x10000 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank: 0,
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the EFSC Superchip 128 B RAM overlay.
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
    }
}

impl Board for BankEF {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_read(&self.rom, &self.ram, self.superchip, self.bank, addr)
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_write(&mut self.ram, self.superchip, addr, val);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
}

/// DF (CPUWIZ): 128 KiB ROM as thirty-two 4 KiB banks, direct-select
/// hotspots `$1FC0..=$1FDF`. DFSC adds the 128 B Superchip RAM overlay.
/// BestEffort tier. See [`BankEF`] for the shared mechanics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankDF {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
}

impl BankDF {
    const HOTSPOT_BASE: u16 = 0x1FC0;
    const BANK_COUNT: u8 = 32;

    /// Build from a 128 KiB image. Returns `None` if the slice is not 128 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() != 0x20000 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank: 0,
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the DFSC Superchip 128 B RAM overlay.
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
    }
}

impl Board for BankDF {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_read(&self.rom, &self.ram, self.superchip, self.bank, addr)
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_write(&mut self.ram, self.superchip, addr, val);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
}

/// BF (CPUWIZ): 256 KiB ROM as sixty-four 4 KiB banks, direct-select
/// hotspots `$1F80..=$1FBF`. BFSC adds the 128 B Superchip RAM overlay.
/// BestEffort tier. See [`BankEF`] for the shared mechanics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankBF {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
    #[serde(with = "serde_bytes_array")]
    ram: [u8; 0x80],
    superchip: bool,
}

impl BankBF {
    const HOTSPOT_BASE: u16 = 0x1F80;
    const BANK_COUNT: u8 = 64;

    /// Build from a 256 KiB image. Returns `None` if the slice is not 256 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() != 0x40000 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank: 0,
            ram: [0; 0x80],
            superchip: false,
        })
    }

    /// Enable the BFSC Superchip 128 B RAM overlay.
    #[must_use]
    pub const fn with_superchip(mut self) -> Self {
        self.superchip = true;
        self
    }
}

impl Board for BankBF {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_read(&self.rom, &self.ram, self.superchip, self.bank, addr)
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        ef_family_hotspot(&mut self.bank, Self::HOTSPOT_BASE, Self::BANK_COUNT, addr);
        ef_family_write(&mut self.ram, self.superchip, addr, val);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
}

/// UA (UA Ltd. / Brazilian Digivision): 8 KiB ROM as two 4 KiB banks,
/// switched by accessing `$220`/`$240` (or the Digivision variant's
/// `$2C0`/`$FB0`) — addresses in TIA-mirrored space, not the cart window,
/// so bank switching relies on [`Board::snoop_read`]/[`Board::snoop_write`]
/// rather than `cpu_read`/`cpu_write` (real hardware: the cart observes
/// these accesses but never changes what TIA/RIOT return for them). Default
/// start bank is 0 (Stella's `CartridgeEnhanced` default). BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankUA {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x2000],
    bank: u8,
}

impl BankUA {
    /// Build from an 8 KiB image. Returns `None` if the slice is not 8 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            bank: 0,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1260 {
            0x0220 => self.bank = 0,
            0x0240 => self.bank = 1,
            _ => {}
        }
    }
}

impl Board for BankUA {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        self.rom[usize::from(self.bank) * 0x1000 + a as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_read(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn snoop_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
}

/// 0840 (EconoBank): 8 KiB ROM as two 4 KiB banks, switched by accessing
/// `$800`/`$840` — again TIA-mirrored space, using the same snoop-based
/// mechanism as [`BankUA`]. BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bank0840 {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x2000],
    bank: u8,
}

impl Bank0840 {
    /// Build from an 8 KiB image. Returns `None` if the slice is not 8 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            bank: 0,
        })
    }

    #[allow(clippy::missing_const_for_fn)]
    fn hotspot(&mut self, addr: u16) {
        match addr & 0x1840 {
            0x0800 => self.bank = 0,
            0x0840 => self.bank = 1,
            _ => {}
        }
    }
}

impl Board for Bank0840 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        self.rom[usize::from(self.bank) * 0x1000 + a as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_read(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn snoop_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
}

/// FE (Activision "Robot Tank"/"Decathlon"/"Space Shuttle"/"Thwocker"): 8 KiB
/// ROM as two 4 KiB banks, selected by a hardware trick rather than an
/// explicit hotspot address. The bank-switch routine performs an indirect
/// `JSR` whose target lives in the OTHER bank; because the call always
/// happens with the stack pointer parked at a fixed value, the CPU's `JSR`
/// microcode pushes the return address's high byte to `$01FE` — a mirror of
/// RIOT zero-page RAM, not the cart window — immediately followed by the low
/// byte to `$01FD`. `$01FE` is watched via [`Board::snoop_read`]/
/// [`Board::snoop_write`] (the console routes it to RIOT RAM, same
/// TIA/RIOT-mirrored-space reasoning as [`BankUA`]/[`Bank0840`]); the NEXT
/// bus access after a `$01FE` touch (of either kind, from any address) uses
/// ITS value to select the bank: `(value >> 5) ^ 0b111`, masked to the 2
/// available banks. Matches Stella's `CartridgeFE::checkSwitchBank` exactly.
/// BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankFe {
    #[serde(with = "serde_bytes_array")]
    rom: [u8; 0x2000],
    bank: u8,
    last_access_was_01fe: bool,
}

impl BankFe {
    /// Build from an 8 KiB image. Returns `None` if the slice is not 8 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        Some(Self {
            rom: rom.try_into().ok()?,
            bank: 0,
            last_access_was_01fe: false,
        })
    }

    fn check_switch(&mut self, addr: u16, val: u8) {
        if self.last_access_was_01fe {
            self.bank = ((val >> 5) ^ 0b111) & 0x01;
            self.last_access_was_01fe = false;
        } else {
            self.last_access_was_01fe = addr == 0x01FE;
        }
    }
}

impl Board for BankFe {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        let val = self.rom[usize::from(self.bank) * 0x1000 + a as usize];
        self.check_switch(addr, val);
        val
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        self.check_switch(addr, val);
    }
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_read(&mut self, addr: u16, val: u8) {
        self.check_switch(addr, val);
    }
    fn snoop_write(&mut self, addr: u16, val: u8) {
        self.check_switch(addr, val);
    }
}

/// SB (Superbank / "128-in-1"-style multicarts): 128 or 256 KiB ROM as 32
/// or 64 4 KiB banks. Any read OR write to `$0800..=$0FFF` (TIA/RIOT-mirrored
/// space, watched via [`Board::snoop_read`]/[`Board::snoop_write`] like
/// [`BankUA`]/[`Bank0840`]/[`BankFe`]) selects the bank from the LOW BITS of
/// the accessed address itself (`address & (bank_count - 1)`), not a fixed
/// hotspot value — so the specific address touched within that 2 KiB range
/// IS the bank number. Matches Stella's `CartridgeSB::checkSwitchBank`
/// (modulo its outer address-mirroring pre-mask, an implementation detail of
/// Stella's own paged-address model with no equivalent here since this
/// crate's `Bus` already fully decodes to 13 bits before reaching `Board`).
/// BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankSb {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
    bank_mask: u8,
}

impl BankSb {
    /// Build from a 128 KiB or 256 KiB image. Returns `None` for any other size.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let bank_count: u16 = match rom.len() {
            0x20000 => 32,
            0x40000 => 64,
            _ => return None,
        };
        Some(Self {
            rom: rom.to_vec(),
            bank: 0,
            #[allow(clippy::cast_possible_truncation)]
            bank_mask: (bank_count - 1) as u8,
        })
    }

    fn hotspot(&mut self, addr: u16) {
        if addr & 0x1800 == 0x0800 {
            #[allow(clippy::cast_possible_truncation)]
            let low = addr as u8;
            self.bank = low & self.bank_mask;
        }
    }
}

impl Board for BankSb {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        self.rom[usize::from(self.bank) * 0x1000 + a as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_read(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn snoop_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
}

/// X07 (AtariAge homebrew multicart scheme, "Stellar Wars" et al.): 64 KiB
/// ROM as 16 4 KiB banks, selected by two hotspot patterns in TIA/RIOT-
/// mirrored space (watched via [`Board::snoop_read`]/[`Board::snoop_write`]):
/// a direct select (`address & 0x180F == 0x080D` picks address bits 4-7 as
/// the bank number directly) plus a secondary toggle active ONLY while the
/// currently-selected bank is 14 or 15 (`address & 0x1880 == 0` flips the
/// bank's low bit via address bit 6, staying within {14, 15}). Matches
/// Stella's `CartridgeX07::checkSwitchBank` exactly. BestEffort tier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BankX07 {
    rom: alloc::vec::Vec<u8>,
    bank: u8,
}

impl BankX07 {
    /// Build from a 64 KiB image. Returns `None` if the slice is not 64 KiB.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        if rom.len() != 0x10000 {
            return None;
        }
        Some(Self {
            rom: rom.to_vec(),
            bank: 0,
        })
    }

    fn hotspot(&mut self, addr: u16) {
        #[allow(clippy::cast_possible_truncation)]
        if addr & 0x180F == 0x080D {
            self.bank = ((addr & 0x00F0) >> 4) as u8;
        } else if addr & 0x1880 == 0 && (self.bank & 0x0E) == 0x0E {
            self.bank = (((addr & 0x0040) >> 6) as u8) | 0x0E;
        }
    }
}

impl Board for BankX07 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x0FFF;
        self.rom[usize::from(self.bank) * 0x1000 + a as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _val: u8) {}
    fn tier(&self) -> Tier {
        Tier::BestEffort
    }
    fn snoop_read(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
    fn snoop_write(&mut self, addr: u16, _val: u8) {
        self.hotspot(addr);
    }
}

/// 4A50 (John "Supercat" Payson's scheme, e.g. some homebrew titles): a
/// 128 KiB ROM image (32/64 KiB dumps tiled to fill it, matching Stella's own
/// constructor) plus 32 KiB of on-cart RAM, split into three independently
/// relocatable segments, each individually mapped to ROM or RAM:
///
/// - `$1000-$17FF` (2K, `slice_low`): ROM reads come from the FIRST 64 KiB of
///   the image (no `+0x10000` offset); RAM reads/writes are unoffset into the
///   32 KiB RAM array.
/// - `$1800-$1DFF` (1.5K, `slice_middle`): ROM reads come from the SECOND
///   64 KiB of the image (`+0x10000`).
/// - `$1E00-$1EFF` (256 B, `slice_high`): ROM reads also come from the second
///   64 KiB (`+0x10000`).
/// - `$1F00-$1FFF` (256 B): always the LAST 256 B of the (tiled) 128 KiB
///   image — never switches ROM/RAM — but doubles as this scheme's hotspot
///   trigger region (see below).
///
/// Bankswitching is driven by a `(previous access's value, previous access's
/// address)` state machine (`last_data`/`last_address`, updated after EVERY
/// access to ANY address, cart-window or not) rather than a fixed hotspot
/// address: most hotspots only arm when the immediately preceding access read
/// or wrote a value matching `(value & 0xe0) == 0x60` from an address that
/// was either in the cart window or in RIOT zero-page (`< 0x200`). Given that,
/// the NEXT access's address (checked in `$1000..=$1FFF`-mirrored TIA/RIOT
/// space, i.e. below `$1000`) decides what switches, per Stella's
/// `checkBankSwitch` (`address & 0x0f00`/`0x0f40`/`0x0f50` patterns for the
/// three segments plus four "helper" address-bit-toggle hotspots) — this is
/// exactly the case [`Board::snoop_read`]/[`Board::snoop_write`] exist for. A
/// second, unconditional set of zero-page hotspots (`$74-$7F`/`$F4-$FF`)
/// additionally arms straight off the accessed VALUE, matching Stella's
/// zero-page hotspot chain. Within the cart window, only the `$1F00-$1FFF`
/// region has its own (smaller) instance of this same previous-access-gated
/// check, toggling bits of `slice_high` directly.
///
/// Confirmed against Stella's `Cartridge4A50.cxx`/`.hxx` — including that
/// scheme's own doc comment noting it "hasn't been fully implemented, and may
/// never be" (missing hi-res helper functions and `$1E00` page-wrap, per
/// Stella's own author). This port is a faithful, equally-scoped translation
/// of exactly the behavior Stella itself implements — not a superset. Only
/// one known test ROM exists for this scheme (per Stella's own comment), so
/// this stays `BestEffort` tier indefinitely; see `docs/cart.md`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bank4A50 {
    /// Always the full 128 KiB, tiled from a 32/64 KiB input image if needed
    /// (matches Stella's own constructor).
    rom: alloc::vec::Vec<u8>,
    /// 32 KiB of on-cart RAM.
    ram: alloc::vec::Vec<u8>,
    slice_low: u16,
    slice_middle: u16,
    slice_high: u16,
    is_rom_low: bool,
    is_rom_middle: bool,
    is_rom_high: bool,
    last_data: u8,
    last_address: u16,
}

impl Bank4A50 {
    const IMAGE_SIZE: usize = 0x2_0000; // 128 KiB
    const RAM_SIZE: usize = 0x8000; // 32 KiB

    /// Build from a 32, 64, or 128 KiB image, tiled to fill the full 128 KiB
    /// address space Stella's own constructor always allocates. Returns
    /// `None` for any other size.
    #[must_use]
    pub fn new(rom: &[u8]) -> Option<Self> {
        let size = rom.len();
        if size != 0x8000 && size != 0x1_0000 && size != Self::IMAGE_SIZE {
            return None;
        }
        let mut image = alloc::vec![0u8; Self::IMAGE_SIZE];
        for chunk in image.chunks_exact_mut(size) {
            chunk.copy_from_slice(rom);
        }
        Some(Self {
            rom: image,
            ram: alloc::vec![0u8; Self::RAM_SIZE],
            slice_low: 0,
            slice_middle: 0,
            slice_high: 0,
            is_rom_low: true,
            is_rom_middle: true,
            is_rom_high: true,
            // Matches Stella's `reset()`: a sentinel that can never satisfy
            // `(last_data & 0xe0) == 0x60` until a real access sets it.
            last_data: 0xff,
            last_address: 0xffff,
        })
    }

    /// Whether the previous access satisfies the `(value & 0xe0) == 0x60`
    /// gate (from a cart-window OR RIOT-zero-page address) that most of this
    /// scheme's hotspots require, per Stella's `checkBankSwitch`/`peek`/`poke`.
    const fn hotspots_active(&self) -> bool {
        (self.last_data & 0xe0) == 0x60
            && (self.last_address >= 0x1000 || self.last_address < 0x200)
    }

    fn bank_rom_lower(&mut self, value: u16) {
        self.is_rom_low = true;
        self.slice_low = value << 11;
    }
    fn bank_ram_lower(&mut self, value: u16) {
        self.is_rom_low = false;
        self.slice_low = value << 11;
    }
    fn bank_rom_middle(&mut self, value: u16) {
        self.is_rom_middle = true;
        self.slice_middle = value << 11;
    }
    fn bank_ram_middle(&mut self, value: u16) {
        self.is_rom_middle = false;
        self.slice_middle = value << 11;
    }
    fn bank_rom_high(&mut self, value: u16) {
        self.is_rom_high = true;
        self.slice_high = value << 8;
    }
    fn bank_ram_high(&mut self, value: u16) {
        self.is_rom_high = false;
        self.slice_high = value << 8;
    }

    /// Port of Stella's `Cartridge4A50::checkBankSwitch`: only called for
    /// accesses to addresses below `$1000` (TIA/RIOT-mirrored space, i.e.
    /// from [`Board::snoop_read`]/[`Board::snoop_write`]). Two independent
    /// hotspot chains: the first (gated by [`Self::hotspots_active`]) covers
    /// the three main segment-select hotspots plus four address-bit-toggle
    /// "helper" hotspots; the second (unconditional) covers a zero-page
    /// hotspot pair/quad keyed off the ACCESSED VALUE rather than address.
    fn check_bank_switch(&mut self, address: u16, value: u8) {
        if self.hotspots_active() {
            if address & 0x0f00 == 0x0c00 {
                self.bank_rom_high(address & 0xff);
            } else if address & 0x0f00 == 0x0d00 {
                self.bank_ram_high(address & 0x7f);
            } else if address & 0x0f40 == 0x0e00 {
                self.bank_rom_lower(address & 0x1f);
            } else if address & 0x0f40 == 0x0e40 {
                self.bank_ram_lower(address & 0xf);
            } else if address & 0x0f40 == 0x0f00 {
                self.bank_rom_middle(address & 0x1f);
            } else if address & 0x0f50 == 0x0f40 {
                self.bank_ram_middle(address & 0xf);
            } else if address & 0x0f00 == 0x0400 {
                self.slice_low ^= 0x800;
            } else if address & 0x0f00 == 0x0500 {
                self.slice_low ^= 0x1000;
            } else if address & 0x0f00 == 0x0800 {
                self.slice_middle ^= 0x800;
            } else if address & 0x0f00 == 0x0900 {
                self.slice_middle ^= 0x1000;
            }
        }

        let value16 = u16::from(value);
        if address & 0xf75 == 0x74 {
            self.bank_rom_high(value16);
        } else if address & 0xf75 == 0x75 {
            self.bank_ram_high(value16 & 0x7f);
        } else if address & 0xf7c == 0x78 {
            if value16 & 0xf0 == 0 {
                self.bank_rom_lower(value16 & 0xf);
            } else if value16 & 0xf0 == 0x40 {
                self.bank_ram_lower(value16 & 0xf);
            } else if value16 & 0xf0 == 0x90 {
                self.bank_rom_middle((value16 & 0xf) | 0x10);
            } else if value16 & 0xf0 == 0xc0 {
                self.bank_ram_middle(value16 & 0xf);
            }
        }
    }

    /// Record this access as the "previous" one for the NEXT access's
    /// hotspot check, matching Stella's unconditional `myLastData =
    /// value; myLastAddress = address & 0x1fff;` at the end of both
    /// `peek`/`poke`.
    fn record_access(&mut self, address: u16, value: u8) {
        self.last_data = value;
        self.last_address = address & 0x1fff;
    }
}

impl Board for Bank4A50 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let a = addr & 0x1fff;
        let value = if a & 0x1800 == 0x1000 {
            if self.is_rom_low {
                self.rom[usize::from(a & 0x7ff) + usize::from(self.slice_low)]
            } else {
                self.ram[usize::from(a & 0x7ff) + usize::from(self.slice_low)]
            }
        } else if (0x1800..=0x1dff).contains(&a) {
            if self.is_rom_middle {
                self.rom[usize::from(a & 0x7ff) + usize::from(self.slice_middle) + 0x1_0000]
            } else {
                self.ram[usize::from(a & 0x7ff) + usize::from(self.slice_middle)]
            }
        } else if a & 0x1f00 == 0x1e00 {
            if self.is_rom_high {
                self.rom[usize::from(a & 0xff) + usize::from(self.slice_high) + 0x1_0000]
            } else {
                self.ram[usize::from(a & 0xff) + usize::from(self.slice_high)]
            }
        } else {
            // The only remaining case in a 13-bit cart-window address is
            // `$1F00..=$1FFF` (fixed last 256 B of ROM, also this segment's
            // hotspot-trigger region).
            let value = self.rom[usize::from(a & 0xff) + 0x1_ff00];
            if self.hotspots_active() {
                self.slice_high = (self.slice_high & 0xf0ff) | ((a & 0x8) << 8) | ((a & 0x70) << 4);
            }
            value
        };
        self.record_access(a, value);
        value
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        let a = addr & 0x1fff;
        if a & 0x1800 == 0x1000 {
            if !self.is_rom_low {
                self.ram[usize::from(a & 0x7ff) + usize::from(self.slice_low)] = val;
            }
        } else if (0x1800..=0x1dff).contains(&a) {
            if !self.is_rom_middle {
                self.ram[usize::from(a & 0x7ff) + usize::from(self.slice_middle)] = val;
            }
        } else if a & 0x1f00 == 0x1e00 {
            if !self.is_rom_high {
                self.ram[usize::from(a & 0xff) + usize::from(self.slice_high)] = val;
            }
        } else if self.hotspots_active() {
            // `$1F00..=$1FFF`: ROM is fixed (never written), but still arms
            // the same `slice_high`-bit-toggle hotspot as the read path.
            self.slice_high = (self.slice_high & 0xf0ff) | ((a & 0x8) << 8) | ((a & 0x70) << 4);
        }
        self.record_access(a, val);
    }

    fn tier(&self) -> Tier {
        Tier::BestEffort
    }

    fn snoop_read(&mut self, addr: u16, val: u8) {
        self.check_bank_switch(addr, val);
        self.record_access(addr, val);
    }

    fn snoop_write(&mut self, addr: u16, val: u8) {
        self.check_bank_switch(addr, val);
        self.record_access(addr, val);
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
    /// F0 (Dynacom Megaboy): 64 KiB ROM, 16×4K banks, sequential-advance
    /// hotspot (BestEffort tier)
    BankF0(BankF0),
    /// E0 (Parker Bros): 8 KiB ROM, four independently-selectable 1K
    /// segments (BestEffort tier)
    BankE0(BankE0),
    /// 3F (Tigervision): variable-size ROM, low 2K bank-selectable via a
    /// `$3F`-low-byte write anywhere (BestEffort tier)
    Bank3F(Bank3F),
    /// 3E (Tigervision + RAM): `Bank3F` plus a `$3E` RAM-bank-select hotspot
    /// (BestEffort tier)
    Bank3E(Bank3E),
    /// EF (CPUWIZ): 64 KiB ROM, 16x4K banks, direct-select hotspots
    /// (BestEffort tier)
    BankEF(BankEF),
    /// DF (CPUWIZ): 128 KiB ROM, 32x4K banks, direct-select hotspots
    /// (BestEffort tier)
    BankDF(BankDF),
    /// BF (CPUWIZ): 256 KiB ROM, 64x4K banks, direct-select hotspots
    /// (BestEffort tier)
    BankBF(BankBF),
    /// UA (UA Ltd. / Digivision): 8 KiB ROM, 2x4K banks, snoop-based
    /// hotspots in TIA-mirrored space (BestEffort tier)
    BankUA(BankUA),
    /// 0840 (EconoBank): 8 KiB ROM, 2x4K banks, snoop-based hotspots
    /// (BestEffort tier)
    Bank0840(Bank0840),
    /// FE (Activision): 8 KiB ROM, 2x4K banks, JSR-stack-frame hotspot in
    /// TIA-mirrored space (BestEffort tier)
    BankFe(BankFe),
    /// SB (Superbank): 128/256 KiB ROM, 32/64x4K banks, address-encodes-bank
    /// hotspots in TIA-mirrored space (BestEffort tier)
    BankSb(BankSb),
    /// X07 (AtariAge multicart scheme): 64 KiB ROM, 16x4K banks, dual
    /// hotspot patterns in TIA-mirrored space (BestEffort tier)
    BankX07(BankX07),
    /// 4A50 (Supercat): 128 KiB ROM (32/64 KiB tiled) + 32 KiB RAM, three
    /// independently relocatable ROM/RAM segments, previous-access-gated
    /// hotspots in both cart-window and TIA-mirrored space (BestEffort tier)
    Bank4A50(Bank4A50),
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
            Self::BankF0(b) => b.cpu_read(addr),
            Self::BankE0(b) => b.cpu_read(addr),
            Self::Bank3F(b) => b.cpu_read(addr),
            Self::Bank3E(b) => b.cpu_read(addr),
            Self::BankEF(b) => b.cpu_read(addr),
            Self::BankDF(b) => b.cpu_read(addr),
            Self::BankBF(b) => b.cpu_read(addr),
            Self::BankUA(b) => b.cpu_read(addr),
            Self::Bank0840(b) => b.cpu_read(addr),
            Self::BankFe(b) => b.cpu_read(addr),
            Self::BankSb(b) => b.cpu_read(addr),
            Self::BankX07(b) => b.cpu_read(addr),
            Self::Bank4A50(b) => b.cpu_read(addr),
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
            Self::BankF0(b) => b.cpu_write(addr, val),
            Self::BankE0(b) => b.cpu_write(addr, val),
            Self::Bank3F(b) => b.cpu_write(addr, val),
            Self::Bank3E(b) => b.cpu_write(addr, val),
            Self::BankEF(b) => b.cpu_write(addr, val),
            Self::BankDF(b) => b.cpu_write(addr, val),
            Self::BankBF(b) => b.cpu_write(addr, val),
            Self::BankUA(b) => b.cpu_write(addr, val),
            Self::Bank0840(b) => b.cpu_write(addr, val),
            Self::BankFe(b) => b.cpu_write(addr, val),
            Self::BankSb(b) => b.cpu_write(addr, val),
            Self::BankX07(b) => b.cpu_write(addr, val),
            Self::Bank4A50(b) => b.cpu_write(addr, val),
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
            Self::BankF0(b) => b.tier(),
            Self::BankE0(b) => b.tier(),
            Self::Bank3F(b) => b.tier(),
            Self::Bank3E(b) => b.tier(),
            Self::BankEF(b) => b.tier(),
            Self::BankDF(b) => b.tier(),
            Self::BankBF(b) => b.tier(),
            Self::BankUA(b) => b.tier(),
            Self::Bank0840(b) => b.tier(),
            Self::BankFe(b) => b.tier(),
            Self::BankSb(b) => b.tier(),
            Self::BankX07(b) => b.tier(),
            Self::Bank4A50(b) => b.tier(),
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
            Self::BankF0(b) => b.tick(),
            Self::BankE0(b) => b.tick(),
            Self::Bank3F(b) => b.tick(),
            Self::Bank3E(b) => b.tick(),
            Self::BankEF(b) => b.tick(),
            Self::BankDF(b) => b.tick(),
            Self::BankBF(b) => b.tick(),
            Self::BankUA(b) => b.tick(),
            Self::Bank0840(b) => b.tick(),
            Self::BankFe(b) => b.tick(),
            Self::BankSb(b) => b.tick(),
            Self::BankX07(b) => b.tick(),
            Self::Bank4A50(b) => b.tick(),
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
            Self::BankF0(b) => b.tick_coprocessor(),
            Self::BankE0(b) => b.tick_coprocessor(),
            Self::Bank3F(b) => b.tick_coprocessor(),
            Self::Bank3E(b) => b.tick_coprocessor(),
            Self::BankEF(b) => b.tick_coprocessor(),
            Self::BankDF(b) => b.tick_coprocessor(),
            Self::BankBF(b) => b.tick_coprocessor(),
            Self::BankUA(b) => b.tick_coprocessor(),
            Self::Bank0840(b) => b.tick_coprocessor(),
            Self::BankFe(b) => b.tick_coprocessor(),
            Self::BankSb(b) => b.tick_coprocessor(),
            Self::BankX07(b) => b.tick_coprocessor(),
            Self::Bank4A50(b) => b.tick_coprocessor(),
        }
    }

    fn snoop_write(&mut self, addr: u16, val: u8) {
        match self {
            Self::Rom2K(b) => b.snoop_write(addr, val),
            Self::Rom4K(b) => b.snoop_write(addr, val),
            Self::BankF8(b) => b.snoop_write(addr, val),
            Self::BankF6(b) => b.snoop_write(addr, val),
            Self::BankF4(b) => b.snoop_write(addr, val),
            Self::BankCV(b) => b.snoop_write(addr, val),
            Self::BankFA(b) => b.snoop_write(addr, val),
            Self::BankDpc(b) => b.snoop_write(addr, val),
            Self::BankE7(b) => b.snoop_write(addr, val),
            Self::BankF0(b) => b.snoop_write(addr, val),
            Self::BankE0(b) => b.snoop_write(addr, val),
            Self::Bank3F(b) => b.snoop_write(addr, val),
            Self::Bank3E(b) => b.snoop_write(addr, val),
            Self::BankEF(b) => b.snoop_write(addr, val),
            Self::BankDF(b) => b.snoop_write(addr, val),
            Self::BankBF(b) => b.snoop_write(addr, val),
            Self::BankUA(b) => b.snoop_write(addr, val),
            Self::Bank0840(b) => b.snoop_write(addr, val),
            Self::BankFe(b) => b.snoop_write(addr, val),
            Self::BankSb(b) => b.snoop_write(addr, val),
            Self::BankX07(b) => b.snoop_write(addr, val),
            Self::Bank4A50(b) => b.snoop_write(addr, val),
        }
    }

    fn snoop_read(&mut self, addr: u16, val: u8) {
        match self {
            Self::Rom2K(b) => b.snoop_read(addr, val),
            Self::Rom4K(b) => b.snoop_read(addr, val),
            Self::BankF8(b) => b.snoop_read(addr, val),
            Self::BankF6(b) => b.snoop_read(addr, val),
            Self::BankF4(b) => b.snoop_read(addr, val),
            Self::BankCV(b) => b.snoop_read(addr, val),
            Self::BankFA(b) => b.snoop_read(addr, val),
            Self::BankDpc(b) => b.snoop_read(addr, val),
            Self::BankE7(b) => b.snoop_read(addr, val),
            Self::BankF0(b) => b.snoop_read(addr, val),
            Self::BankE0(b) => b.snoop_read(addr, val),
            Self::Bank3F(b) => b.snoop_read(addr, val),
            Self::Bank3E(b) => b.snoop_read(addr, val),
            Self::BankEF(b) => b.snoop_read(addr, val),
            Self::BankDF(b) => b.snoop_read(addr, val),
            Self::BankBF(b) => b.snoop_read(addr, val),
            Self::BankUA(b) => b.snoop_read(addr, val),
            Self::Bank0840(b) => b.snoop_read(addr, val),
            Self::BankFe(b) => b.snoop_read(addr, val),
            Self::BankSb(b) => b.snoop_read(addr, val),
            Self::BankX07(b) => b.snoop_read(addr, val),
            Self::Bank4A50(b) => b.snoop_read(addr, val),
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

/// Returns `true` if `needle` occurs in `haystack` at least `min_hits`
/// non-overlapping times, matching Stella's `BSPF::searchForBytes(..,
/// minhits)` semantics (used for signatures that need 2+ occurrences to
/// reduce false positives — plausible for a single stray match, much less
/// so for two).
fn count_bytes_at_least(haystack: &[u8], needle: &[u8], min_hits: usize) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    let mut count = 0;
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()] == *needle {
            count += 1;
            if count >= min_hits {
                return true;
            }
            i += needle.len(); // non-overlapping, matching Stella
        } else {
            i += 1;
        }
    }
    false
}

/// Port of Stella's `CartDetector::isProbablyE0`: search for known Parker
/// Bros bankswitch-hotspot opcode encodings targeting `$FE0..=$FF9`.
fn is_probably_e0(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 8] = [
        [0x8D, 0xE0, 0x1F], // STA $1FE0
        [0x8D, 0xE0, 0x5F], // STA $5FE0
        [0x8D, 0xE9, 0xFF], // STA $FFE9
        [0x0C, 0xE0, 0x1F], // NOP $1FE0
        [0xAD, 0xE0, 0x1F], // LDA $1FE0
        [0xAD, 0xE9, 0xFF], // LDA $FFE9
        [0xAD, 0xED, 0xFF], // LDA $FFED
        [0xAD, 0xF3, 0xBF], // LDA $BFF3
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
}

/// Port of Stella's `CartDetector::isProbably3F`: at least two occurrences
/// of `STA $3F` (a Tigervision cart with only one bank wouldn't need to
/// bankswitch at all, so a genuine 3F image writes it repeatedly).
fn is_probably_3f(rom: &[u8]) -> bool {
    count_bytes_at_least(rom, &[0x85, 0x3F], 2)
}

/// Port of Stella's `CartDetector::isProbably3E`: at least one `STA $3E`
/// (RAM-bank select) AND at least two `STA $3F` (ROM-bank select, same
/// reasoning as [`is_probably_3f`]).
fn is_probably_3e(rom: &[u8]) -> bool {
    count_bytes_at_least(rom, &[0x85, 0x3E], 1) && count_bytes_at_least(rom, &[0x85, 0x3F], 2)
}

/// Shared tail-signature check for the CPUWIZ EF/BF/DF family: newer carts
/// of these types (per AtariAge's "RevEng") store a 4-byte marker in the
/// last 8 bytes of the image — `"xFxF"` for plain, `"xFSC"` for the
/// Superchip variant (`x` = the scheme letter: `E`/`B`/`D`). Returns
/// `Some(true)` for a Superchip match, `Some(false)` for plain, `None` if
/// neither marker is present (ported from Stella's `isProbablyEF`/`BF`/`DF`).
fn ef_family_tail_signature(rom: &[u8], letter: u8) -> Option<bool> {
    let tail = &rom[rom.len().saturating_sub(8)..];
    if count_bytes_at_least(tail, &[letter, b'F', letter, b'F'], 1) {
        Some(false)
    } else if count_bytes_at_least(tail, &[letter, b'F', b'S', b'C'], 1) {
        Some(true)
    } else {
        None
    }
}

/// Port of Stella's `CartDetector::isProbablyEF`'s opcode fallback (used
/// when the tail signature isn't present — older EF carts predate the
/// marker convention): EF's bankswitching switches banks by accessing
/// `$FE0..=$FEF`, usually via a NOP or LDA to bank 0.
fn is_probably_ef_by_opcode(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 4] = [
        [0x0C, 0xE0, 0xFF], // NOP $FFE0
        [0xAD, 0xE0, 0xFF], // LDA $FFE0
        [0x0C, 0xE0, 0x1F], // NOP $1FE0
        [0xAD, 0xE0, 0x1F], // LDA $1FE0
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
}

/// Port of Stella's `CartDetector::isProbablyUA`: search for known UA /
/// Brazilian-Digivision bankswitch-hotspot opcode encodings.
fn is_probably_ua(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 6] = [
        [0x8D, 0x40, 0x02], // STA $240 (Funky Fish, Pleiades)
        [0xAD, 0x40, 0x02], // LDA $240
        [0xBD, 0x1F, 0x02], // LDA $21F,X (Gingerbread Man)
        [0x2C, 0xC0, 0x02], // BIT $2C0 (Time Pilot)
        [0x8D, 0xC0, 0x02], // STA $2C0 (Fathom, Vanguard)
        [0xAD, 0xC0, 0x02], // LDA $2C0 (Mickey)
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
        || count_bytes_at_least(rom, &[0x2C, 0xB0, 0x0F], 1) // BIT $FB0 (Digivision Beamrider)
}

/// Port of Stella's `CartDetector::isProbably0840`: at least two occurrences
/// of a known 0840 bankswitch-hotspot opcode encoding (a single access
/// wouldn't need to bankswitch at all).
fn is_probably_0840(rom: &[u8]) -> bool {
    const SIGNATURES_3: [[u8; 3]; 3] = [
        [0xAD, 0x00, 0x08], // LDA $0800
        [0xAD, 0x40, 0x08], // LDA $0840
        [0x2C, 0x00, 0x08], // BIT $0800
    ];
    if SIGNATURES_3
        .iter()
        .any(|sig| count_bytes_at_least(rom, sig, 2))
    {
        return true;
    }
    const SIGNATURES_4: [[u8; 4]; 2] = [
        [0x0C, 0x00, 0x08, 0x4C], // NOP $0800; JMP ...
        [0x0C, 0xFF, 0x0F, 0x4C], // NOP $0FFF; JMP ...
    ];
    SIGNATURES_4
        .iter()
        .any(|sig| count_bytes_at_least(rom, sig, 2))
}

/// Port of Stella's `CartDetector`'s inline F8-signature check used to guard
/// FE detection below: at least 2 occurrences of `STA $1FF9`/`STA $FFF9`
/// (F8's own hotspot) — a single hit wouldn't need to bankswitch at all, and
/// a real F8 image commonly re-emits its own hotspot write from both banks.
fn is_probably_f8_signature(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 2] = [
        [0x8D, 0xF9, 0x1F], // STA $1FF9
        [0x8D, 0xF9, 0xFF], // STA $FFF9
    ];
    SIGNATURES
        .iter()
        .any(|sig| count_bytes_at_least(rom, sig, 2))
}

/// Port of Stella's `CartDetector::isProbablyFE`: a small, exact signature
/// list (attributed to the MESS project) for the five known FE titles' boot
/// code, each anchored on the `JSR`/`BNE` sequence that triggers the
/// bank-switch stack-frame trick. Checked with `!is_probably_f8_signature`
/// in `detect()` (matching Stella's `isProbablyFE(image) && !f8` guard) so a
/// real F8 image is never misdetected as FE.
fn is_probably_fe(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 5]; 5] = [
        [0x20, 0x00, 0xD0, 0xC6, 0xC5], // JSR $D000; DEC $C5  (Decathlon)
        [0x20, 0xC3, 0xF8, 0xA5, 0x82], // JSR $F8C3; LDA $82  (Robot Tank)
        [0xD0, 0xFB, 0x20, 0x73, 0xFE], // BNE $FB; JSR $FE73  (Space Shuttle NTSC/PAL)
        [0xD0, 0xFB, 0x20, 0x68, 0xFE], // BNE $FB; JSR $FE68  (Space Shuttle SECAM)
        [0x20, 0x00, 0xF0, 0x84, 0xD6], // JSR $F000; STY $D6  (Thwocker)
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
}

/// Port of Stella's `CartDetector::isProbablyX07`: search for any of the six
/// known opcode encodings of `LDA`/`NOP $080D`/`$081D`/`$082D` — X07's
/// direct bank-select hotspots (`docs/cart.md`).
fn is_probably_x07(rom: &[u8]) -> bool {
    const SIGNATURES: [[u8; 3]; 6] = [
        [0xAD, 0x0D, 0x08], // LDA $080D
        [0xAD, 0x1D, 0x08], // LDA $081D
        [0xAD, 0x2D, 0x08], // LDA $082D
        [0x0C, 0x0D, 0x08], // NOP $080D
        [0x0C, 0x1D, 0x08], // NOP $081D
        [0x0C, 0x2D, 0x08], // NOP $082D
    ];
    SIGNATURES.iter().any(|sig| contains_bytes(rom, sig))
}

/// Port of Stella's `CartDetector::isProbably4A50`: a 4A50 cart stores
/// `$4A50` (its own namesake address) at the NMI vector, which in this
/// scheme's rev-1 layout always lives in the last page of ROM at
/// `$1FFA-$1FFB` — checked here as the raw image's last-6th/-5th bytes
/// (relative-from-end indexing, so it works whether `rom` is the 32/64/128
/// KiB dump `detect()` actually sees). Falling back to a second heuristic:
/// the program's RESET vector points into the last page (`$1Fxx`) AND its
/// first instruction there is a 3-byte absolute `NOP $6Exx`/`NOP $6Fxx`
/// (opcode `$0C`, target high byte `$6E`/`$6F`) — like Stella, this second
/// check indexes the FIXED absolute offsets `$FFFC`/`$FFFD`, so it's only
/// meaningful (and only ever reached via `detect()`) for 64 KiB+ images.
fn is_probably_4a50(rom: &[u8]) -> bool {
    let len = rom.len();
    if len < 6 {
        return false;
    }
    if rom[len - 6] == 0x50 && rom[len - 5] == 0x4A {
        return true;
    }
    if len <= 0xFFFE {
        return false;
    }
    let reset_hi = rom[0xFFFD];
    let reset_lo = rom[0xFFFC];
    if reset_hi & 0x1F != 0x1F {
        return false;
    }
    let target = usize::from(reset_hi) * 256 + usize::from(reset_lo);
    target + 2 < len && rom[target] == 0x0C && (rom[target + 2] & 0xFE) == 0x6E
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
            // 8 KiB: checked in the same priority order Stella's own
            // CartDetector uses at this size (SC, E0, 3E, 3F, UA, FE, 0840,
            // ... default F8) — Superchip (F8SC) via its RAM-shadow
            // signature, E0 (Parker Bros)/3E/3F (Tigervision)/UA/FE/0840 via
            // their hotspot-opcode signatures, falling back to plain F8
            // (Curated, the far more common scheme). FE is guarded by
            // `!is_probably_f8_signature`, matching Stella's own
            // `isProbablyFE(image) && !f8` so a real F8 image is never
            // misdetected (`T-0402-006`, DONE).
            if is_probably_superchip(rom) {
                BankF8::new(rom)
                    .map(BankF8::with_superchip)
                    .map(Cartridge::BankF8)
            } else if is_probably_e0(rom) {
                BankE0::new(rom).map(Cartridge::BankE0)
            } else if is_probably_3e(rom) {
                Bank3E::new(rom, 32).map(Cartridge::Bank3E)
            } else if is_probably_3f(rom) {
                Bank3F::new(rom).map(Cartridge::Bank3F)
            } else if is_probably_ua(rom) {
                BankUA::new(rom).map(Cartridge::BankUA)
            } else if is_probably_fe(rom) && !is_probably_f8_signature(rom) {
                BankFe::new(rom).map(Cartridge::BankFe)
            } else if is_probably_0840(rom) {
                Bank0840::new(rom).map(Cartridge::Bank0840)
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
            // signature, then 3E/3F (Tigervision, also possible at this
            // size per Stella's own CartDetector), falling back to plain F4
            // (Curated, the far more common scheme at 32 KiB).
            if is_probably_superchip(rom) {
                BankF4::new(rom)
                    .map(BankF4::with_superchip)
                    .map(Cartridge::BankF4)
            } else if is_probably_3e(rom) {
                Bank3E::new(rom, 32).map(Cartridge::Bank3E)
            } else if is_probably_3f(rom) {
                Bank3F::new(rom).map(Cartridge::Bank3F)
            } else {
                BankF4::new(rom).map(Cartridge::BankF4)
            }
        }
        // 64 KiB: checked in the same relative priority Stella's own
        // CartDetector uses at this size among the schemes implemented here
        // (3E, 3F, 4A50, EF, X07, then default F0) — EFF/CDF/3EX (also
        // possible at 64 KiB per Stella) aren't implemented yet, so they're
        // simply skipped in the chain (same pattern already used elsewhere).
        0x10000 => {
            if is_probably_3e(rom) {
                Bank3E::new(rom, 32).map(Cartridge::Bank3E)
            } else if is_probably_3f(rom) {
                Bank3F::new(rom).map(Cartridge::Bank3F)
            } else if is_probably_4a50(rom) {
                Bank4A50::new(rom).map(Cartridge::Bank4A50)
            } else if let Some(sc) = ef_family_tail_signature(rom, b'E') {
                let ef = BankEF::new(rom)?;
                Some(Cartridge::BankEF(if sc { ef.with_superchip() } else { ef }))
            } else if is_probably_ef_by_opcode(rom) {
                BankEF::new(rom).map(Cartridge::BankEF)
            } else if is_probably_x07(rom) {
                BankX07::new(rom).map(Cartridge::BankX07)
            } else {
                BankF0::new(rom).map(Cartridge::BankF0)
            }
        }
        // 128 KiB: 3E / 3F (Tigervision) checked first — matching Stella's
        // own priority order at this size — then DF (CPUWIZ) via its tail
        // signature, then 4A50; falls back to SB (BestEffort), matching
        // Stella's own chain at this size, which defaults straight to SB once
        // 3E/DF/3F/4A50/CDF are ruled out (`T-0402-011`/`T-0402-014`, DONE —
        // CDF/3EX remain unimplemented and are simply skipped, same as 64 KiB
        // above).
        0x20000 => {
            if is_probably_3e(rom) {
                Bank3E::new(rom, 32).map(Cartridge::Bank3E)
            } else if let Some(sc) = ef_family_tail_signature(rom, b'D') {
                let df = BankDF::new(rom)?;
                Some(Cartridge::BankDF(if sc { df.with_superchip() } else { df }))
            } else if is_probably_3f(rom) {
                Bank3F::new(rom).map(Cartridge::Bank3F)
            } else if is_probably_4a50(rom) {
                Bank4A50::new(rom).map(Cartridge::Bank4A50)
            } else {
                BankSb::new(rom).map(Cartridge::BankSb)
            }
        }
        // 256 KiB: 3E checked first, then BF (CPUWIZ) via its tail
        // signature, then 3F — matching Stella's own priority order at this
        // size. Falls back to SB (BestEffort), same reasoning as 128 KiB
        // above (`T-0402-011`, DONE).
        0x40000 => {
            if is_probably_3e(rom) {
                Bank3E::new(rom, 32).map(Cartridge::Bank3E)
            } else if let Some(sc) = ef_family_tail_signature(rom, b'B') {
                let bf = BankBF::new(rom)?;
                Some(Cartridge::BankBF(if sc { bf.with_superchip() } else { bf }))
            } else if is_probably_3f(rom) {
                Bank3F::new(rom).map(Cartridge::Bank3F)
            } else {
                BankSb::new(rom).map(Cartridge::BankSb)
            }
        }
        // T-0401-003 (DONE): Superchip variants F8SC/F6SC/F4SC — dispatched
        // above via is_probably_superchip().
        // T-0401-004 (DONE): 3F (Tigervision) / 3E (Boulder Dash) — dispatched
        // above at 8/32/64/128/256 KiB via is_probably_3f()/is_probably_3e().
        // 3E+ (an ARM-assisted successor) still unimplemented.
        // T-0402-008/009/010 (DONE): EF/EFSC, DF/DFSC, BF/BFSC — dispatched
        // above at 64/128/256 KiB via ef_family_tail_signature() (and EF's
        // opcode fallback for pre-marker-convention images).
        // T-0402-012/013 (DONE): UA, 0840 — dispatched above at 8 KiB via
        // is_probably_ua()/is_probably_0840(), using the new snoop_read hook
        // (they only need the ACCESS ADDRESS, not the value, so a simpler
        // case than FE below).
        // T-0401-005 (DONE): DPC (Pitfall II, Curated) — see the 0x2800..=0x2900 arm above.
        // T-0402-006 (DONE): FE — dispatched above at 8 KiB via
        // is_probably_fe(), guarded by !is_probably_f8_signature().
        // T-0402-011 (DONE): SB, X07 — dispatched above at 64/128/256 KiB.
        // T-0402-014 (DONE): 4A50 — dispatched above at 64/128 KiB via
        // is_probably_4a50(); three independently relocatable ROM/RAM
        // segments plus a previous-access-dependent hotspot state machine
        // (Stella's `Cartridge4A50::checkBankSwitch`), ported faithfully onto
        // `Board::snoop_read`/`snoop_write` (below `$1000`) plus a smaller
        // in-window instance of the same check (`$1F00-$1FFF`).
        // TODO(T-0402-015): AR/Supercharger (BestEffort) — deliberately NOT
        // attempted in the same pass as 4A50 above (`v1.5.0`): even
        // "fast-load" (ROM-image-only, skipping the real tape-audio "sound-
        // load" mode entirely) needs a bank-config decode, a delayed-write
        // protocol keyed on 5 DISTINCT bus accesses (needing accumulation
        // across `snoop_read`/`snoop_write`/`cpu_read`/`cpu_write` combined,
        // since Stella tracks this via a global CPU-side counter this crate
        // has no equivalent of), AND a synthesized dummy 6502 BIOS stub
        // whose exact bytes (Stella's `ourDummyROMCode`/`scrom.asm`) haven't
        // been sourced yet — a substantially larger, still-separately-scoped
        // item versus every other scheme in this catalogue, including 4A50.
        // TODO(T-0401-006): DPC+, CDF/CDFJ/CDFJ+ (BestEffort) — both need a
        // full ARM7TDMI Thumb interpreter via tick_coprocessor (see
        // Gopher2600's arm.go/thumb.go for the reference implementation);
        // deliberately not attempted here, same call as v0.4.x's own
        // scoping of this family.
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

    #[test]
    fn f0_sequential_bank_advance_wraps() {
        let mut img = [0u8; 0x10000];
        for b in 0..16u8 {
            img[usize::from(b) * 0x1000] = b; // marker byte at each bank's start
        }
        let mut board = BankF0::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 15); // default (start) bank is 15
        board.cpu_read(0x1FF0); // advance: wraps 15 -> 0
        assert_eq!(board.cpu_read(0x1000), 0);
        board.cpu_read(0x1FF0); // advance: 0 -> 1
        assert_eq!(board.cpu_read(0x1000), 1);
    }

    #[test]
    fn e0_independent_segment_selection_and_fixed_last_bank() {
        let mut img = [0u8; 0x2000];
        for b in 0..8u8 {
            img[usize::from(b) * 0x400] = b; // marker at each 1 KiB bank's start
        }
        let mut board = BankE0::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        // Default segments (Stella's non-randomized reset path): 4, 5, 6;
        // segment 3 permanently fixed to the last bank (7).
        assert_eq!(board.cpu_read(0x1000), 4);
        assert_eq!(board.cpu_read(0x1400), 5);
        assert_eq!(board.cpu_read(0x1800), 6);
        assert_eq!(board.cpu_read(0x1C00), 7);
        board.cpu_read(0x1FE2); // select bank 2 into segment 0
        assert_eq!(board.cpu_read(0x1000), 2);
        assert_eq!(board.cpu_read(0x1C00), 7); // segment 3 unaffected
    }

    #[test]
    fn bank3f_selects_bank_via_snoop_write_on_low_byte_3f() {
        let mut img = [0u8; 0x1000]; // 4 KiB = 2 banks of 2 KiB
        img[0] = 0x11; // bank 0, first byte
        img[0x800] = 0x22; // bank 1, first byte
        let mut board = Bank3F::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(
            board.cpu_read(0x1800),
            0x22,
            "high segment always maps the last bank"
        );
        assert_eq!(
            board.cpu_read(0x1000),
            0x11,
            "low segment defaults to bank 0"
        );
        board.snoop_write(0x003F, 1); // any address whose low byte is $3F
        assert_eq!(board.cpu_read(0x1000), 0x22);
        board.snoop_write(0x0040, 0); // NOT a $..3F address -- no switch
        assert_eq!(board.cpu_read(0x1000), 0x22);
    }

    #[test]
    fn bank3e_rom_and_ram_bank_select_independent() {
        let mut img = [0u8; 0x1000]; // 4 KiB = 2 ROM banks of 2 KiB
        img[0] = 0x11;
        img[0x800] = 0x22;
        let mut board = Bank3E::new(&img, 2).unwrap(); // 2 KiB RAM = 2x1 KiB banks
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0x11);
        assert_eq!(
            board.cpu_read(0x1800),
            0x22,
            "high segment always fixed to the last ROM bank"
        );
        board.snoop_write(0x003E, 1); // select RAM bank 1 into the low segment
        board.cpu_write(0x1000, 0x55);
        assert_eq!(board.cpu_read(0x1000), 0x55);
        board.snoop_write(0x003F, 1); // switch back to ROM bank 1
        assert_eq!(board.cpu_read(0x1000), 0x22);
    }

    #[test]
    fn detect_resolves_e0_via_hotspot_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        img[0x0100] = 0x8D; // STA $1FE0 (Parker Bros hotspot)
        img[0x0101] = 0xE0;
        img[0x0102] = 0x1F;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankE0(_)));
    }

    #[test]
    fn detect_resolves_3f_via_repeated_sta_3f_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        img[0x0100] = 0x85; // STA $3F (x2 -- 3F needs 2+ occurrences)
        img[0x0101] = 0x3F;
        img[0x0200] = 0x85;
        img[0x0201] = 0x3F;
        assert!(matches!(detect(&img).unwrap(), Cartridge::Bank3F(_)));
    }

    #[test]
    fn detect_resolves_3e_via_sta_3e_and_repeated_sta_3f_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        img[0x0100] = 0x85; // STA $3E
        img[0x0101] = 0x3E;
        img[0x0200] = 0x85; // STA $3F (x2)
        img[0x0201] = 0x3F;
        img[0x0300] = 0x85;
        img[0x0301] = 0x3F;
        assert!(matches!(detect(&img).unwrap(), Cartridge::Bank3E(_)));
    }

    #[test]
    fn detect_resolves_f0_at_64kib() {
        assert!(matches!(
            detect(&[0u8; 0x10000]).unwrap(),
            Cartridge::BankF0(_)
        ));
    }

    #[test]
    fn bank_ef_direct_select_hotspot_and_superchip() {
        let mut img = alloc::vec![0u8; 0x10000];
        img[3 * 0x1000] = 0x33; // bank 3 marker
        let mut board = BankEF::new(&img).unwrap().with_superchip();
        assert_eq!(board.tier(), Tier::BestEffort);
        board.cpu_read(0x1FE3); // direct-select bank 3 (not sequential, unlike F0)
        assert_eq!(board.cpu_read(0x1000), 0x33);
        board.cpu_write(0x1000, 0x42); // superchip write-low
        assert_eq!(board.cpu_read(0x1080), 0x42); // read-high
    }

    #[test]
    fn bank_df_direct_select_hotspot() {
        let mut img = alloc::vec![0u8; 0x20000];
        img[5 * 0x1000] = 0x55;
        let mut board = BankDF::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        board.cpu_read(0x1FC5); // select bank 5
        assert_eq!(board.cpu_read(0x1000), 0x55);
    }

    #[test]
    fn bank_bf_direct_select_hotspot() {
        let mut img = alloc::vec![0u8; 0x40000];
        img[40 * 0x1000] = 0x28;
        let mut board = BankBF::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        board.cpu_read(0x1F80 + 40); // select bank 40
        assert_eq!(board.cpu_read(0x1000), 0x28);
    }

    #[test]
    fn detect_resolves_ef_via_tail_signature_plain_and_superchip() {
        let mut img = alloc::vec![0u8; 0x10000];
        let len = img.len();
        img[len - 8..len - 4].copy_from_slice(b"EFEF");
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankEF(_)));

        let mut img_sc = alloc::vec![0u8; 0x10000];
        let len = img_sc.len();
        img_sc[len - 8..len - 4].copy_from_slice(b"EFSC");
        let mut board = detect(&img_sc).unwrap();
        assert!(matches!(board, Cartridge::BankEF(_)));
        board.cpu_write(0x1000, 0x11); // only persists if Superchip actually engaged
        assert_eq!(board.cpu_read(0x1080), 0x11);
    }

    #[test]
    fn detect_resolves_ef_via_opcode_fallback_when_no_tail_signature() {
        let mut img = alloc::vec![0u8; 0x10000];
        img[0x100] = 0xAD; // LDA $1FE0
        img[0x101] = 0xE0;
        img[0x102] = 0x1F;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankEF(_)));
    }

    #[test]
    fn detect_resolves_df_via_tail_signature() {
        let mut img = alloc::vec![0u8; 0x20000];
        let len = img.len();
        img[len - 8..len - 4].copy_from_slice(b"DFDF");
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankDF(_)));
    }

    #[test]
    fn detect_resolves_bf_via_tail_signature() {
        let mut img = alloc::vec![0u8; 0x40000];
        let len = img.len();
        img[len - 8..len - 4].copy_from_slice(b"BFBF");
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankBF(_)));
    }

    #[test]
    fn detect_128kib_and_256kib_without_signature_falls_back_to_sb() {
        // No 3E/3F/DF/BF signature present -- SB is now implemented and is
        // Stella's own default fallback at these two sizes (`T-0402-011`).
        assert!(matches!(
            detect(&alloc::vec![0u8; 0x20000]).unwrap(),
            Cartridge::BankSb(_)
        ));
        assert!(matches!(
            detect(&alloc::vec![0u8; 0x40000]).unwrap(),
            Cartridge::BankSb(_)
        ));
    }

    #[test]
    fn bank_ua_snoop_selects_bank_via_read_or_write() {
        let mut img = [0u8; 0x2000];
        img[0] = 0xAA; // bank 0
        img[0x1000] = 0xBB; // bank 1
        let mut board = BankUA::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0xAA, "default start bank is 0");
        board.snoop_read(0x0240, 0); // an observed READ of $240 also switches
        assert_eq!(board.cpu_read(0x1000), 0xBB);
        board.snoop_write(0x0220, 0); // an observed WRITE of $220 switches back
        assert_eq!(board.cpu_read(0x1000), 0xAA);
    }

    #[test]
    fn bank_0840_snoop_selects_bank_via_read_or_write() {
        let mut img = [0u8; 0x2000];
        img[0] = 0xAA;
        img[0x1000] = 0xBB;
        let mut board = Bank0840::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0xAA);
        board.snoop_read(0x0840, 0);
        assert_eq!(board.cpu_read(0x1000), 0xBB);
        board.snoop_write(0x0800, 0);
        assert_eq!(board.cpu_read(0x1000), 0xAA);
    }

    #[test]
    fn detect_resolves_ua_via_opcode_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        img[0x100] = 0x8D; // STA $240
        img[0x101] = 0x40;
        img[0x102] = 0x02;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankUA(_)));
    }

    #[test]
    fn detect_resolves_0840_via_repeated_opcode_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        img[0x100] = 0xAD; // LDA $0800 (x2 -- 0840 needs 2+ occurrences)
        img[0x101] = 0x00;
        img[0x102] = 0x08;
        img[0x200] = 0xAD;
        img[0x201] = 0x00;
        img[0x202] = 0x08;
        assert!(matches!(detect(&img).unwrap(), Cartridge::Bank0840(_)));
    }

    #[test]
    fn bank_fe_switches_via_stack_frame_value_after_01fe_touch() {
        let mut img = [0u8; 0x2000];
        img[0] = 0xAA; // bank 0
        img[0x1000] = 0xBB; // bank 1
        let mut board = BankFe::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0xAA, "default start bank is 0");
        // A JSR's two-byte stack push: PCH to $01FE (arms the flag, value
        // irrelevant), then PCL to $01FD (its value picks the bank).
        board.snoop_write(0x01FE, 0xAB);
        board.snoop_write(0x01FD, 0x00); // (0x00>>5)^7 & 1 = 1 -> bank 1
        assert_eq!(board.cpu_read(0x1000), 0xBB);
        board.snoop_write(0x01FE, 0xAB);
        board.snoop_write(0x01FD, 0xE0); // (0xE0>>5)^7 & 1 = 0 -> bank 0
        assert_eq!(board.cpu_read(0x1000), 0xAA);
    }

    #[test]
    fn detect_resolves_fe_via_boot_signature() {
        let mut img = [0u8; 0x2000];
        img[0x00] = 0x01; // rule out the trivial all-zero Superchip match
        // Decathlon's "JSR $D000; DEC $C5" boot signature.
        img[0x100..0x105].copy_from_slice(&[0x20, 0x00, 0xD0, 0xC6, 0xC5]);
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankFe(_)));
    }

    #[test]
    fn bank_sb_hotspot_selects_bank_via_address_low_bits() {
        let mut img = alloc::vec![0u8; 0x20000]; // 128 KiB -> 32 banks
        img[0] = 0xAA; // bank 0
        img[5 * 0x1000] = 0xCC; // bank 5
        let mut board = BankSb::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0xAA, "default start bank is 0");
        board.snoop_read(0x0805, 0); // low byte 0x05 -> bank 5
        assert_eq!(board.cpu_read(0x1000), 0xCC);
        board.snoop_write(0x0800, 0); // low byte 0x00 -> bank 0
        assert_eq!(board.cpu_read(0x1000), 0xAA);
    }

    #[test]
    fn bank_x07_direct_select_and_high_bank_toggle() {
        let mut img = alloc::vec![0u8; 0x10000]; // 64 KiB -> 16 banks
        img[0] = 0xAA; // bank 0
        img[14 * 0x1000] = 0xCC; // bank 14
        img[15 * 0x1000] = 0xDD; // bank 15
        let mut board = BankX07::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        assert_eq!(board.cpu_read(0x1000), 0xAA, "default start bank is 0");
        board.snoop_read(0x08ED, 0); // direct select: bits 4-7 = 0xE -> bank 14
        assert_eq!(board.cpu_read(0x1000), 0xCC);
        // The secondary toggle only applies while the bank is 14 or 15.
        board.snoop_write(0x0040, 0); // bit 6 set -> bank 15
        assert_eq!(board.cpu_read(0x1000), 0xDD);
        board.snoop_write(0x0000, 0); // bit 6 clear -> bank 14
        assert_eq!(board.cpu_read(0x1000), 0xCC);
    }

    #[test]
    fn detect_resolves_x07_via_opcode_signature() {
        let mut img = alloc::vec![0u8; 0x10000];
        img[0x100] = 0xAD; // LDA $080D
        img[0x101] = 0x0D;
        img[0x102] = 0x08;
        assert!(matches!(detect(&img).unwrap(), Cartridge::BankX07(_)));
    }

    #[test]
    fn bank4a50_defaults_to_rom_slice_zero_in_low_segment() {
        let mut img = alloc::vec![0u8; 0x10000]; // 64 KiB
        img[0x0000] = 0x11; // slice_low default is 0, no +0x10000 offset
        let mut board = Bank4A50::new(&img).unwrap();
        assert_eq!(board.tier(), Tier::BestEffort);
        // $1000-$17FF: ROM, slice_low = 0 -> the image's very first byte.
        assert_eq!(board.cpu_read(0x1000), 0x11);
    }

    #[test]
    fn bank4a50_fixed_last_page_reads_end_of_tiled_image() {
        let mut img = alloc::vec![0u8; 0x8000]; // 32 KiB, tiled x4 to fill 128 KiB
        let last = img.len() - 1;
        img[last] = 0x99; // lands at the tiled image's very last byte
        let mut board = Bank4A50::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1FFF), 0x99);
    }

    #[test]
    fn bank4a50_hotspot_switches_rom_lower_segment() {
        let mut img = alloc::vec![0u8; 0x10000]; // 64 KiB
        img[0x2800] = 0xAB; // lands at slice_low = 5 << 11 after the switch
        let mut board = Bank4A50::new(&img).unwrap();
        assert_eq!(board.cpu_read(0x1000), img[0], "default slice_low is 0");
        // Arm the hotspot gate: a cart-window access whose value satisfies
        // (value & 0xe0) == 0x60 (`hotspots_active`'s condition).
        board.cpu_write(0x1000, 0x60);
        // Enable 2K of ROM at $1000-$17FF, slice index 5 (address bits 0-4) —
        // `address & 0x0f40 == 0x0e00` (TIA/RIOT-mirrored space).
        board.snoop_write(0x0E05, 0);
        assert_eq!(board.cpu_read(0x1000), 0xAB);
    }

    #[test]
    fn bank4a50_hotspot_switches_ram_lower_segment_and_round_trips() {
        let img = alloc::vec![0u8; 0x10000]; // 64 KiB
        let mut board = Bank4A50::new(&img).unwrap();
        board.cpu_write(0x1000, 0x60); // arm the hotspot gate
        // Enable 2K of RAM at $1000-$17FF, slice index 3 —
        // `address & 0x0f40 == 0x0e40`.
        board.snoop_write(0x0E43, 0);
        board.cpu_write(0x1000, 0x77);
        assert_eq!(board.cpu_read(0x1000), 0x77, "RAM segment round-trips");
    }

    #[test]
    fn bank4a50_zero_page_hotspot_selects_rom_lower_from_value_unconditionally() {
        // The second checkBankSwitch chain arms straight off the accessed
        // VALUE at a fixed zero-page address pattern — no prior "armed"
        // access needed, unlike the main chain above.
        let mut img = alloc::vec![0u8; 0x10000];
        img[0x0800] = 0xCD; // slice index 1 << 11 = 0x800
        let mut board = Bank4A50::new(&img).unwrap();
        board.snoop_write(0x0078, 0x01); // address & 0xf7c == 0x78, value & 0xf0 == 0
        assert_eq!(board.cpu_read(0x1000), 0xCD);
    }

    #[test]
    fn detect_resolves_4a50_via_nmi_vector_signature() {
        let mut img64 = alloc::vec![0u8; 0x10000];
        let len64 = img64.len();
        img64[len64 - 6] = 0x50;
        img64[len64 - 5] = 0x4A;
        assert!(matches!(detect(&img64).unwrap(), Cartridge::Bank4A50(_)));

        let mut img128 = alloc::vec![0u8; 0x20000];
        let len128 = img128.len();
        img128[len128 - 6] = 0x50;
        img128[len128 - 5] = 0x4A;
        assert!(matches!(detect(&img128).unwrap(), Cartridge::Bank4A50(_)));
    }

    #[test]
    fn detect_resolves_4a50_via_boot_nop_signature() {
        let mut img = alloc::vec![0u8; 0x10000];
        img[0xFFFD] = 0xFF; // reset_hi & 0x1f == 0x1f
        img[0xFFFC] = 0x00; // reset_lo
        // target = 0xFF00: a 3-byte absolute NOP ($0C) targeting $6Exx.
        img[0xFF00] = 0x0C;
        img[0xFF02] = 0x6E;
        assert!(matches!(detect(&img).unwrap(), Cartridge::Bank4A50(_)));
    }
}
