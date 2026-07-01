# Phase 6, Sprint 2 — Pass-Count Pinning + the CI Regression Gate

**Context:** Part of Phase 6 — Accuracy to ~100. Targets v0.8.0 "Battery"
(`to-dos/ROADMAP.md`): stand up `rusty2600-test-harness`'s Layer 4 accuracy
battery for real (previously unused scaffolding — `GoldenLogDiffer`/
`run_until_complete`/`AccuracyScore`/`SnapComparator` existed but nothing
called them; the real Klaus tests hand-rolled their own PC-trap loops
directly against `rusty2600_cpu::Cpu`, bypassing the harness entirely).

## Tickets (`T-0602-NNN`)

- [x] `T-0602-001` (DONE — v0.8.0): `Sentinel` + `run_cpu_until_sentinel` —
  the shared Layer 2 (CPU-only) runner. Two variants cover the bundled
  Klaus oracles' own completion protocols exactly: `PcTrap` (success when
  PC reaches an address AFTER stepping; any other stuck PC is a failure —
  the functional test's convention) and `PcWithZeroPageCheck` (success when
  PC reaches an address BEFORE stepping, gated on a zero-page pass/fail
  byte — the decimal test's convention, since `$DB` sitting at its `DONE`
  label is a real illegal opcode on this CPU type, not a halt). Both
  variants preserve their ROM's exact original check order (verified via
  the refactor below producing byte-identical pass/fail behavior).
- [x] `T-0602-002` (DONE — v0.8.0): Refactored `tests/klaus_test.rs`'s two
  tests (functional + decimal) to call `run_cpu_until_sentinel` instead of
  each hand-rolling its own loop. Same ROMs, same sentinels, same instruction
  budgets — a pure refactor, not a behavior change; both tests still pass.
- [x] `T-0602-003` (DONE — v0.8.0): `tests/accuracy_battery.rs` — the real
  Layer 4 battery. Runs both bundled Klaus oracles through
  `run_cpu_until_sentinel`, records each into a real `AccuracyScore`, and
  asserts the aggregate pass rate meets the v1.0 threshold (`docs/
  STATUS.md`'s "Version policy": ≥90%, 100% the goal) — currently 2/2
  (100%). Gated behind `test-roms`, matching `klaus_test.rs`.
- [x] `T-0602-004` (DONE — v0.8.0, no new CI YAML needed): the CI regression
  gate. `.github/workflows/ci.yml` already runs `cargo test --workspace
  --features test-roms` on every push (unchanged); since
  `accuracy_battery_meets_v1_0_threshold` now lives inside that same
  feature-gated test run and asserts the threshold, any pass-rate
  regression already fails that existing CI step — no new workflow file
  was needed to satisfy this ticket.
- [x] `T-0602-005` (DONE — v0.8.0): `SnapComparator` gains real
  tolerance-aware comparison (`diff_count_within_tolerance`/
  `matches_within_tolerance`, both `abs_diff`-based per-byte thresholds),
  not just the prior exact-byte-diff stub. Still no bundled TIA-timing
  golden screenshots to compare against yet — see `T-0602-006`.
- [ ] `T-0602-006` (not started, honestly deferred): populate real
  TIA-timing/draw-ROM test fixtures and golden captures so
  `run_until_complete` (Layer 3, full-`System`) and the tolerance-aware
  `SnapComparator` have something concrete to run against. No such
  fixtures are bundled today, so `run_until_complete` remains a documented
  stub rather than a guessed-at protocol.
- [ ] `T-0602-007` (not started, honestly deferred): a genuine per-instruction
  golden CPU trace log for `GoldenLogDiffer`, produced by an independent,
  externally-trusted oracle (an instrumented Stella or Gopher2600 build, per
  the project's own differential-oracle workflow — see the
  `rusty2600-gopher-differential-oracle` project memory). `GoldenLogDiffer`
  itself is now real (a genuine `Vec<TraceRecord>` capture buffer + a real
  diff algorithm), but `bundled()` honestly reports `false` until this
  ticket lands a real reference trace — Klaus's own internal self-check
  (via `Sentinel`) remains the authoritative CPU oracle in the meantime.
- [ ] `T-0602-008` (not started): fold the `SingleStepTests` cycle-exact
  audit's own 233/233-opcode result into the SAME `AccuracyScore` the
  battery uses, rather than leaving it as a separately-enforced gate in
  `rusty2600-cpu`'s own test suite. Needs a way for that audit to report a
  summary this crate can consume across the crate boundary.

## Technical notes

`Sentinel`'s two variants are deliberately narrow (exactly what the two
bundled Klaus ROMs need) rather than a speculative general-purpose protocol
— extend with a new variant only when a THIRD CPU-only oracle ROM with a
genuinely different completion convention is bundled, per the project's
own "don't design for hypothetical future requirements" convention.

`accuracy_battery.rs` deliberately re-runs the same two Klaus ROMs
`klaus_test.rs` already runs, rather than trying to share a single run
across two separate `#[test]` functions (which `cargo test`'s parallel,
isolated-process-per-binary execution model makes awkward to do safely). The
modest CI-time duplication (the decimal sweep is ~700M instructions either
way) buys a genuine end-to-end pass-rate assertion independent of the
per-suite tests, which is the actual "Layer 4 battery" the testing strategy
calls for — not a re-derivation of the Layer 2 protocol itself, which stays
defined once in `Sentinel`.

## Verification

- `cargo test -p rusty2600-test-harness` — 11 lib unit tests (up from 3):
  `Sentinel`/`run_cpu_until_sentinel` coverage (pass, fail, timeout, both
  variants), `GoldenLogDiffer` (bundled-or-not, divergence-finding,
  identical-trace matching), and `SnapComparator` tolerance behavior.
- `cargo test -p rusty2600-test-harness --features test-roms` — adds the
  refactored `klaus_functional_test`/`klaus_decimal_test` (unchanged
  pass/fail behavior, confirmed via the refactor) plus the new
  `accuracy_battery_meets_v1_0_threshold` (2/2, 100%).
- `cargo clippy -p rusty2600-test-harness --all-targets --features
  test-roms,commercial-roms -- -D warnings` — clean (the feature-gated test
  files needed the same large-stack-array / items-after-statements /
  doc-markdown fixes the rest of the workspace already carries, since they
  lack `klaus_test.rs`'s blanket `#![allow(warnings)]`).
- Full CI matrix green.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
