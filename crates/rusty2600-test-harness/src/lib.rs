//! `rusty2600-test-harness` — the `AccuracyCoin`-equivalent accuracy gate.
//!
//! Mirrors the `RustyNES` harness SHAPE (don't reinvent):
//!
//! 1. [`GoldenLogDiffer`] — a Klaus-6502 golden-log differ: capture a
//!    `(PC, A, X, Y, SP, P, cycle)` trace record per retired instruction and
//!    diff it against a bundled golden log (the Klaus functional-test / 6502
//!    golden trace is the closed-form 6502 spec).
//! 2. [`Sentinel`] + [`run_cpu_until_sentinel`] — the Layer 2 (CPU-only)
//!    runner: steps a [`rusty2600_cpu::Cpu`] against any [`CpuBus`] until the
//!    suite's completion protocol fires. Both bundled Klaus oracles
//!    (`tests/klaus_test.rs`) run through this shared function now, rather
//!    than each hand-rolling its own PC-trap loop.
//! 3. [`run_until_complete`] — the Layer 3 (full-system) runner: steps a
//!    [`System`] until a TIA-timing test ROM's completion sentinel fires.
//!    Still a stub — no TIA-timing test-ROM fixtures are bundled yet (see
//!    its own doc comment).
//! 4. [`AccuracyScore`] — an accuracy-battery scorer (the `AccuracyCoin`
//!    pass-rate equivalent). `tests/accuracy_battery.rs` wires it to the
//!    bundled Klaus oracles and asserts the v1.0 pass-rate threshold.
//! 5. [`SnapComparator`] — a tolerance-aware `.snap` / screenshot comparator.
//!    Because the TIA is beam-raced, the captured artifact is the composed
//!    scanline buffer, not a chip-owned framebuffer.
//!
//! The `mapper_tier_honesty` integration test (`tests/`) enforces that no
//! `BestEffort` bankswitch board ever backs the accuracy oracle. See
//! `docs/testing-strategy.md`.

use rusty2600_core::System;
use rusty2600_cpu::{Cpu, CpuBus};

/// One captured CPU trace record, compared field-for-field against the golden
/// log. The 2600 has no IRQ/NMI wiring, so the differ only needs the core
/// register file + the cycle count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRecord {
    /// Program counter at instruction fetch.
    pub pc: u16,
    /// Accumulator.
    pub a: u8,
    /// Index register X.
    pub x: u8,
    /// Index register Y.
    pub y: u8,
    /// Stack pointer.
    pub sp: u8,
    /// Processor status `P`.
    pub p: u8,
    /// Total CPU cycles elapsed when this instruction retired.
    pub cycle: u64,
}

/// Klaus-6502 golden-log differ: accumulates the live trace and reports the
/// first record that diverges from a bundled golden log.
///
/// **Honesty note (`T-0601-002`):** the golden log itself — a genuine
/// per-instruction `(PC, A, X, Y, SP, P, cycle)` trace produced by an
/// independent, externally-trusted oracle (e.g. an instrumented Stella or
/// Gopher2600 build, per the project's own differential-oracle workflow) —
/// isn't bundled yet. `capture` and the trace buffer are real; `bundled()`
/// honestly reports `false` until a real golden log lands, so
/// `first_divergence` can't yet claim a false "no divergence" from an empty
/// reference. Klaus's OWN internal self-check (the functional/decimal
/// tests' own pass/fail trap, see [`Sentinel`]/[`run_cpu_until_sentinel`])
/// remains the real oracle in the meantime — it's independently
/// authoritative (it IS the closed-form 6502 spec, not just a captured
/// trace of one run), just coarser-grained than a per-instruction diff.
#[derive(Debug, Default)]
pub struct GoldenLogDiffer {
    trace: Vec<TraceRecord>,
    golden: Option<Vec<TraceRecord>>,
}

impl GoldenLogDiffer {
    /// Construct an empty differ.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one live trace record (called after each retired instruction).
    pub fn capture(&mut self, record: TraceRecord) {
        self.trace.push(record);
    }

    /// Load a golden log to diff future captures against.
    pub fn load_golden(&mut self, golden: Vec<TraceRecord>) {
        self.golden = Some(golden);
    }

    /// Whether a golden log has been loaded (see the honesty note above —
    /// `false` until `T-0601-002` bundles a real one).
    #[must_use]
    pub const fn bundled(&self) -> bool {
        self.golden.is_some()
    }

    /// Diff the captured trace against the loaded golden log. Returns the
    /// index of the first divergence, or `None` if the run matches (or if
    /// no golden log is loaded — see [`Self::bundled`]).
    #[must_use]
    pub fn first_divergence(&self) -> Option<usize> {
        let golden = self.golden.as_ref()?;
        self.trace
            .iter()
            .zip(golden.iter())
            .position(|(a, b)| a != b)
            .or_else(|| {
                (self.trace.len() != golden.len()).then_some(self.trace.len().min(golden.len()))
            })
    }
}

/// The result of a test-ROM run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The ROM signalled success via its result protocol.
    Passed,
    /// The ROM signalled a numbered failure.
    Failed(u8),
    /// The run hit the step budget without reaching the completion sentinel.
    TimedOut,
}

/// A CPU-only (Layer 2) test ROM's completion-detection protocol.
///
/// Different suites signal success differently; this covers the two
/// conventions the bundled Klaus oracles use — extend with a new variant as
/// a new CPU-only suite is bundled, rather than special-casing it outside
/// this type.
#[derive(Debug, Clone, Copy)]
pub enum Sentinel {
    /// Success once PC reaches `success_pc` (checked AFTER stepping, so the
    /// instruction that lands on it has already retired). Any OTHER
    /// address where PC fails to advance (the ROM's own infinite-loop trap)
    /// is a failure. Matches the Klaus functional test's convention.
    PcTrap {
        /// The address PC must reach for the run to count as a pass.
        success_pc: u16,
    },
    /// Success once PC reaches `success_pc` (checked BEFORE stepping, so the
    /// instruction sitting there — often not meant to execute — never
    /// runs), AND a zero-page byte holds an expected pass code. Matches the
    /// Klaus decimal test's convention (`ERROR` at zero-page `$0B`).
    PcWithZeroPageCheck {
        /// The address PC must reach for the run to be considered complete.
        success_pc: u16,
        /// The zero-page address holding the suite's own pass/fail code.
        zp_addr: u16,
        /// The value at `zp_addr` that means "passed".
        pass_value: u8,
    },
}

/// Step `cpu` against `bus` until `sentinel` fires or `max_instructions` is
/// exhausted, returning the outcome.
///
/// Shared by every CPU-only (Layer 2) oracle — the bundled Klaus
/// functional/decimal tests today, any future bundled CPU-only test ROM
/// without needing its own hand-rolled loop.
#[must_use]
pub fn run_cpu_until_sentinel<B: CpuBus>(
    cpu: &mut Cpu,
    bus: &mut B,
    sentinel: Sentinel,
    max_instructions: u64,
) -> RunOutcome {
    let mut instructions = 0u64;
    while instructions < max_instructions {
        match sentinel {
            Sentinel::PcTrap { success_pc } => {
                let prev_pc = cpu.pc;
                cpu.step(bus);
                if cpu.pc == success_pc {
                    return RunOutcome::Passed;
                } else if cpu.pc == prev_pc {
                    return RunOutcome::Failed(0);
                }
            }
            Sentinel::PcWithZeroPageCheck {
                success_pc,
                zp_addr,
                pass_value,
            } => {
                if cpu.pc == success_pc {
                    let code = bus.read(zp_addr);
                    return if code == pass_value {
                        RunOutcome::Passed
                    } else {
                        RunOutcome::Failed(code)
                    };
                }
                cpu.step(bus);
            }
        }
        instructions += 1;
    }
    RunOutcome::TimedOut
}

/// Step a [`System`] until the suite's completion sentinel, then report the
/// outcome.
///
/// **Honesty note (`T-0601-003`):** this is Layer 3 (full-system TIA-timing
/// test ROMs) per `docs/testing-strategy.md` — genuinely still a stub, since
/// no TIA-timing test-ROM fixtures are bundled yet to define what the
/// per-suite result protocol even looks like. The CPU-only (Layer 2)
/// counterpart, [`run_cpu_until_sentinel`], is real and in active use (see
/// `tests/klaus_test.rs`); this one still needs test-ROM fixtures before a
/// real protocol can be wired without guessing at one.
#[must_use]
pub fn run_until_complete(system: &mut System, max_color_clocks: u64) -> RunOutcome {
    let mut clocks = 0u64;
    while clocks < max_color_clocks {
        system.tick_one_color_clock();
        clocks += 1;
    }
    RunOutcome::TimedOut
}

/// Accuracy-battery scorer (the `AccuracyCoin` pass-rate equivalent).
///
/// Tracks passed / total across the oracle corpus.
#[derive(Debug, Default, Clone, Copy)]
pub struct AccuracyScore {
    /// Number of oracle ROMs that passed.
    pub passed: u32,
    /// Total oracle ROMs run.
    pub total: u32,
}

impl AccuracyScore {
    /// Record one ROM result.
    // Not `const`: trivially const today, but kept non-const for symmetry with
    // the other recorders that will gather per-ROM detail.
    #[allow(clippy::missing_const_for_fn)]
    pub fn record(&mut self, passed: bool) {
        self.total += 1;
        if passed {
            self.passed += 1;
        }
    }

    /// Pass-rate in `[0.0, 1.0]` (0.0 when nothing has been run).
    #[must_use]
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            f64::from(self.passed) / f64::from(self.total)
        }
    }
}

/// `.snap` / screenshot comparator. The beam-raced TIA produces a composed
/// scanline buffer per frame; this compares it against a committed golden
/// capture (`tests/golden/`), never a commercial ROM frame.
#[derive(Debug, Default)]
pub struct SnapComparator;

impl SnapComparator {
    /// Compare a freshly captured frame buffer against a golden one. Returns
    /// the number of BYTE-exact differing positions (0 ⇒ byte-identical).
    /// Length mismatch counts every extra/missing byte as differing.
    #[must_use]
    pub fn diff_pixels(&self, captured: &[u8], golden: &[u8]) -> usize {
        if captured.len() != golden.len() {
            return captured.len().max(golden.len());
        }
        captured
            .iter()
            .zip(golden.iter())
            .filter(|(c, g)| c != g)
            .count()
    }

    /// Tolerance-aware comparison: counts positions whose absolute
    /// difference exceeds `tolerance` (0 ⇒ identical behavior to
    /// [`Self::diff_pixels`]). For a case with legitimately
    /// bounded-but-nondeterministic output — e.g. seeded power-on RAM
    /// (`docs/adr/0006`) touching a handful of border pixels differently
    /// per seed — a small nonzero tolerance distinguishes "a few
    /// off-by-a-shade pixels" from "the frame is actually wrong." A
    /// length mismatch is still an unconditional total mismatch (no
    /// tolerance can paper over a differently-sized capture).
    #[must_use]
    pub fn diff_count_within_tolerance(
        &self,
        captured: &[u8],
        golden: &[u8],
        tolerance: u8,
    ) -> usize {
        if captured.len() != golden.len() {
            return captured.len().max(golden.len());
        }
        captured
            .iter()
            .zip(golden.iter())
            .filter(|&(&c, &g)| c.abs_diff(g) > tolerance)
            .count()
    }

    /// `true` if every byte is within `tolerance` of its golden counterpart
    /// (and the lengths match).
    #[must_use]
    pub fn matches_within_tolerance(&self, captured: &[u8], golden: &[u8], tolerance: u8) -> bool {
        captured.len() == golden.len()
            && self.diff_count_within_tolerance(captured, golden, tolerance) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_until_complete_times_out_on_stub() {
        let mut sys = System::new(0);
        assert_eq!(run_until_complete(&mut sys, 16), RunOutcome::TimedOut);
    }

    #[test]
    fn score_pass_rate() {
        let mut score = AccuracyScore::default();
        score.record(true);
        score.record(false);
        assert!((score.pass_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn snap_identical_is_zero() {
        let cmp = SnapComparator;
        assert_eq!(cmp.diff_pixels(&[1, 2, 3], &[1, 2, 3]), 0);
        assert_eq!(cmp.diff_pixels(&[1, 2, 3], &[1, 9, 3]), 1);
    }

    #[test]
    fn snap_tolerance_absorbs_small_deltas_but_not_large_ones() {
        let cmp = SnapComparator;
        // Within tolerance (delta 1 <= tolerance 2): no diff reported.
        assert_eq!(cmp.diff_count_within_tolerance(&[10, 20], &[11, 19], 2), 0);
        assert!(cmp.matches_within_tolerance(&[10, 20], &[11, 19], 2));
        // Exceeds tolerance (delta 9 > tolerance 2): reported.
        assert_eq!(cmp.diff_count_within_tolerance(&[10, 20], &[19, 20], 2), 1);
        assert!(!cmp.matches_within_tolerance(&[10, 20], &[19, 20], 2));
        // Length mismatch is never absorbed by tolerance.
        assert_eq!(cmp.diff_count_within_tolerance(&[1, 2, 3], &[1, 2], 255), 3);
    }

    struct FlatRamBus {
        ram: Vec<u8>,
    }

    impl FlatRamBus {
        // Heap-allocated (not a stack array): a 64 KiB flat address space is
        // the simplest possible `CpuBus` for these unit tests, but 64 KiB on
        // the stack trips clippy's `large_stack_arrays`.
        fn filled(byte: u8) -> Self {
            Self {
                ram: vec![byte; 0x10000],
            }
        }
    }

    impl CpuBus for FlatRamBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.ram[usize::from(addr)]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.ram[usize::from(addr)] = val;
        }
    }

    #[test]
    fn pc_trap_sentinel_passes_on_success_address() {
        let mut bus = FlatRamBus::filled(0xEA); // NOP forever
        bus.ram[0x1000] = 0x4C; // JMP $1000 (infinite loop -- would-be failure trap)
        bus.ram[0x1001] = 0x00;
        bus.ram[0x1002] = 0x10;
        let mut cpu = Cpu::power_on();
        cpu.set_pc(0x1000);
        // Success PC is $1000 itself: the very first step (JMP -> $1000)
        // lands back on $1000, which the sentinel checks AFTER stepping.
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcTrap { success_pc: 0x1000 },
            100,
        );
        assert_eq!(outcome, RunOutcome::Passed);
    }

    #[test]
    fn pc_trap_sentinel_fails_on_unexpected_infinite_loop() {
        let mut bus = FlatRamBus::filled(0xEA);
        bus.ram[0x1000] = 0x4C; // JMP $1000
        bus.ram[0x1001] = 0x00;
        bus.ram[0x1002] = 0x10;
        let mut cpu = Cpu::power_on();
        cpu.set_pc(0x1000);
        // A DIFFERENT success address than the one the ROM actually loops
        // at -- the loop is a failure trap, not the documented success one.
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcTrap { success_pc: 0x2000 },
            100,
        );
        assert_eq!(outcome, RunOutcome::Failed(0));
    }

    #[test]
    fn pc_with_zp_check_sentinel_reports_pass_and_fail_codes() {
        let mut bus = FlatRamBus::filled(0xEA);
        bus.ram[0x000B] = 0x00; // ERROR = 0 (pass)
        let mut cpu = Cpu::power_on();
        cpu.set_pc(0x0200);
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcWithZeroPageCheck {
                success_pc: 0x0200,
                zp_addr: 0x000B,
                pass_value: 0,
            },
            100,
        );
        assert_eq!(outcome, RunOutcome::Passed);

        let mut bus_fail = FlatRamBus::filled(0xEA);
        bus_fail.ram[0x000B] = 0x01; // ERROR = 1 (fail)
        let mut cpu_fail = Cpu::power_on();
        cpu_fail.set_pc(0x0200);
        let outcome_fail = run_cpu_until_sentinel(
            &mut cpu_fail,
            &mut bus_fail,
            Sentinel::PcWithZeroPageCheck {
                success_pc: 0x0200,
                zp_addr: 0x000B,
                pass_value: 0,
            },
            100,
        );
        assert_eq!(outcome_fail, RunOutcome::Failed(0x01));
    }

    #[test]
    fn sentinel_times_out_when_never_reached() {
        let mut bus = FlatRamBus::filled(0xEA); // NOP forever, PC just marches on
        let mut cpu = Cpu::power_on();
        cpu.set_pc(0x0000);
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcTrap { success_pc: 0xFFFF },
            8,
        );
        assert_eq!(outcome, RunOutcome::TimedOut);
    }

    #[test]
    fn golden_log_differ_reports_no_bundle_by_default() {
        let mut differ = GoldenLogDiffer::new();
        assert!(!differ.bundled());
        differ.capture(TraceRecord {
            pc: 0,
            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            p: 0,
            cycle: 0,
        });
        // No golden loaded -- honestly reports no divergence found (not "matches").
        assert_eq!(differ.first_divergence(), None);
    }

    #[test]
    fn golden_log_differ_finds_first_divergence() {
        let mut differ = GoldenLogDiffer::new();
        let rec = |pc| TraceRecord {
            pc,
            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            p: 0,
            cycle: 0,
        };
        differ.load_golden(vec![rec(1), rec(2), rec(3)]);
        assert!(differ.bundled());
        differ.capture(rec(1));
        differ.capture(rec(2));
        differ.capture(rec(99)); // diverges here
        assert_eq!(differ.first_divergence(), Some(2));
    }

    #[test]
    fn golden_log_differ_matches_identical_trace() {
        let mut differ = GoldenLogDiffer::new();
        let rec = |pc| TraceRecord {
            pc,
            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            p: 0,
            cycle: 0,
        };
        differ.load_golden(vec![rec(1), rec(2)]);
        differ.capture(rec(1));
        differ.capture(rec(2));
        assert_eq!(differ.first_divergence(), None);
    }
}
