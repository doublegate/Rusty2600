//! N/S/I cycle accounting and the MAM prefetch-latch approximation.
//!
//! Ported from Gopher2600's `cycles.go` + `cycles_arm7tdmi.go`. **This is an
//! approximate hardware-timing model even in the reference implementation**
//! — `stretchedCycles` is accumulated as a float, and Gopher2600's own
//! comments admit the constants involved are unverified ("no idea if this
//! value is sufficient"). This port does not claim cycle-exactness for the
//! coprocessor path; it faithfully reproduces the same approximation.
//!
//! One further simplification versus the reference: Gopher2600's flash
//! wait-state length is looked up per memory REGION (`architecture.Map`,
//! a per-cartridge address table deliberately out of scope for this crate —
//! see the crate-level docs). This crate models a single flat memory region
//! (one wait-state length, MAM-eligible) until a future `Board` wiring pass
//! needs to distinguish flash from RAM regions with different latencies.

use crate::Arm7Tdmi;
use crate::mam::MamMode;
use crate::memory::ThumbMemory;

/// The three ARM7TDMI bus-cycle types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    /// Nonsequential — a fresh address, full decode/latency cost.
    N,
    /// Sequential — the next address after the last one, often cheaper.
    S,
    /// Internal — no bus activity (register-only operations, extra shift
    /// cycles, multiply cycles).
    I,
}

/// The kind of bus activity a cycle represents, matching Gopher2600's
/// `busAccess` — used to decide whether the MAM prefetch latch should be
/// consulted, and (for [`BusAccess::is_data_access`]) whether an access is
/// data or instruction-fetch traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusAccess {
    /// An instruction-opcode fetch.
    Prefetch,
    /// A branch target fetch (consults the branch-trail buffer).
    Branch,
    DataRead,
    DataWrite,
}

impl BusAccess {
    /// Whether this access is data traffic rather than an instruction fetch
    /// — equivalent to asking whether `Prot0` is 0 or 1 in ARM bus terms.
    #[must_use]
    pub const fn is_data_access(self) -> bool {
        matches!(self, Self::DataRead | Self::DataWrite)
    }
}

/// How the branch-trail buffer was used for the most recent branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchTrail {
    #[default]
    NotUsed,
    Used,
    Flushed,
}

/// Per-instruction cycle-accumulation state, persisted across
/// [`Arm7Tdmi::step`] calls where the reference does (`last_cycle`,
/// `prefetch_cycle`) and reset at instruction boundaries where it does
/// (`stretched_cycles`, `branch_trail`, `merged_is`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct CycleState {
    pub(crate) stretched_cycles: f32,
    pub(crate) last_cycle: Option<CycleType>,
    /// The bus-cycle type the NEXT instruction's opcode prefetch should use;
    /// defaults to `S` but a store instruction sets it to `N` (a store
    /// forces a nonsequential cycle for whatever comes next).
    pub(crate) prefetch_cycle: CycleType,
    pub(crate) branch_trail: BranchTrail,
    pub(crate) merged_is: bool,
}

impl Default for CycleState {
    fn default() -> Self {
        Self {
            stretched_cycles: 0.0,
            last_cycle: None,
            prefetch_cycle: CycleType::S,
            branch_trail: BranchTrail::NotUsed,
            merged_is: false,
        }
    }
}

/// The single flat-region wait-state length this crate assumes (see the
/// module doc comment) — one flash-equivalent wait state, MAM-eligible.
const FLAT_REGION_LENGTH: f32 = 1.0;

impl<M: ThumbMemory> Arm7Tdmi<M> {
    /// Reset the per-instruction accumulator at the start of a
    /// [`Arm7Tdmi::step`] — mirrors the reference's end-of-loop reset,
    /// simply performed at the opposite end since a single `step()` call is
    /// this crate's unit of execution.
    pub(crate) fn cycles_begin_instruction(&mut self) {
        self.cycles.stretched_cycles = 0.0;
        self.cycles.branch_trail = BranchTrail::NotUsed;
        self.cycles.merged_is = false;
    }

    pub(crate) fn cycles_end_instruction(&mut self) -> u32 {
        // Round-half-up rather than truncate: a fractional accumulation of
        // e.g. 1.3 N-cycles should not silently collapse to "1" every time.
        // `f32::round` isn't available in `core` (no_std); `stretched_cycles`
        // is always non-negative, so `+ 0.5` then truncating is equivalent.
        (self.cycles.stretched_cycles + 0.5) as u32
    }

    /// Whether `addr`'s latch line matches the given latch, updating it if
    /// not — ported from Gopher2600's `isLatched`. Returns `true` if the
    /// access can use the fast latched path.
    fn is_latched(&mut self, cycle: CycleType, bus: BusAccess, addr: u32) -> bool {
        let latch = addr & 0xffff_ff80;
        match bus {
            BusAccess::Prefetch => {
                if latch == self.mam.prefetch_latch {
                    return true;
                }
                self.mam.prefetch_latch = latch;
                matches!(cycle, CycleType::S) && !self.mam.prefetch_aborted
            }
            BusAccess::Branch => {
                if latch == self.mam.branch_latch {
                    self.cycles.branch_trail = BranchTrail::Used;
                    return true;
                }
                self.mam.branch_latch = latch;
                self.cycles.branch_trail = BranchTrail::Flushed;
                false
            }
            BusAccess::DataRead => {
                if latch == self.mam.data_latch {
                    return true;
                }
                self.mam.data_latch = latch;
                false
            }
            BusAccess::DataWrite => {
                self.mam.data_latch = 0;
                false
            }
        }
    }

    /// An internal (no bus activity) cycle.
    pub(crate) fn icycle(&mut self) {
        self.cycles.stretched_cycles += 1.0;
        self.cycles.last_cycle = Some(CycleType::I);
        self.mam.prefetch_aborted = false;
    }

    /// A sequential bus cycle, ported from `sCycle_ARM7TDMI`.
    pub(crate) fn scycle(&mut self, bus: BusAccess, addr: u32) {
        self.mam.prefetch_aborted = bus.is_data_access();

        // "Merged I-S cycles": an S cycle immediately after an I cycle can
        // be folded into it on real ARM7TDMI-S hardware.
        if matches!(self.cycles.last_cycle, Some(CycleType::I)) {
            self.cycles.stretched_cycles -= 1.0;
            self.cycles.merged_is = true;
        }
        self.cycles.last_cycle = Some(CycleType::S);

        match self.mam.mode() {
            MamMode::Disabled => self.cycles.stretched_cycles += FLAT_REGION_LENGTH,
            MamMode::Partial => {
                if bus.is_data_access() {
                    self.cycles.stretched_cycles += FLAT_REGION_LENGTH;
                } else if self.is_latched(CycleType::S, bus, addr) {
                    self.cycles.stretched_cycles += 1.0;
                } else {
                    self.cycles.stretched_cycles += FLAT_REGION_LENGTH;
                }
            }
            MamMode::Full => {
                if self.is_latched(CycleType::S, bus, addr) {
                    self.cycles.stretched_cycles += 1.0;
                } else {
                    self.cycles.stretched_cycles += FLAT_REGION_LENGTH;
                }
            }
        }
    }

    /// A nonsequential bus cycle, ported from `nCycle_ARM7TDMI`.
    pub(crate) fn ncycle(&mut self, bus: BusAccess, addr: u32) {
        self.mam.prefetch_aborted = bus.is_data_access();

        let mut mclk_flash = 1.0_f32;
        let mut mclk_non_flash = 1.0_f32;
        if matches!(self.cycles.last_cycle, Some(CycleType::N)) {
            mclk_flash = 1.3;
            mclk_non_flash = 1.8;
        }
        self.cycles.last_cycle = Some(CycleType::N);

        match self.mam.mode() {
            MamMode::Disabled | MamMode::Partial => {
                self.cycles.stretched_cycles += FLAT_REGION_LENGTH * mclk_flash;
            }
            MamMode::Full => {
                if self.is_latched(CycleType::N, bus, addr) {
                    self.cycles.stretched_cycles += mclk_non_flash;
                } else {
                    self.cycles.stretched_cycles += FLAT_REGION_LENGTH * mclk_flash;
                }
            }
        }
    }

    /// A store instruction's cycle: an N-cycle data write, then arrange for
    /// the NEXT instruction's prefetch to also be nonsequential.
    pub(crate) fn store_register_cycles(&mut self, addr: u32) {
        self.ncycle(BusAccess::DataWrite, addr);
        self.cycles.prefetch_cycle = CycleType::N;
    }

    /// Extra cycles to refill the pipeline after a branch — called from
    /// [`Arm7Tdmi::step`] whenever the executed instruction changed `PC`.
    pub(crate) fn fill_pipeline(&mut self) {
        let pc = self.registers[crate::registers::REG_PC];
        self.ncycle(BusAccess::Branch, pc);
        self.scycle(BusAccess::Prefetch, pc.wrapping_add(2));
    }
}
