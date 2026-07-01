# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v0.7.0 "Cheevos" (see `to-dos/ROADMAP.md`
for the full v0.1.1→v1.0.0 version-to-phase plan and `CHANGELOG.md`'s
`[0.7.0]` entry for the complete list) — the full 8-scheme Curated cart tier
(v0.3.0) plus 12 BestEffort schemes (F0, E0, 3F, 3E, EF/EFSC, DF/DFSC,
BF/BFSC, UA, 0840, FE, SB, X07) are implemented and wired into automatic
`detect()` — 22 of the 25 schemes in the LOCAL catalogue (`docs/cart.md`).
Two `Board` hooks, `snoop_write` and `snoop_read`, let bankswitch schemes
react to accesses the console routes to TIA/RIOT space (not just the
`$1000+` cart window) — FE, SB, and X07 all depend on them, FE additionally
using `snoop_read`'s `val` parameter to pick a bank from the observed JSR
stack-frame byte. Only 4A50 (`T-0402-014`), AR/Supercharger (`T-0402-015`),
and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family (`T-0401-006`, needing a full
ARM7TDMI Thumb interpreter) remain — all substantially larger undertakings
than the rest of the breadth work, deliberately deferred to a documented
follow-up rather than rushed. Phase 5 Frontend is **fully complete**:
rendering, audio, pacing, input, WASM/thread support, AND the real
`debug-hooks` debugger (live 6507/TIA/RIOT/memory panels, breakpoints/step/
continue, a side-effect-free `Bus::peek`/`peek_range`, a standalone
disassembler — `to-dos/phase-5-frontend/sprint-4-debugger.md`), plus
populated Criterion benches with real measured baselines
(`docs/performance.md`). Phase 8's RetroAchievements slice is now real:
`rusty2600-cheevos` vendors the `rcheevos` C library and wires a safe
`RaClient` into the frontend (`retroachievements` feature, off by default)
— per-frame achievement tracking, hardcore mode, and a RetroAchievements
menu all work; a dedicated achievement-list/login/toast UI is deferred
(`to-dos/phase-8-reach/sprint-2-ra-and-tas.md`, `T-0802-005`).

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | Documented + undocumented opcodes implemented; cycle-exact against both the trimmed and full SingleStepTests corpus, and Bruce Clark's exhaustive decimal-mode test (all ERROR=0). Split into `status.rs`/`bus.rs`/`cpu.rs` + a thin `lib.rs` (v0.2.0, `T-0601-006`) — the crate previously also carried a second, entirely dead, never-compiled RustyNES-lineage CPU implementation (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`, ~3,560 lines, no `mod` declarations ever wired them in) plus stale NES-flavored comment prose in the one live file; both fully resolved, see `docs/cpu.md`. |
| `rusty2600-tia` | TIA — video + audio | Beam-raced video (RESPx/HMOVE/playfield/players/missiles/ball/collisions) + audio poly-counter synthesis implemented and unit-tested. RIOT-timer-adjacent edge cases, AUDC 0xA/0xB pinning, TIA-revision modeling, and power-on RAM determinism are open (v0.2.0). |
| `rusty2600-riot` | MOS 6532 RIOT | RAM/DDR ports/timer implemented and unit-tested (prescale, underflow, INSTAT). Read-after-write `INTIM` edge case still needs verification against the DirtyHairy/Stella model (v0.2.0). |
| `rusty2600-cart` | Bankswitch boards | All 8 Curated schemes (2K, 4K, F8, F6, F4, CV, FA/CBS-RAM, Superchip, DPC, E7) implemented and wired into `detect()` (v0.3.0). BestEffort (v0.4.0-v0.6.0): F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB, X07 implemented and wired (22 of 25 catalogued schemes total). Two hooks, `Board::snoop_write`/`snoop_read` (`crates/rusty2600-core/src/bus.rs`), let boards react to accesses the console routes to TIA/RIOT space — needed for 3F/3E's `$3E`/`$3F` write hotspots, UA/0840/X07's read+write hotspots, FE's `$01FE` stack-frame-value pickup, and SB's address-low-bits bank select, none of which are in the cart window at all. Only 4A50 (`T-0402-014`, needs three independently relocatable ROM/RAM windows), AR/Supercharger (`T-0402-015`, tape/audio-based loading, architecturally unlike every other scheme here), and DPC+/CDF/CDFJ/CDFJ+ (`T-0401-006`, need a full ARM7TDMI Thumb interpreter) remain — all deliberately deferred as substantially larger, separately-scoped undertakings. |
| `rusty2600-core` | Bus + scheduler | lockstep loop + seeded phase live; bus decode complete |
| `rusty2600-frontend` | egui shell | Rendering, audio, pacing, input, WASM support, the emu-thread path, the real debugger (`debug-hooks`, default-on), and now RetroAchievements (`retroachievements`, off by default: `cheevos.rs` owns an `RaClient` on the main thread, pumped once per frame under the brief lock, ROM load/close wired, hardcore-mode menu, unlock events surfaced as status text) all real and tested (v0.5.0-v0.7.0). HD-pack remains an unwired stub. |
| `rusty2600-cheevos` | RetroAchievements FFI | Vendors the `rcheevos` C library (MIT); safe `RaClient` wrapper adapted from RustyNES's own `rustynes-cheevos` (console-agnostic except the memory map + one console-ID constant). `ra_addr_to_riot` maps RA's flat address space directly onto the RIOT's 128 bytes of RAM. Native-only (`#![cfg(not(target_arch = "wasm32"))]`); 7 tests passing, including real FFI smoke tests (v0.7.0). |
| `rusty2600-test-harness` | accuracy oracle | Shapes present (`GoldenLogDiffer`/`run_until_complete`/`AccuracyScore`/`SnapComparator`); Klaus functional golden-log passes; the full AccuracyCoin-style battery (live trace buffer, suite result-protocol polling, tolerance-aware snap compare) is still TODO — v0.8.0. |

## Accuracy (per-suite pass counts)

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | test-harness (`--features test-roms`) | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | test-harness (`--features test-roms`) | 1 / 1 — wired v0.2.0; exhaustive 256×256×2-carry-in `ADC`/`SBC` decimal-mode sweep, `ERROR=0` (bit-exact) |
| SingleStepTests/`65x02` `6502` (trimmed: 20 cases/opcode) | cycle-exact audit | 4,660 / 4,660 cases, 233 / 233 opcodes |
| SingleStepTests/`65x02` `6502` (full corpus, ~10K cases/opcode) | cycle-exact audit | wired v0.2.0 — `.github/workflows/singlestep-full.yml`, weekly cron + manual dispatch (not per-push: ~700 MB download across 233 opcodes) |
| TIA timing / draw ROMs | test-ROM corpus | not yet wired (v0.8.0) |
| Stella regression corpus | test-ROM corpus | not yet wired (v0.8.0/v0.9.x) |
| Accuracy battery (AccuracyCoin-equivalent) | battery | not yet stood up (v0.8.0) |
| **Workspace test suite** | `cargo test --workspace` | **142 / 142** (both Klaus tests moved to `--features test-roms`, gated out of the fast default path — see `crates/rusty2600-test-harness/tests/klaus_test.rs`) |
| **Workspace test suite (`--features test-roms`)** | `cargo test --workspace --features test-roms` | **144 / 144** |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K |
| Curated | 8 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC), DPC, E7 | yes | all 8, all wired into `detect()` (`T-0401-009` closed the CV/Superchip/E7 same-size collisions via hotspot-pattern heuristics). DPC and E7 both reclassified from `docs/cart.md`'s original BestEffort listing (see its tier-totals note) |
| BestEffort | 15 | F0, FE, E0, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC+/CDF/CDFJ | **no** | F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB, X07 (`T-0402-001..004`/`006`/`008..011`) — 12 of 15, all wired into `detect()`. Only 4A50 (`T-0402-014`), AR (`T-0402-015`), and DPC+/CDF/CDFJ (`T-0401-006`, ARM-driven) remain, each deliberately deferred as a substantially larger, separately-scoped undertaking (v0.6.x follow-up). |

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
