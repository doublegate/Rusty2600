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
  against a loaded golden log — the capture/diff machinery is real, but no
  externally-oracled golden log is bundled yet (`bundled()` honestly
  reports `false` until one is; Klaus's own internal pass/fail trap remains
  the authoritative CPU oracle meanwhile). Use the `6502` set, **not**
  `nes6502` (which ignores decimal). Per ref-docs/research-report.md §11.
- **Layer 3 — test-ROM corpus.** TIA timing/draw ROMs (beam-racing, RESPx,
  HMOVE-comb, collisions) verified against Stella + reference scanline buffers,
  plus the Stella regression corpus for mapper/audio/per-quirk behaviour.
  `run_until_complete(system, max_color_clocks)` steps until the suite's result
  protocol fires and asserts the code — still a stub; no TIA-timing test-ROM
  fixtures are bundled yet to define the protocol against (v0.8.0 honesty
  note, `T-0602-006`).
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
