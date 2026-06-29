# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v0.1.0 (scaffold). The Phase 1-4 core components (CPU, TIA video/audio, RIOT, Bus, Core/Curated Carts) are complete. Phase 5 Frontend (rendering, pacing, WASM/thread support) is 100% complete.

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | 100% complete (BCD, undocumented opcodes) |
| `rusty2600-tia` | TIA — video + audio | 100% complete (beam-raced video, Audio LFSRs) |
| `rusty2600-riot` | MOS 6532 RIOT | 100% complete (RAM, DDR ports, Timers) |
| `rusty2600-cart` | Bankswitch boards | Core boards complete (2K/4K/F8/F6/F4); honesty gate live |
| `rusty2600-core` | Bus + scheduler | lockstep loop + seeded phase live; bus decode complete |
| `rusty2600-frontend` | egui shell | 100% complete (rendering, audio, pacing, input, WASM support) |
| `rusty2600-test-harness` | accuracy oracle | shapes present (differ/runner/score/snap); no golden logs yet |

## Accuracy (per-suite pass counts)

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | test-harness | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | golden-log | 0 / TBD |
| SingleStepTests/ProcessorTests `6502` | golden-log | 0 / TBD |
| TIA timing / draw ROMs | test-ROM corpus | 0 / TBD |
| Stella regression corpus | test-ROM corpus | 0 / TBD |
| Accuracy battery (AccuracyCoin-equivalent) | battery | 0% (0 / TBD) |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K (+ F8 stub) |
| Curated | 6 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC) | yes | F8 stub only |
| BestEffort | 17 | F0, FE, E0, E7, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC, DPC+/CDF/CDFJ | **no** | none |

Note: the cart crate currently pins F8 as `Core`; `docs/cart.md` tracks the
research-report split (F8 → Curated). Both are accuracy-gated; reconcile the
label when F6/F4 land (`T-PS-012/013`).

## Version policy

Start at **v0.1.0**; additive features behind default-off flags keep
shipped/native/`no_std`/wasm byte-identical. Drive the accuracy battery to ≥90%
by v1.0, 100% the goal; hard residuals are **deferred and documented**, never
point-fixed (and only the ADR 0002 fractional-timebase refactor — **likely
unneeded** for the 2600 — would close sub-color-clock residuals). Do **NOT**
import RustyNES engine-lineage "v2.0" anchors as releases (the versioning trap).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
