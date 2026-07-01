# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v0.3.0 "Curated" (see `to-dos/ROADMAP.md` for the full
v0.1.1→v1.0.0 version-to-phase plan and `CHANGELOG.md`'s `[0.3.0]` entry for
the complete list) — the full 8-scheme Curated cart tier landed, all wired
into automatic `detect()`. **v0.4.0 "Breadth" is in progress**: Batch 1 of
the BestEffort long tail (F0, E0, 3F, 3E) is implemented, including a new
`Board::snoop_write` hook so bankswitch schemes that trigger on TIA/RIOT-
mirrored addresses (not just the `$1000+` cart window) can be modeled at
all — see `to-dos/phase-4-carts-mappers/sprint-2-besteffort-boards.md`.
Phase 5 Frontend (rendering, pacing, input, WASM/thread support) has a
working synchronous AND dedicated emu-thread path (`emu-thread` default-on);
its debugger panels are still TODO stubs (targeted for v0.5.0).

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | Documented + undocumented opcodes implemented; cycle-exact against both the trimmed and full SingleStepTests corpus, and Bruce Clark's exhaustive decimal-mode test (all ERROR=0). Split into `status.rs`/`bus.rs`/`cpu.rs` + a thin `lib.rs` (v0.2.0, `T-0601-006`) — the crate previously also carried a second, entirely dead, never-compiled RustyNES-lineage CPU implementation (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`, ~3,560 lines, no `mod` declarations ever wired them in) plus stale NES-flavored comment prose in the one live file; both fully resolved, see `docs/cpu.md`. |
| `rusty2600-tia` | TIA — video + audio | Beam-raced video (RESPx/HMOVE/playfield/players/missiles/ball/collisions) + audio poly-counter synthesis implemented and unit-tested. RIOT-timer-adjacent edge cases, AUDC 0xA/0xB pinning, TIA-revision modeling, and power-on RAM determinism are open (v0.2.0). |
| `rusty2600-riot` | MOS 6532 RIOT | RAM/DDR ports/timer implemented and unit-tested (prescale, underflow, INSTAT). Read-after-write `INTIM` edge case still needs verification against the DirtyHairy/Stella model (v0.2.0). |
| `rusty2600-cart` | Bankswitch boards | All 8 Curated schemes (2K, 4K, F8, F6, F4, CV, FA/CBS-RAM, Superchip, DPC, E7) implemented and wired into `detect()` (v0.3.0). BestEffort Batch 1 (v0.4.0, in progress): F0, E0, 3F, 3E implemented and wired (14 of 25 catalogued schemes total). A new `Board::snoop_write` hook (`crates/rusty2600-core/src/bus.rs`) lets boards react to writes the console routes to TIA/RIOT space — needed for 3F/3E's `$3E`/`$3F` hotspots, which aren't in the cart window at all. UA/0840/FE need the same treatment on READS too (`T-0402-006`, deferred — a bigger interface question, not rushed). The remaining ~35-scheme BestEffort long tail (Batches 2-5) targets the rest of v0.4.x. |
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
| **Workspace test suite** | `cargo test --workspace` | **111 / 111** (both Klaus tests moved to `--features test-roms`, gated out of the fast default path — see `crates/rusty2600-test-harness/tests/klaus_test.rs`) |
| **Workspace test suite (`--features test-roms`)** | `cargo test --workspace --features test-roms` | **113 / 113** |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K |
| Curated | 8 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC), DPC, E7 | yes | all 8, all wired into `detect()` (`T-0401-009` closed the CV/Superchip/E7 same-size collisions via hotspot-pattern heuristics). DPC and E7 both reclassified from `docs/cart.md`'s original BestEffort listing (see its tier-totals note) |
| BestEffort | 15 | F0, FE, E0, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC+/CDF/CDFJ | **no** | F0, E0, 3F, 3E (v0.4.0 Batch 1, `T-0402-001..004`), all wired into `detect()`. UA/0840/FE need `snoop_read` (`T-0402-006`); the remaining Batches 2-5 (~35 schemes) target the rest of v0.4.x. |

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
