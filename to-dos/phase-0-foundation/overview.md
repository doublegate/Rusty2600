# Phase 0 — Foundation

**Goal:** the workspace and all crate skeletons compile; CI is green on the
stubs; the lockstep substrate, the determinism seed, the Core cart boards, and
the honesty gate exist so every later phase has a frame to hang work on.

References: `docs/architecture.md`; `docs/scheduler.md`; `docs/adr/0001`,
`docs/adr/0003`, `docs/adr/0004`; `ref-docs/research-report.md` §12, §13.

## Scope

In: the `rusty2600-{cpu,tia,riot,cart,core,frontend,test-harness}` crate split;
the `Bus` (no `wram` field) + `System` lockstep loop; the seeded power-on phase;
the `Tier` enum + the Core boards (2K/4K/F8) + `detect()`; the honesty-gate test;
the harness shapes (`GoldenLogDiffer`, `run_until_complete`, `AccuracyScore`,
`SnapComparator`); CI (`cargo check/test/clippy/fmt`, `no_std` cross-compile of
the chip stack).

Out: any real chip engine (Phases 1–3), real bus decode (Phase 2), board breadth
(Phase 4), the frontend UI (Phase 5).

## Exit criteria (verifiable)

- `cargo check --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo fmt --all --check` all pass.
- The chip stack cross-compiles `no_std`:
  `cargo build -p rusty2600-cpu --target thumbv7em-none-eabihf --no-default-features`
  (and the same for `-tia`, `-riot`, `-cart`).
- `mapper_tier_honesty.rs` passes (no BestEffort board in the oracle set).
- `System::new(seed)` is deterministic for a given seed
  (`seeded_phase_is_deterministic` green).
- `docs/STATUS.md` reflects v0.1.0 with all suite counts at 0 / TBD.

## Sprints

- Sprint 1 — workspace + substrate + CI → `sprint-1-workspace-substrate.md`
  (`T-0001-NNN`).

## Risks

- Drift between the cart crate's tier labels (F8 pinned `Core`) and `docs/cart.md`
  (F8 → Curated) — both accuracy-gated, reconcile when F6/F4 land.
- `no_std` regressions sneaking in via a `std`-only dependency on a chip crate —
  the cross-compile gate must stay in CI.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
