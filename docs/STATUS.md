# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** unreleased (v0.1.1 pending — Sprint 0 truth pass; see
`to-dos/ROADMAP.md` for the full v0.1.1→v1.0.0 version-to-phase plan). The
Phase 1-4 core components (CPU, TIA video/audio, RIOT, Bus, Core/Curated cart
boards implemented so far) are real and tested (74 tests passing
workspace-wide, `cargo test --workspace`). Phase 5 Frontend (rendering,
pacing, input, WASM/thread support) has a working synchronous AND dedicated
emu-thread path (`emu-thread` default-on as of this release); its debugger
panels are still TODO stubs (targeted for v0.5.0). This entry replaces an
earlier draft that under-reported the actual pass counts below.

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | Documented + undocumented opcodes implemented; cycle-exact against a trimmed SingleStepTests corpus (see Accuracy below). Full untrimmed corpus + Klaus decimal golden-log still open (v0.2.0). Also carries a confirmed-dead NES-lineage IRQ/NMI/mapper-IRQ/APU-frame-IRQ vestige (`rusty2600-core` never overrides the relevant `Bus` trait methods, so it's inert) contradicting `docs/cpu.md`'s "no interrupt dispatch" — scheduled for removal, `T-0601-006` (v0.2.0). |
| `rusty2600-tia` | TIA — video + audio | Beam-raced video (RESPx/HMOVE/playfield/players/missiles/ball/collisions) + audio poly-counter synthesis implemented and unit-tested. RIOT-timer-adjacent edge cases, AUDC 0xA/0xB pinning, TIA-revision modeling, and power-on RAM determinism are open (v0.2.0). |
| `rusty2600-riot` | MOS 6532 RIOT | RAM/DDR ports/timer implemented and unit-tested (prescale, underflow, INSTAT). Read-after-write `INTIM` edge case still needs verification against the DirtyHairy/Stella model (v0.2.0). |
| `rusty2600-cart` | Bankswitch boards | 2K, 4K, F8, F6, F4 implemented (5 of 25 catalogued schemes); honesty gate live and tier-consistent (F8/F6/F4 all Curated, matching `docs/cart.md`). Remaining Curated schemes (CV, FA/CBS-RAM, Superchip, DPC, E7) target v0.3.0; the ~50-scheme BestEffort long tail targets v0.4.x. |
| `rusty2600-core` | Bus + scheduler | lockstep loop + seeded phase live; bus decode complete |
| `rusty2600-frontend` | egui shell | Rendering, audio, pacing, input, WASM support, and the emu-thread path all real and tested. Debugger (`debug-hooks`), HD-pack, and RetroAchievements feature flags remain unwired stubs — v0.5.0 (debugger) and v0.6.0 (RA) respectively. |
| `rusty2600-test-harness` | accuracy oracle | Shapes present (`GoldenLogDiffer`/`run_until_complete`/`AccuracyScore`/`SnapComparator`); Klaus functional golden-log passes; the full AccuracyCoin-style battery (live trace buffer, suite result-protocol polling, tolerance-aware snap compare) is still TODO — v0.7.0. |

## Accuracy (per-suite pass counts)

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | test-harness | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | golden-log | not yet wired (v0.2.0) |
| SingleStepTests/`65x02` `6502` (trimmed: 20 cases/opcode) | cycle-exact audit | 4,660 / 4,660 cases, 233 / 233 opcodes |
| SingleStepTests/`65x02` `6502` (full corpus, ~10K cases/opcode) | cycle-exact audit | not yet run in CI (`SINGLESTEP_VECTORS_DIR` override exists; a dedicated slow CI job is v0.2.0) |
| TIA timing / draw ROMs | test-ROM corpus | not yet wired (v0.7.0) |
| Stella regression corpus | test-ROM corpus | not yet wired (v0.7.0/v0.8.x) |
| Accuracy battery (AccuracyCoin-equivalent) | battery | not yet stood up (v0.7.0) |
| **Workspace test suite** | `cargo test --workspace` | **74 / 74** |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K |
| Curated | 6 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC) | yes | F8, F6, F4 (3 of 6; CV/FA/SC target v0.3.0) |
| BestEffort | 17 | F0, FE, E0, E7, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC, DPC+/CDF/CDFJ | **no** | none (target v0.4.x — the development plan extends this tier toward Stella's ~55-60-scheme catalogue, well past this 17) |

The F8 Core-vs-Curated tier discrepancy (`T-0401-008`) is **resolved**:
`BankF8::tier()` returns `Curated`, matching `docs/cart.md` and pinned by
`mapper_tier_honesty.rs`'s `core_tier_is_reserved_for_unbanked_schemes` test.

## Version policy

Start at **v0.1.0**; additive features behind default-off flags keep
shipped/native/`no_std`/wasm byte-identical. Drive the accuracy battery to ≥90%
by v1.0, 100% the goal; hard residuals are **deferred and documented**, never
point-fixed (and only the ADR 0002 fractional-timebase refactor — **likely
unneeded** for the 2600 — would close sub-color-clock residuals). Do **NOT**
import RustyNES engine-lineage "v2.0" anchors as releases (the versioning trap).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
