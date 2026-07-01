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
- [x] `T-0601-008` (found during `T-0401-005` DPC bring-up, v0.3.0; FIXED v0.9.0):
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
  which is complete and independently verified. **2026-07-01 update:**
  re-tried the screenshot at 60/300/900/5000 `dump_frame` frames — always
  the same blank result. Root cause: `EmuCore::run_frame`
  (`crates/rusty2600-frontend/src/emu_thread.rs`) has a 200,000-instruction
  safety timeout that fires unconditionally while stuck in this loop (no
  real VSYNC to break on), so every `run_frame()` call just burns exactly
  200,000 instructions and returns — by 5000 "frames" the emulator has run
  on the order of 1 BILLION instructions total, still in the same loop.
  This makes "INTIM genuinely never reaches zero" (a real bug) more likely
  than "just slower but still bounded" — re-scope the fix investigation
  accordingly when this is picked up.
  **2026-07-01 RESOLVED (v0.9.0):** rebuilt the Go probe (memory-access
  tracing, not just PC) plus a Rust harness example; found the timer WAS
  passing through zero periodically once it entered its post-underflow
  divide-by-1 fast mode (confirmed via per-instruction INTIM sampling), but
  Rusty2600's `post_underflow` flag was never cleared by a later `INTIM`
  read (only by a fresh `TIMxT` write) — so it stayed in fast mode forever
  after the first underflow, and the loop's 13-cycle poll rhythm happened
  to never phase-align with the fast mode's 256-cycle sawtooth landing
  exactly on `$00`. Confirmed against Stella's `M6532::peek`/
  `updateEmulation` (the authoritative behavioral oracle): reading `INTIM`
  reverts the decrement rate to the normal prescale unless the underflow
  fired on that exact same cycle. Fixed in `rusty2600-riot`
  (`docs/riot.md` has the full writeup); confirmed via the rebuilt probe
  that Rusty2600 now leaves the `$F108` loop at instruction ~22,864 and
  reaches varied gameplay code, matching Gopher2600's own behavior.
  `screenshots/commercial/Pitfall II - Lost Caverns (USA).png` regenerated
  — no longer a blank blue frame.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
