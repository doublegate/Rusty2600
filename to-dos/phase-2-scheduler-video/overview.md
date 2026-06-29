# Phase 2 — Scheduler + video

**Goal:** the color-clock lockstep scheduler drives the 6507 + the TIA in tight
lockstep, and the TIA beam-racing renderer composes a **stable, correct frame** —
RESPx positioning, the HMOVE comb, playfield, players/missiles/ball, and
collisions all verified against the TIA timing test ROMs (Stella as the oracle).

References: `docs/scheduler.md`; `docs/tia.md`; `docs/adr/0001`, `docs/adr/0004`;
`ref-docs/research-report.md` §5, §10; `crates/rusty2600-tia/src/lib.rs`,
`crates/rusty2600-core/src/{scheduler.rs,bus.rs}`.

## Scope

In: the full sparse/mirrored bus decode (TIA write/read regs, RIOT RAM + I/O +
timer, cart window); the TIA beam renderer (emit one dot per visible color clock);
RESPx 9-CLK reset pipeline + NUSIZ copy offsets; HMOVE clock-stuffing + the 8-clock
HBLANK-extend comb; PF0/PF1/PF2 + CTRLPF; GRPx/NUSIZx/REFPx/VDELPx; missiles/ball;
the 15 collision latches; COLUPx → region palette; the WSYNC mid-instruction
freeze point (`T-PS-061`); the `SnapComparator` wired to scanline goldens.

Out: audio (Phase 3 — though the audio register *writes* are decoded here),
board breadth (Phase 4), the frontend (Phase 5).

## Exit criteria (verifiable)

- A known test ROM renders a frame whose composed scanline buffer is byte-identical
  to a committed golden (`SnapComparator::diff_pixels == 0`).
- The RESPx, HMOVE-comb, and collision TIA timing ROMs pass against Stella +
  reference buffers.
- WSYNC stalls the CPU to the exact scanline start; the determinism contract holds
  (same seed ⇒ identical buffer).
- The bus decode round-trips RIOT RAM and TIA register writes correctly.

## Sprints

- Sprint 1 — bus decode + the beam renderer core → `sprint-1-bus-and-beam.md`
  (`T-21-NNN`).
- Sprint 2 — RESPx / HMOVE / collisions (stub — add when Sprint 1 is ~complete).

## Risks

- The RESPx 9-CLK pipeline interacting with HMOVE and HBLANK writes has sub-pixel
  edge cases that need direct Stella verification, not Towers' prose (research
  report §14, item 4) — but all within integer resolution (no ADR 0002 need).
- TIA-revision differences in HMOVE/playfield edges (research report §14, item 3)
  — pick a default revision, parameterize only if a ROM forces it.
- The mid-instruction WSYNC freeze point must not break the Phase 1 per-cycle bus
  ordering.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
