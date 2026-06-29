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
  undocumented opcodes). The `GoldenLogDiffer` captures a `(PC, A, X, Y, SP, P,
  cycle)` record per retired instruction; the first divergence fails. Use the
  `6502` set, **not** `nes6502` (which ignores decimal). Per
  ref-docs/research-report.md §11.
- **Layer 3 — test-ROM corpus.** TIA timing/draw ROMs (beam-racing, RESPx,
  HMOVE-comb, collisions) verified against Stella + reference scanline buffers,
  plus the Stella regression corpus for mapper/audio/per-quirk behaviour.
  `run_until_complete(system, max_color_clocks)` steps until the suite's result
  protocol fires and asserts the code.
- **Layer 4 — accuracy battery** (the AccuracyCoin-equivalent). `AccuracyScore`
  tracks passed/total across the **Core/Curated-gated** oracle corpus; pass-rate
  gate (≥90% by v1.0, 100% the goal — see `docs/STATUS.md` §version policy).
- **Layer 5 — visual golden + screenshots.** `SnapComparator` diffs the composed
  scanline buffer (the beam-raced TIA has no chip-owned framebuffer) against a
  committed golden in `tests/golden/`. Commit only CC0/public-domain ROMs +
  screenshots; commercial dumps live in gitignored `tests/roms/external/`.

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
