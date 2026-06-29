# Phase 7 — Breadth

**Goal:** extend the cart catalog across the BestEffort long-tail board families
(honesty-gated under ADR 0003) and land the three broadcast regions (NTSC / PAL /
SECAM) as runtime data, not a build fork — so the bulk of the 2600 library boots,
with every board labelled by its accuracy tier.

References: `docs/cart.md` (the scheme catalogue); `docs/compatibility.md`
(region timing as data); `docs/adr/0003` (honesty gate); `crates/rusty2600-cart/`;
`crates/rusty2600-test-harness/tests/mapper_tier_honesty.rs`; `ref-docs/
research-report.md` §8.

## Scope

In (BestEffort tier, honesty-gated): F0/3F/3E bank-register schemes, FE
(Activision Robot Tank/Decathlon), E0 (Parker Bros), E7 (M-Network), UA, 0840,
EF/EFSC, BF/BFSC/DF/DFSC, SB (128K Superbank), X07, 4A50, AR (Supercharger);
the DPC (Pitfall II) music-fetcher coprocessor; optionally DPC+/CDF/CDFJ behind
an ARM-thumb interpreter (deepest BestEffort, may slip). Region as data: the
NTSC 262 / PAL 312 / SECAM 312 line budgets + the three palettes selected at
runtime; auto-detect region from line-count heuristics + a ROM DB.

Out: the accuracy-battery hard-residual closure (Phase 6, already done); reach
features (netplay / RA / TAS / Lua / shaders) — Phase 8.

## Exit criteria (verifiable)

- Each BestEffort board family boots its representative title (boot-smoke
  screenshot vs. a committed/redistributable fixture) and carries `Tier::BestEffort`.
- `detect()` disambiguates the 8 KiB+ schemes via hotspot pattern + ROM DB; a
  mis-detect is caught by the boot-smoke corpus.
- NTSC / PAL / SECAM each render with the correct line budget + palette, selected
  as data (no `cfg` build fork); region auto-detection works on the corpus.
- `mapper_tier_honesty.rs` stays green — no BestEffort board backs the accuracy
  oracle (ADR 0003); the STATUS.md tier counts match the code.

## Sprints

- Sprint 1 — BestEffort bank-register + RAM board families → `sprint-1-besteffort-boards.md`
  (`T-71-NNN`).
- Sprint 2 — region-as-data (NTSC/PAL/SECAM) + auto-detect → `sprint-2-region-data.md`
  (`T-72-NNN`).
- Sprint 3 — DPC / DPC+ / CDF coprocessor boards (optional ARM interpreter) →
  `sprint-3-coprocessor-carts.md` (`T-73-NNN`).

## Risks

- 8 KiB scheme ambiguity (F8 vs E0 vs FE vs 3F) silently corrupts a game on
  mis-detect — the ROM DB must stay in lockstep with the board set.
- A BestEffort board quietly creeping into the oracle inflates the honesty claim
  (ADR 0003) — the gate test must be extended with every board.
- The DPC+/CDFJ ARM-thumb interpreter is a large, optional dependency; scope it
  out cleanly if it threatens the phase, rather than shipping a half model.
- SECAM's 8-colour set splayed over the 128-entry layout is an easy palette bug —
  pin against a SECAM reference.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
