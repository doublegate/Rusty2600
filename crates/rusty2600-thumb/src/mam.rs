//! The LPC2000-family Memory Accelerator Module (MAMCR/MAMTIM registers).
//!
//! Ported from Gopher2600's `mam.go`, simplified: the reference threads a
//! whole-application preferences/environment object through this struct
//! purely to pick the initial MAM mode from user settings; this crate has no
//! such concept; a future `Board` picks the mode at construction instead.

/// The MAM operating mode. `Disabled`/`Partial`/`Full` map directly onto the
/// hardware's `MAMCR` register values `0`/`1`/`2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MamMode {
    /// `MAMCR = 0`: every access goes to flash at full latency.
    #[default]
    Disabled,
    /// `MAMCR = 1`: only program (instruction-fetch) accesses may use the
    /// prefetch/branch-trail latches; data accesses always pay full latency.
    Partial,
    /// `MAMCR = 2`: both program and data accesses may use the latches.
    Full,
}

impl MamMode {
    const fn from_bits(v: u32) -> Self {
        match v {
            1 => Self::Partial,
            2 => Self::Full,
            _ => Self::Disabled,
        }
    }

    const fn to_bits(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::Partial => 1,
            Self::Full => 2,
        }
    }
}

/// The two memory-mapped MAM registers plus the cross-instruction latch
/// state [`crate::cycles`] reads to approximate flash-wait-state timing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mam {
    mode: MamMode,
    mamtim: u32,
    /// Address (masked to its latch line) of the last prefetch and data read.
    pub(crate) prefetch_latch: u32,
    pub(crate) data_latch: u32,
    /// Address of the last branch target (the branch-trail buffer).
    pub(crate) branch_latch: u32,
    /// Set when the previous cycle was a data read, aborting any pending
    /// prefetch latch (a plain data access invalidates the instruction
    /// prefetch buffer on real LPC2000 hardware).
    pub(crate) prefetch_aborted: bool,
}

/// The two register addresses this module answers for. A future `Board`
/// supplying its own per-coprocessor memory map will pass its own addresses
/// in; these are the values Gopher2600's LPC2000-alike default map uses.
pub const MAMCR_ADDR: u32 = 0xE01F_C000;
/// See [`MAMCR_ADDR`].
pub const MAMTIM_ADDR: u32 = 0xE01F_C004;

impl Mam {
    /// Current operating mode.
    #[must_use]
    pub const fn mode(&self) -> MamMode {
        self.mode
    }

    /// Read `MAMCR`/`MAMTIM`; returns `None` for any other address (the
    /// caller then tries other peripheral fallbacks or faults).
    #[must_use]
    pub fn read(&self, addr: u32) -> Option<u32> {
        if addr == MAMCR_ADDR {
            Some(self.mode.to_bits())
        } else if addr == MAMTIM_ADDR {
            Some(self.mamtim)
        } else {
            None
        }
    }

    /// Write `MAMCR`/`MAMTIM`; returns whether the address was recognized.
    pub fn write(&mut self, addr: u32, val: u32) -> bool {
        if addr == MAMCR_ADDR {
            self.mode = MamMode::from_bits(val);
            true
        } else if addr == MAMTIM_ADDR {
            self.mamtim = val;
            true
        } else {
            false
        }
    }
}
