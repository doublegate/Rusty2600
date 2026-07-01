//! `rusty2600-test-harness` — the `AccuracyCoin`-equivalent accuracy gate.
//!
//! Mirrors the `RustyNES` harness SHAPE (don't reinvent):
//!
//! 1. [`GoldenLogDiffer`] — a Klaus-6502 golden-log differ: capture a
//!    `(PC, A, X, Y, SP, P, cycle)` trace record per retired instruction and
//!    diff it against a bundled golden log (the Klaus functional-test / 6502
//!    golden trace is the closed-form 6502 spec).
//! 2. [`run_until_complete`] — a test-ROM runner that steps a [`System`] until
//!    the suite's completion sentinel and returns the result code.
//! 3. [`AccuracyScore`] — an accuracy-battery scorer (the `AccuracyCoin`
//!    pass-rate equivalent).
//! 4. [`SnapComparator`] — a `.snap` / screenshot comparator stub. Because the
//!    TIA is beam-raced, the captured artifact is the composed scanline buffer,
//!    not a chip-owned framebuffer.
//!
//! The `mapper_tier_honesty` integration test (`tests/`) enforces that no
//! `BestEffort` bankswitch board ever backs the accuracy oracle. See
//! `docs/testing-strategy.md`.

use rusty2600_core::System;

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
/// first record that diverges from the bundled golden log.
#[derive(Debug, Default)]
pub struct GoldenLogDiffer {
    captured: usize,
}

impl GoldenLogDiffer {
    /// Construct an empty differ.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one live trace record (called after each retired instruction).
    // Not `const`: the real differ pushes into a heap trace buffer.
    #[allow(clippy::missing_const_for_fn)]
    pub fn capture(&mut self, _record: TraceRecord) {
        // TODO(T-0601-001): push into the live trace buffer.
        self.captured += 1;
    }

    /// Diff the captured trace against the bundled golden log. Returns the index
    /// of the first divergence, or `None` if the run matches.
    // Not `const`: the real differ walks the captured + golden buffers.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn first_divergence(&self) -> Option<usize> {
        // TODO(T-0601-002): bundle the Klaus functional-test golden trace and diff
        // it record-for-record. Stub: no golden log loaded yet ⇒ no divergence.
        None
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

/// Step a [`System`] until the suite's completion sentinel, then report the
/// outcome.
///
/// Many 6502 test ROMs signal completion by writing a result code to a known
/// address; the per-suite protocol is wired in later.
#[must_use]
pub fn run_until_complete(system: &mut System, max_color_clocks: u64) -> RunOutcome {
    let mut clocks = 0u64;
    while clocks < max_color_clocks {
        system.tick_one_color_clock();
        clocks += 1;
        // TODO(T-0601-003): poll the suite's result protocol (e.g. a sentinel
        // address) and return `Passed` / `Failed(code)` when it fires.
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
    /// Compare a freshly captured frame buffer against a golden one. Returns the
    /// number of differing pixels (0 ⇒ byte-identical). Stub.
    #[must_use]
    pub fn diff_pixels(&self, captured: &[u8], golden: &[u8]) -> usize {
        // TODO(T-0601-004): tolerance-aware comparison + a `.snap` writer for the
        // bless flow. Stub: exact byte diff, length-mismatch ⇒ all differ.
        if captured.len() != golden.len() {
            return captured.len().max(golden.len());
        }
        captured
            .iter()
            .zip(golden.iter())
            .filter(|(c, g)| c != g)
            .count()
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
}
