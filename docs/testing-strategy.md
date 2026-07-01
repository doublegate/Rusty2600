# Testing strategy — Rusty2600

References: `ref-docs/research-report.md` §11 (the test-ROM oracle), §9 (prior
art / Stella as oracle); `docs/adr/0003` (the honesty gate);
`crates/rusty2600-test-harness/`.

**Test ROMs are the spec.** When the docs and a passing test ROM disagree, the
ROM wins — the doc gets updated. For accuracy work, pin the failing ROM
expectation FIRST, then implement until it passes. **Stella is the behavioral
oracle**: when prose docs and Stella disagree about a quirk, prefer Stella plus a
confirming test ROM.

## The layers

- **Layer 1 — unit** (per crate, >90% line coverage on the chip crates).
  Register decode, board hotspots, timer prescale, poly-counter state.
- **Layer 2 — CPU golden-log.** The Klaus Dormann `6502_functional_test` +
  `6502_decimal_test` (Bruce Clark BCD), and the SingleStepTests/ProcessorTests
  `6502` set (per-opcode, per-cycle bus activity, NMOS + decimal active, incl.
  undocumented opcodes). Both Klaus ROMs run through the shared
  `Sentinel`/`run_cpu_until_sentinel` runner (v0.8.0) rather than each
  hand-rolling its own PC-trap loop. `GoldenLogDiffer` captures a real
  `(PC, A, X, Y, SP, P, cycle)` record per retired instruction and diffs it
  against a loaded golden log. **`T-0602-007` closed (v1.8.0):**
  `tests/golden/klaus_functional_test_gopher2600.trace` bundles a genuine
  externally-oracled trace — the first 20,000 retired instructions of the
  Klaus functional test, captured by running Gopher2600's `hardware/cpu`
  package directly (the same technique its own `cpu_test.go` uses to unit-test
  the CPU in isolation) against the identical ROM. `GoldenLogDiffer::bundled()`
  now reports `true`; `crates/rusty2600-test-harness/tests/golden_log_test.rs`
  asserts `first_divergence() == None` — two independently-implemented 6502
  cores agreeing register-for-register and cycle-for-cycle is genuine
  external validation, distinct from Klaus's own internal pass/fail trap
  (which only proves the ROM's self-check passed). Deliberately bounded to
  20,000 instructions, not the full ~30M-instruction run, to keep the
  fixture a reasonable committed size — still a real, meaningful
  confirmation over the test's early control flow. Use the `6502` set,
  **not** `nes6502` (which ignores decimal). Per ref-docs/research-report.md §11.
- **Layer 3 — test-ROM corpus.** TIA timing/draw ROMs (beam-racing, RESPx,
  HMOVE-comb, collisions) verified against Stella + reference scanline buffers,
  plus the Stella regression corpus for mapper/audio/per-quirk behaviour.
  `run_until_complete(system, max_color_clocks)` steps until the suite's result
  protocol fires and asserts the code — **still a stub, and stays one**
  (`T-0602-006`): researched (v1.8.0) whether any freely-redistributable
  2600-specific TIA/RIOT test-ROM corpus exists beyond what's already bundled
  (`tests/roms/test_suite/`, Tom Harte's SingleStepTests) — none found.
  Gopher2600's own README makes the same admission (it has never obtained
  permission to redistribute its TIA/RIOT test ROMs either), and the Atari
  Diagnostic Test Cartridge 2.0 is official service-center software, not
  freely redistributable. This is a real, permanent scope boundary, not a
  gap closed by more effort — TIA/RIOT accuracy work continues via the
  differential-oracle method (`rusty2600-gopher-differential-oracle` project
  memory) against specific known-hard titles, the same technique that found
  the real Frogger WSYNC-jitter and Pitfall II RIOT-timer bugs.
- **Layer 4 — accuracy battery** (the AccuracyCoin-equivalent), real as of
  v0.8.0. `tests/accuracy_battery.rs` runs the bundled Klaus oracles through
  `run_cpu_until_sentinel`, records each into a real `AccuracyScore`, and
  asserts the pass rate against the v1.0 threshold (≥90%, 100% the goal —
  see `docs/STATUS.md` §version policy; currently 2/2, 100%). This test lives
  inside the existing `cargo test --workspace --features test-roms` CI step
  (`.github/workflows/ci.yml`), so a pass-rate regression already fails CI —
  no separate regression-gate workflow was needed.
- **Layer 5 — visual golden + screenshots.** `SnapComparator` diffs the composed
  scanline buffer (the beam-raced TIA has no chip-owned framebuffer) against a
  committed golden in `tests/golden/`, with real tolerance-aware comparison
  (`diff_count_within_tolerance`/`matches_within_tolerance`, v0.8.0) alongside
  the exact byte-diff. Commit only CC0/public-domain ROMs + screenshots;
  commercial dumps live in gitignored `tests/roms/external/`.

## The honesty gate (ADR 0003)

`tests/mapper_tier_honesty.rs` enforces the headline-claim invariant: every board
the accuracy battery covers must report a `Core` or `Curated`
(`is_accuracy_gated`) tier — **never `BestEffort`**. A BestEffort board may carry
reference screenshots but can never inflate the accuracy number. As Curated/
BestEffort boards land, extend the gate's `oracle_boards()` set in lockstep. Per
ref-docs/research-report.md §8.3.

## Determinism in tests

Every harness run seeds the `System` explicitly (`System::new(seed)`); same
seed + ROM + input ⇒ bit-identical scanline buffer + audio (ADR 0004). This is what
makes the golden-log differ and the snap comparator stable.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
