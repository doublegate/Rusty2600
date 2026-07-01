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
- [x] `T-0601-006`: Strip the NES-lineage IRQ/NMI vestige. Done (v0.2.0):
  found the crate actually carried a SECOND, entirely dead, never-compiled
  RustyNES CPU implementation (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`,
  ~3,560 lines, no `mod` declarations wired any of it in) — deleted
  outright. The one live file (`lib.rs`) had only leftover NES-flavored
  comment prose (no actual dead code/fields) attached to correct, needed
  universal 6502 behavior; fixed 4 comment blocks to describe the
  2600-relevant case instead. Also split `lib.rs` into `status.rs`/`bus.rs`/
  `cpu.rs` + a thin `lib.rs`, matching RustyNES's own live file layout.
  Re-ran the full SingleStepTests audit + Klaus functional/decimal suites
  after: zero regression (`docs/cpu.md`).
- [ ] `T-0601-007` (unscheduled — deliberately deferred, see `docs/tia.md`
  §Collisions): extend the TIA's object-position / pixel-coordinate model
  from the current 0..159 visible-window space to the full 0..227 raw
  color-clock range so collisions occurring during HBLANK are detected (real
  hardware runs position counters continuously; only the video DAC output is
  blanked). Requires a Gopher2600/Stella differential-oracle probe to
  confirm exact expected behavior first, and must not regress the
  already-verified RESPx/HMOVE visible-window positioning tests.
- [ ] `T-0601-008` (found during `T-0401-005` DPC bring-up, v0.3.0):
  Pitfall II boots without error (DPC decode confirmed correct — a
  Gopher2600 differential probe showed byte-identical PC control-flow
  through the first ~2,000 executed distinct instructions), but the CPU
  spends far longer than Gopher2600 in a boot-time RIOT-timer wait loop at
  `$F108-$F112` (`LDA $1006` DPC music read / `STA $19` / `LDA $0284`
  INTIM / `BNE $F108`) before reaching steady gameplay — Gopher2600 exits
  this loop by roughly its 175,679th distinct-PC transition; Rusty2600 had
  not exited it after 800,000. Since the loop's exit condition depends only
  on `INTIM` reaching zero (not on the DPC read at `$1006`, which is stored
  but not tested), and control flow up to entering the loop is confirmed
  identical, the likely cause is a DATA-value divergence (not control-flow)
  in whatever earlier read seeded the timer's reload value — needs a
  memory-access-tracing probe (not just PC-tracing) to find where `TIM8T`/
  `TIM64T`/`TIM1024T` gets written and compare the written value against
  Gopher2600's. See the `rusty2600-gopher-differential-oracle` project
  memory for the probe methodology. Does not block `T-0401-005`'s own
  scope (register-level DPC model + hotspot bankswitching + unit tests),
  which is complete and independently verified.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
