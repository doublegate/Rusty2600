# Phase 4 — Carts / mappers

**Goal:** the cart model carries the Core + Curated board families with correct
hotspot bank logic and on-cart RAM, all tier-gated under the honesty gate (ADR
0003), with robust scheme detection. The BestEffort long tail is Phase 7.

References: `docs/cart.md`; `docs/adr/0003`; `docs/testing-strategy.md`;
`ref-docs/research-report.md` §8; `crates/rusty2600-cart/src/lib.rs`,
`crates/rusty2600-test-harness/tests/mapper_tier_honesty.rs`.

## Scope

In (Curated tier): F6 (16K), F4 (32K), the Superchip variants F8SC/F6SC/F4SC
(+128 B RAM), FA/CBS RAM+ (12K +256 B RAM), CV (Commavid, +1 KiB RAM); hotspot-
pattern + ROM-DB-assisted `detect()` (8 KiB is ambiguous between F8/E0/FE/3F);
extend the honesty-gate oracle set as each lands.

Out: the BestEffort families (F0/FE/E0/E7/3F/3E/UA/0840/EF/BF/DF/SB/X07/4A50/AR/
DPC/DPC+) — Phase 7; the ARM-thumb interpreter for DPC+/CDF — deep BestEffort,
optional later dependency.

## Exit criteria (verifiable)

- F6, F4, the Superchip variants, FA/CBS-RAM, and CV pass their hotspot/RAM
  register-decode unit tests and boot-smoke against committed/redistributable
  fixtures.
- `detect()` disambiguates the Curated 8 KiB schemes via hotspot pattern + ROM DB.
- `mapper_tier_honesty.rs` still passes with every new board added to the oracle
  set (no BestEffort backs the oracle).
- The Core/Curated boards back the accuracy battery (Phase 6) honestly.

## Sprints

- Sprint 1 — Curated F-series + Superchip + RAM carts → `sprint-1-curated-boards.md`
  (`T-41-NNN`).

## Risks

- 8 KiB ambiguity (F8 vs E0 vs FE vs 3F) needs a ROM DB; mis-detection silently
  corrupts a game.
- The honesty gate's oracle set must be extended in lockstep with each board, or
  the pass-rate claim drifts out of truth (ADR 0003).
- On-cart RAM (Superchip read/write window split) is a common decode bug — pin
  against fixtures.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
