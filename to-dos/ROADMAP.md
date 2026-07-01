# Rusty2600 — Roadmap

Entry point for planning. Each phase links its overview; phases contain sprints;
sprints contain tickets with stable IDs `T-PPSS-NNN` (`PP` = 2-digit phase,
`SS` = 2-digit sprint, `NNN` = 3-digit ticket sequence, all zero-padded),
e.g. `T-0001-003` = phase 0, sprint 1, ticket 3. Reference them in commit
messages. References: `ref-docs/research-report.md`; `docs/architecture.md`;
`docs/STATUS.md` (current-state source of truth).

**Current release: v1.0.0 "Foundation" — the first stable release.**
Phase 0 (foundation) through the
full Curated-tier board set (Phase 4) are complete. Phase 7 (BestEffort
breadth) has landed 12 of the ~15-scheme BestEffort long tail cataloged in
`docs/cart.md` (F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB,
X07 — 22 of 25 total schemes in the LOCAL catalogue), leaving only 4A50
(`T-0402-014`), AR/Supercharger (`T-0402-015`), and the ARM-driven DPC+/CDF/
CDFJ/CDFJ+ family (`T-0401-006`) — all substantially larger undertakings
than the rest of the breadth work, deliberately deferred rather than rushed.
`Board::snoop_write`/`snoop_read` (added v0.4.0/v0.4.1) underpin all of
UA/0840/FE/SB/X07. **Phase 5 (frontend) is fully complete** — the real
`debug-hooks` debugger (6507/TIA/RIOT/memory panels, breakpoints/step/
continue, a side-effect-free `Bus::peek`/`peek_range`, a standalone
disassembler) shipped in v0.5.0, and the four chip-crate Criterion benches
are populated with real measured baselines (`docs/performance.md`). The
RetroAchievements slice of Phase 8 is now real: `rusty2600-cheevos` vendors
the `rcheevos` C library and wires a safe `RaClient` into the frontend
(`retroachievements`, off by default) — per-frame achievement tracking,
hardcore mode, and a menu all work; a dedicated achievement-list/login/toast
UI is deferred (`T-0802-005`). **Phase 6's accuracy battery (Layer 4) is now
real** — `rusty2600-test-harness` goes from unused scaffolding to a real
battery: the shared `Sentinel`/`run_cpu_until_sentinel` Layer 2 runner (both
bundled Klaus oracles refactored onto it), a real `AccuracyScore`-gated
`tests/accuracy_battery.rs` (2/2, 100%, CI-enforced via the existing
`--features test-roms` step), and a tolerance-aware `SnapComparator`. Still
honestly deferred: a genuine externally-oracled golden CPU trace log for
`GoldenLogDiffer` and TIA-timing test-ROM fixtures for the Layer 3
`run_until_complete` runner (`T-0602-006`/`007`). RIOT timing, TIA
collision continuity, seeded power-on state, the full SingleStepTests
corpus, and Klaus's decimal test landed in v0.2.0. **`T-0601-008` is fixed
(v0.9.0)** — a rebuilt Gopher2600/Stella differential probe against Pitfall
II found that Rusty2600's RIOT timer never reverted its post-underflow
(divide-by-1) decrement rate back to the normal prescale after an `INTIM`
read (confirmed against Stella's `M6532::peek`/`updateEmulation`); the CPU
now correctly leaves the boot-time wait loop (previously stuck forever) —
see `docs/riot.md` and `CHANGELOG.md`. See `docs/STATUS.md` for the
authoritative per-suite/per-chip state.

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

Iterative `v0.x.0` releases (each a real GitHub tag + release, `v0.x.y` for
post-`.0` fixes), extending the phases above with a debugger and
RetroAchievements pulled forward into the v1.0.0 gate, and cart breadth
pushed toward Stella-adjacent parity rather than stopping at the original
Core/Curated set:

| Version | Content |
|---|---|
| v0.1.1 / v0.1.2 | Truth-pass (docs/code reconciliation) + release-CI fixes |
| v0.2.0 "Cycle-Exact" | RIOT/TIA accuracy hardening, ADRs 0005/0006, full SingleStepTests + Klaus decimal in CI, CPU-crate cleanup |
| v0.3.0 "Curated" | Curated-tier cart schemes finished (CV/FA/Superchip/DPC/E7), all wired into `detect()` via Stella-ported hotspot heuristics (`T-0401-009`) |
| v0.4.0 "Breadth" | BestEffort cart breadth toward Stella-adjacent parity (staged patch train) — Batches 1-2 done (F0/E0/3F/3E/EF/DF/BF, 7 schemes) + `Board::snoop_write` |
| v0.4.1 | Continues the Batch 2 patch train — UA/0840 (2 more schemes) + `Board::snoop_read`; FE/SB/X07/4A50 and Batches 3-5 (DPC-family, ARM/peripheral, multicarts) target v0.4.2+ |
| v0.5.0 "Inspector" | Real `debug-hooks` debugger (6507/TIA/RIOT/memory panels, breakpoints/step/continue, `Bus::peek`/`peek_range`, a standalone disassembler); performance benches populated with real Criterion baselines |
| v0.6.0 "Catalog" | Closes 22 of the local 25-scheme catalogue (`docs/cart.md`): FE, SB, X07 land (`T-0402-006`/`011`, DONE) alongside the existing 19. 4A50 (`T-0402-014`), AR/Supercharger (`T-0402-015`), and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family (`T-0401-006`, needs a full ARM7TDMI Thumb interpreter) are substantially larger undertakings, deliberately deferred to a v0.6.x patch train rather than rushed |
| v0.7.0 "Cheevos" | RetroAchievements (`rusty2600-cheevos`, `T-0802-001..004`, DONE): vendors `rcheevos`, wires a safe `RaClient` into the frontend behind the off-by-default `retroachievements` feature — real per-frame achievement tracking, hardcore mode, a menu. A dedicated achievement-list/login/toast UI is deferred (`T-0802-005`) |
| v0.8.0 "Battery" | The accuracy battery stood up for real (`T-0602-001..005`, DONE): shared `Sentinel`/`run_cpu_until_sentinel` Layer 2 runner, a real `AccuracyScore`-gated `accuracy_battery.rs` (2/2, 100%, CI-enforced via the existing `test-roms` step — no new CI YAML needed), tolerance-aware `SnapComparator`. A genuine externally-oracled golden CPU trace log and TIA-timing test-ROM fixtures remain deferred (`T-0602-006`/`007`) |
| v0.9.0 "Hardening" | `T-0601-008` fixed (Pitfall II's boot-time RIOT-timer wait loop, found via a rebuilt Gopher2600/Stella differential probe): reading `INTIM` now reverts the post-underflow decrement rate to the normal prescale, confirmed against Stella's `M6532::peek`/`updateEmulation` (`docs/riot.md`). Commercial-ROM regression oracle expansion remains blocked by data availability (locally-supplied dumps only, none available); doc-sync pass done across `docs/architecture.md`/`compatibility.md` |
| **v1.0.0 "Foundation"** (current) | The first stable release. Every gate below is met: accuracy battery 2/2 (100%, ≥90% threshold), `debug-hooks` debugger + `retroachievements` both shipped, 22/25 cataloged cart schemes (Stella-adjacent breadth), green three-platform release matrix. No code changed from v0.9.0 — a version-line milestone plus a full doc/status reconciliation pass (`CHANGELOG.md`'s `[1.0.0]` entry). |

Explicit v1.0.0 non-requirements: netplay, TAS tooling, Lua scripting, HD
texture packs, shader stacks, mobile builds, and RA server-side allowlisting
(the integration working is the bar, not third-party approval) — all
Beyond-v1.0 (Phase 7 residual breadth / Phase 8 reach), plus the ADR 0002
fractional-timebase refactor **only if** a hard residual ever warrants it
(for the 2600, **likely never** — integer color-clock resolution is the
machine's native granularity).

## Beyond v1.0.0

With v1.0.0 shipped, further work is battery-driven hardening and residual
breadth rather than gated milestones:

- The three deferred cart-scheme families: 4A50 (`T-0402-014`), AR/
  Supercharger (`T-0402-015`), and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family
  (`T-0401-006`, needs a full ARM7TDMI Thumb interpreter) — each a
  substantially larger, separately-scoped undertaking.
- A genuine externally-oracled golden CPU trace log (`T-0602-007`) and
  TIA-timing test-ROM fixtures (`T-0602-006`) for the accuracy battery's
  remaining deferred layers.
- The RetroAchievements achievement-list/login/toast UI (`T-0802-005`).
- The commercial-ROM regression oracle, whenever locally-supplied ROM dumps
  become available in this environment.
- Any further hardening the accuracy battery surfaces, released as
  `v1.x.0`/`v1.x.y` per the same iterative-release discipline used
  throughout the v0.x.0 line.

## How the phases map to the architecture

Phases 1–4 build the chips bottom-up along the one-directional crate graph
(`docs/architecture.md`): CPU first (no deps), then the scheduler + TIA video,
then TIA audio, then the cart boards. Phase 2 is where the lockstep substrate
(ADR 0001) and the determinism contract (ADR 0004) become load-bearing. The
honesty gate (ADR 0003) is live from Phase 0 and tightened every time a board
lands.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
