# Phase 6 — Accuracy to ~100

**Goal:** drive the accuracy battery (the AccuracyCoin-equivalent oracle in
`rusty2600-test-harness`) to its target pass rate, closing the off-by-one TIA
timing and CPU edge cases the chips miss after Phases 1-4, and explicitly
deferring any hard residual to a documented carryover (the 2600 has no fractional
timebase need — ADR 0002 is likely never invoked). This is the gate that earns
the v1.0.0 production cut.

References: `docs/testing-strategy.md`; `docs/STATUS.md` (the authoritative
per-suite pass matrix); `docs/adr/0001` (lockstep) / `0004` (determinism);
`crates/rusty2600-test-harness/`; `ref-docs/research-report.md` §11.

## Scope

In: running the full test-ROM corpus (the redistributable / CC0 corpus committed
under `tests/roms/`) plus the local commercial oracle behind `commercial-roms`;
closing the residual TIA beam-timing cases (RESPx 9-CLK pipeline, HMOVE comb
late/early ratchet, the VDELxx delay, collision-latch timing), the CPU
decimal-mode + unstable-undocumented-opcode tails, and the RIOT timer
prescale/INSTAT edges; pinning the per-suite pass counts in `STATUS.md` and
wiring a regression gate so a pass-count drop fails CI.

Out: breadth (BestEffort boards / region data) — Phase 7; reach (netplay / RA /
TAS) — Phase 8. New chip *features* are out — this phase is bug-closure only.

## Exit criteria (verifiable)

- The accuracy battery reaches its target pass rate (≥90% with the 100% goal);
  every passing suite is recorded in `docs/STATUS.md` with an exact `N/M`.
- `nestest`-equivalent CPU golden-log diff stays 0 (Phase 1 not regressed).
- Every deferred hard residual is enumerated in `STATUS.md` with a reason and a
  pointer (ADR 0002 only if a fractional-clock case genuinely warrants it).
- A CI regression gate fails on any pass-count decrease.
- The honesty gate (ADR 0003) still holds — no BestEffort board backs the oracle.

## Sprints

- Sprint 1 — TIA / CPU / RIOT residual closure → `sprint-1-accuracy.md`
  (`T-0601-NNN`).
- Sprint 2 — pass-count pinning + the CI regression gate → `sprint-2-pass-gate.md`
  (`T-0602-NNN`).

## Risks

- Chasing a single ROM's last cycle can destabilize a passing suite — re-run the
  whole battery after every change; the determinism contract makes diffs exact.
- A "fix" that backs an oracle case with a BestEffort board violates ADR 0003 and
  inflates the pass-rate claim — keep the tiering honest.
- Decimal mode + unstable opcodes (ARR/XAA/AHX) are notorious; treat the
  measured-hardware reference as truth, not the datasheet.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
