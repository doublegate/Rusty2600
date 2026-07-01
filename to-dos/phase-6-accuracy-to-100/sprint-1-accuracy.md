# Sprint 6.1 — Accuracy Battery Drive

**Context:** Part of Phase 6 — Accuracy to ~100.

## Tickets (`T-0601-NNN`)

- [ ] `T-0601-001`: `GoldenLogDiffer::capture` pushes each retired-instruction
  `TraceRecord` into a real live trace buffer (currently a stub that only
  increments a counter). See `crates/rusty2600-test-harness/src/lib.rs`.
- [ ] `T-0601-002`: Bundle the Klaus functional-test golden trace and wire
  `GoldenLogDiffer::first_divergence` to diff it record-for-record against the
  captured trace (currently always returns `None`, no golden log loaded).
- [ ] `T-0601-003`: `run_until_complete` polls the suite's result protocol
  (e.g. a sentinel address) and returns `Passed` / `Failed(code)` when it
  fires, instead of always running to the step budget and returning
  `TimedOut`.
- [ ] `T-0601-004`: `SnapComparator::diff_pixels` gains a tolerance-aware
  comparison mode and a `.snap` writer for the bless flow (currently an exact
  byte diff only). Backs the Phase 2 scanline-buffer golden harness
  (`T-0201-005`).
- [x] `T-0601-005`: Verify the RIOT `INSTAT` underflow flag and the
  post-underflow 1-cycle mode against the DirtyHairy/Stella reference model
  (`docs/riot.md`). Done (v0.2.0): explicit read-after-write-minus-one test
  across all four prescales, plus confirmed the underflow-cycle read/write
  question resolves structurally from the scheduler's existing tick-then-
  access ordering (no RIOT-specific fix needed).
- [ ] `T-0601-006`: Strip the NES-lineage IRQ/NMI service-sequence +
  `irq_level`/`nmi_level`/`poll_nmi`/`poll_irq` surface from
  `crates/rusty2600-cpu/src/{cpu,lib,bus}.rs` (mapper-IRQ, APU frame-counter/
  DMC IRQ, PPU-driven NMI, branch-delays-IRQ microcode — all inherited from
  the RustyNES port, confirmed dead since `rusty2600-core` never overrides
  any of these trait methods). Keep only behavior that's genuinely universal
  6502/6507 timing independent of interrupts (the taken-branch C3 dummy-PC
  read). Re-run the full SingleStepTests audit + Klaus functional/decimal
  suites after to confirm no regression (`docs/cpu.md`).
- [ ] `T-0601-007` (unscheduled — deliberately deferred, see `docs/tia.md`
  §Collisions): extend the TIA's object-position / pixel-coordinate model
  from the current 0..159 visible-window space to the full 0..227 raw
  color-clock range so collisions occurring during HBLANK are detected (real
  hardware runs position counters continuously; only the video DAC output is
  blanked). Requires a Gopher2600/Stella differential-oracle probe to
  confirm exact expected behavior first, and must not regress the
  already-verified RESPx/HMOVE visible-window positioning tests.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
