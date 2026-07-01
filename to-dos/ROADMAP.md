# Rusty2600 — Roadmap

Entry point for planning. Each phase links its overview; phases contain sprints;
sprints contain tickets with stable IDs `T-PPSS-NNN` (`PP` = 2-digit phase,
`SS` = 2-digit sprint, `NNN` = 3-digit ticket sequence, all zero-padded),
e.g. `T-0001-003` = phase 0, sprint 1, ticket 3. Reference them in commit
messages. References: `ref-docs/research-report.md`; `docs/architecture.md`;
`docs/STATUS.md` (current-state source of truth).

**Current phase: Phase 5 (frontend).** Phase 0 (foundation) through Phase 4 (carts / mappers) are complete. The workspace compiles as fully wired components with the lockstep scheduler, the seeded determinism phase, the Core/Curated cart boards, and the honesty gate already live. We are now working on filling out the frontend egui shell and wiring it perfectly to the core in Phase 5.

## The phase line

- **Phase 0 — foundation:** workspace + crate skeletons compiling; CI green on
  stubs → `phase-0-foundation/overview.md`
- **Phase 1 — CPU golden log:** the 6507 to 0-diff against the Klaus / ProcessorTests
  golden log (documented + undocumented opcodes, decimal mode) →
  `phase-1-cpu-golden-log/overview.md`
- **Phase 2 — scheduler + video:** the color-clock lockstep scheduler + the TIA
  beam-racing renderer (RESPx, HMOVE comb, playfield, players/missiles/ball,
  collisions) producing a stable frame → `phase-2-scheduler-video/overview.md`
- **Phase 3 — audio:** the TIA two-channel poly-counter synthesis + the
  non-linear mixer → `phase-3-audio/overview.md`
- **Phase 4 — carts / mappers:** the cart model + board breadth, tier-gated under
  the honesty gate → `phase-4-carts-mappers/overview.md`
- **Phase 5 — frontend:** the egui shell + wasm + save-states / rewind / run-ahead
  → `phase-5-frontend/overview.md`
- **Phase 6 — accuracy to ~100:** drive the accuracy battery to target; defer hard
  residuals → `phase-6-accuracy-to-100/overview.md`
- **Phase 7 — breadth:** the BestEffort board families + region timing (NTSC/PAL/
  SECAM) as data → `phase-7-breadth/overview.md`
- **Phase 8 — reach:** netplay / RetroAchievements / TAS / Lua / shaders —
  additive, off-by-default → `phase-8-reach/overview.md`

## Release line

- **v1.0.0** — production cut: Phases 0–6 complete (CPU 0-diff, beam-racing video,
  TIA audio, Core/Curated boards, accuracy battery ≥90% → 100% goal),
  README/CHANGELOG/docs/STATUS in sync, release matrix + Pages green.
- **Beyond v1.0** — Phases 7–8 breadth/reach; and the ADR 0002 fractional-timebase
  refactor **only if** a hard residual warrants it (for the 2600, **likely never**
  — integer color-clock resolution is the machine's native granularity).

## How the phases map to the architecture

Phases 1–4 build the chips bottom-up along the one-directional crate graph
(`docs/architecture.md`): CPU first (no deps), then the scheduler + TIA video,
then TIA audio, then the cart boards. Phase 2 is where the lockstep substrate
(ADR 0001) and the determinism contract (ADR 0004) become load-bearing. The
honesty gate (ADR 0003) is live from Phase 0 and tightened every time a board
lands.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
