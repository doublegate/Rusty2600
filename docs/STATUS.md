# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v0.2.0 "Cycle-Exact" (see `to-dos/ROADMAP.md` for the
full v0.1.1→v1.0.0 version-to-phase plan and `CHANGELOG.md`'s `[0.2.0]` entry
for the complete list). RIOT read-after-write timing, TIA collision-latch
continuity, seeded power-on RAM/registers (ADR 0006), TIA revision modeling
(ADR 0005), the full SingleStepTests corpus + Klaus decimal test wired into
CI, and a substantial CPU-crate cleanup (a second, entirely dead RustyNES-
lineage implementation removed; the live code split into `status.rs`/
`bus.rs`/`cpu.rs`) all landed this release. Phase 5 Frontend (rendering,
pacing, input, WASM/thread support) has a working synchronous AND dedicated
emu-thread path (`emu-thread` default-on); its debugger panels are still
TODO stubs (targeted for v0.5.0).

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | Documented + undocumented opcodes implemented; cycle-exact against both the trimmed and full SingleStepTests corpus, and Bruce Clark's exhaustive decimal-mode test (all ERROR=0). Split into `status.rs`/`bus.rs`/`cpu.rs` + a thin `lib.rs` (v0.2.0, `T-0601-006`) — the crate previously also carried a second, entirely dead, never-compiled RustyNES-lineage CPU implementation (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`, ~3,560 lines, no `mod` declarations ever wired them in) plus stale NES-flavored comment prose in the one live file; both fully resolved, see `docs/cpu.md`. |
| `rusty2600-tia` | TIA — video + audio | Beam-raced video (RESPx/HMOVE/playfield/players/missiles/ball/collisions) + audio poly-counter synthesis implemented and unit-tested. RIOT-timer-adjacent edge cases, AUDC 0xA/0xB pinning, TIA-revision modeling, and power-on RAM determinism are open (v0.2.0). |
| `rusty2600-riot` | MOS 6532 RIOT | RAM/DDR ports/timer implemented and unit-tested (prescale, underflow, INSTAT). Read-after-write `INTIM` edge case still needs verification against the DirtyHairy/Stella model (v0.2.0). |
| `rusty2600-cart` | Bankswitch boards | 2K, 4K, F8, F6, F4, CV, FA/CBS-RAM, Superchip (F8SC/F6SC/F4SC) implemented (8 of 25 catalogued schemes); honesty gate live and tier-consistent (all Curated). Curated tier is now feature-complete except DPC and E7, both still targeting v0.3.0; the ~50-scheme BestEffort long tail targets v0.4.x. |
| `rusty2600-core` | Bus + scheduler | lockstep loop + seeded phase live; bus decode complete |
| `rusty2600-frontend` | egui shell | Rendering, audio, pacing, input, WASM support, and the emu-thread path all real and tested. Debugger (`debug-hooks`), HD-pack, and RetroAchievements feature flags remain unwired stubs — v0.5.0 (debugger) and v0.6.0 (RA) respectively. |
| `rusty2600-test-harness` | accuracy oracle | Shapes present (`GoldenLogDiffer`/`run_until_complete`/`AccuracyScore`/`SnapComparator`); Klaus functional golden-log passes; the full AccuracyCoin-style battery (live trace buffer, suite result-protocol polling, tolerance-aware snap compare) is still TODO — v0.7.0. |

## Accuracy (per-suite pass counts)

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | test-harness (`--features test-roms`) | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | test-harness (`--features test-roms`) | 1 / 1 — wired v0.2.0; exhaustive 256×256×2-carry-in `ADC`/`SBC` decimal-mode sweep, `ERROR=0` (bit-exact) |
| SingleStepTests/`65x02` `6502` (trimmed: 20 cases/opcode) | cycle-exact audit | 4,660 / 4,660 cases, 233 / 233 opcodes |
| SingleStepTests/`65x02` `6502` (full corpus, ~10K cases/opcode) | cycle-exact audit | wired v0.2.0 — `.github/workflows/singlestep-full.yml`, weekly cron + manual dispatch (not per-push: ~700 MB download across 233 opcodes) |
| TIA timing / draw ROMs | test-ROM corpus | not yet wired (v0.7.0) |
| Stella regression corpus | test-ROM corpus | not yet wired (v0.7.0/v0.8.x) |
| Accuracy battery (AccuracyCoin-equivalent) | battery | not yet stood up (v0.7.0) |
| **Workspace test suite** | `cargo test --workspace` | **87 / 87** (both Klaus tests moved to `--features test-roms`, gated out of the fast default path — see `crates/rusty2600-test-harness/tests/klaus_test.rs`) |
| **Workspace test suite (`--features test-roms`)** | `cargo test --workspace --features test-roms` | **89 / 89** |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K |
| Curated | 6 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC) | yes | all 6 — Curated tier complete (v0.3.0) |
| BestEffort | 17 | F0, FE, E0, E7, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC, DPC+/CDF/CDFJ | **no** | none (target v0.4.x — the development plan extends this tier toward Stella's ~55-60-scheme catalogue, well past this 17) |

The F8 Core-vs-Curated tier discrepancy (`T-0401-008`) is **resolved**:
`BankF8::tier()` returns `Curated`, matching `docs/cart.md` and pinned by
`mapper_tier_honesty.rs`'s `core_tier_is_reserved_for_unbanked_schemes` test.

## Version policy

Additive features behind default-off flags keep shipped/native/`no_std`/wasm
byte-identical. Drive the accuracy battery to ≥90% by v1.0, 100% the goal;
hard residuals are **deferred and documented**, never point-fixed (and only
the ADR 0002 fractional-timebase refactor — **likely unneeded** for the
2600 — would close sub-color-clock residuals). Do **NOT** import RustyNES
engine-lineage "v2.0" anchors as releases (the versioning trap). Bump the
workspace + every crate's `Cargo.toml` version to match BEFORE tagging and
pushing a release (never after); every GitHub release's notes are the full,
comprehensive `CHANGELOG.md` entry for that version via `--notes-file`, not
an abbreviated tag-annotation summary. See `to-dos/ROADMAP.md` for the full
v0.1.1→v1.0.0 version-to-phase mapping.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
