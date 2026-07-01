# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v1.8.0 "Oracle" — the eighth release of the
`v1.1.0 -> v2.0.0` RustyNES-parity line (see `to-dos/ROADMAP.md` for the
full plan and `CHANGELOG.md`'s `[1.8.0]` entry). Closes `T-0602-007`:
`GoldenLogDiffer` now bundles a genuine externally-oracled golden CPU
trace (`tests/golden/klaus_functional_test_gopher2600.trace`, 20,000
retired instructions of the Klaus functional test, captured by running
Gopher2600's `hardware/cpu` package directly against the identical CC0
ROM). `GoldenLogDiffer::bundled()` reports `true`;
`tests/golden_log_test.rs` confirms `first_divergence() == None` — two
independently-implemented 6502 cores agreeing register-for-register and
cycle-for-cycle, real external validation distinct from Klaus's own
internal pass/fail trap. `T-0602-006` (a TIA-timing test-ROM corpus for
Layer 3's `run_until_complete`) stays honestly a stub: researched again
this release, confirming no freely-redistributable 2600-specific TIA/RIOT
test-ROM corpus exists (Gopher2600's own README makes the same admission;
the Atari Diagnostic Test Cartridge 2.0 is official service-center
software, not redistributable) — a real, permanent scope boundary, not a
gap to close by more effort. TIA/RIOT accuracy work continues via the
differential-oracle method against specific known-hard titles, the same
technique that found the real Frogger WSYNC-jitter and Pitfall II
RIOT-timer bugs. See `docs/testing-strategy.md` for the full detail.

Earlier in the line: `v1.7.0 "Chronicle"` added a `.r26m` TAS movie
format (`rusty2600-core::movie`, `no_std`-compatible): a start point
(fresh seeded power-on per ADR 0006, or an embedded save-state blob — a
branch point is exactly this) plus a per-frame `MovieFrame` input log
(joystick directions/fire, four paddle positions + fire, console
switches — the latter per-frame since they can change mid-run on real
hardware). Deliberately mirrors `save_state.rs`'s header conventions
(magic, format version, `rom_tag`, `postcard`-encoded, typed errors)
without depending on it beyond reusing `SaveState`'s own encoding for
embedded branch points. Also adds a TAStudio-lite piano-roll panel
(`debugger/tastudio_panel.rs`, click-to-toggle editing, jump-to-frame via
the existing `[1.1.0]` rewind ring, branch points as separate `.r26m`
files) plus two cheap generic riders: `debugger/access_counter.rs`
(per-address write-count heatmap) and `debugger/memory_compare_panel.rs`
(byte-diff two memory snapshots). **Live per-frame recording is not yet
wired into `EmuCore::run_frame`'s hot path** — the format, panel state
machine, and manual jump/branch actions are real and tested, but nothing
auto-appends a frame every tick yet (the same honest-partial-landing call
`[1.4.0]`'s sprite-pack data model made). See `docs/movie.md` for the full
architecture and scope.

Earlier still: `v1.6.0 "Coprocessor"` added a new `rusty2600-thumb`
crate: a real ARM7TDMI Thumb-1 interpreter ported from Gopher2600's Go
implementation (`hardware/memory/cartridge/arm/`), not Stella's C++
`Thumbulator` — registers, N/Z/C/V status flags, the `ThumbMemory` trait
seam, an approximate N/S/I cycle + MAM prefetch-latch timing model, and
all 19 Thumb-1 instruction-format classes, with 27 hand-authored
conformance tests. That release landed the interpreter core only — it is
still **not yet wired into any `rusty2600-cart` `Board`/`Cartridge`
variant**; the `v1.6.x` patch train wires DPC+, then CDF, then CDFJ/CDFJ+
into `detect()` one family at a time (`T-0401-006`), closing the bankswitch
catalogue to 24 of 25 (AR/Supercharger remains its own separately-scoped
follow-up, per `[1.5.0]`). See `docs/thumb.md` for the full architecture,
scope, and deviations from the Go reference.

Earlier still: `v1.5.0 "Full Catalog"` implemented `Bank4A50`
(`T-0402-014`, BestEffort): three independently relocatable ROM/RAM
segments (2K/1.5K/256B) plus a fixed 256B trailer, driven by a
previous-access-gated hotspot state machine ported faithfully from
Stella's `Cartridge4A50::checkBankSwitch`, wired into `detect()`'s 64K/128K
size branches via `is_probably_4a50()` (the NMI-vector `$4A50` signature,
falling back to a reset-vector-targets-a-`NOP $6Exx`/`$6Fxx` heuristic) —
closing the catalogue to 23 of 25 schemes. AR/Supercharger (`T-0402-015`)
was deliberately NOT attempted in that release: even its "fast-load"
(ROM-image-only) mode needs a bank-config decode, a delayed-write protocol
keyed on 5 DISTINCT bus accesses (this crate has no equivalent of Stella's
global CPU-side access counter), and a synthesized dummy 6502 BIOS stub
whose exact bytes haven't been sourced — substantially larger than every
other scheme in the catalogue, so it stays its own separately-scoped
follow-up. `v1.4.0 "Signal"` added a composable post-process
shader stack (new `rusty2600-gfx-shaders` crate +
`rusty2600-frontend::shader_pass`: `CrtScanline` + an honestly-labeled
`CompositeArtifact` approximation, toggleable from Settings, empty-stack
default preserving the byte-identical build), plus the data-model half of
the 2600-appropriate HD-pack analog (`sprite_pack`, `hd-pack` feature:
replacement bitmaps keyed by `GRPx`/`NUSIZx`) — its live rendering splice
is honestly deferred pending a TIA object-ID mask, a genuine architectural
prerequisite not yet built. `v1.3.0 "Scope"` added debugger depth (a
watch/conditional-breakpoint expression engine, a call stack, a TIA
write-scatter viewer, a player/missile/ball panel) plus the RetroAchievements
achievement-list/login/toast UI (`T-0802-005`, DONE). `v1.2.0 "Foresight"`
shipped run-ahead built on `[1.1.0]`'s save-state snapshot primitives,
fixing a real `Tia::scanline` `u16` overflow panic along the way. `v1.1.0
"Persistence"` shipped save-states (`rusty2600-core::save_state`, ADR 0007)
and a rewind rework, plus fixes for three real frontend bugs found during
manual verification. See `CHANGELOG.md`'s `[1.1.0]`-`[1.3.0]` entries for
the full detail. The full 8-scheme Curated cart tier
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
(`docs/performance.md`). Phase 8's RetroAchievements slice is real:
`rusty2600-cheevos` vendors the `rcheevos` C library and wires a safe
`RaClient` into the frontend (`retroachievements` feature, off by default)
— per-frame achievement tracking, hardcore mode, and a RetroAchievements
menu all work; a dedicated achievement-list/login/toast UI is deferred
(`to-dos/phase-8-reach/sprint-2-ra-and-tas.md`, `T-0802-005`). **Phase 6's
accuracy battery (Layer 4) is now real** — `rusty2600-test-harness` goes
from unused scaffolding to a real battery: the shared `Sentinel`/
`run_cpu_until_sentinel` Layer 2 runner (both bundled Klaus oracles refactored
onto it, unchanged pass/fail behavior), a real `AccuracyScore`-gated
`tests/accuracy_battery.rs` (2/2, 100%, already CI-enforced via the existing
`--features test-roms` step), and a real tolerance-aware `SnapComparator`.
At the time (v0.8.0), still honestly deferred: a genuine
externally-oracled golden CPU trace log for `GoldenLogDiffer` and
TIA-timing test-ROM fixtures for the Layer 3 `run_until_complete` runner
(`to-dos/phase-6-accuracy-to-100/sprint-2-pass-gate.md`, `T-0602-006`/
`007`). **`T-0602-007` closed in v1.8.0** (see the "Current release" note
above); **`T-0602-006` stays a permanent stub** — no legally
redistributable TIA/RIOT test-ROM corpus exists. **A real accuracy bug is fixed
in v0.9.0**: `T-0601-008` (Pitfall II never leaving its boot-time RIOT-timer
wait loop) is resolved — the timer's post-underflow (divide-by-1) decrement
rate never reverted to the normal prescale after an `INTIM` read (only a
fresh `TIMxT` write cleared it), confirmed against Stella's
`M6532::peek`/`updateEmulation` as the authoritative behavioral oracle.
Found via a rebuilt Gopher2600/Stella differential probe against the real
ROM. See `docs/riot.md` for the full writeup;
`screenshots/commercial/Pitfall II - Lost Caverns (USA).png` regenerated
(no longer a blank blue frame).

## Subsystem progress

| Crate | Chip | State |
|---|---|---|
| `rusty2600-cpu` | MOS 6507 | Documented + undocumented opcodes implemented; cycle-exact against both the trimmed and full SingleStepTests corpus, and Bruce Clark's exhaustive decimal-mode test (all ERROR=0). Split into `status.rs`/`bus.rs`/`cpu.rs` + a thin `lib.rs` (v0.2.0, `T-0601-006`) — the crate previously also carried a second, entirely dead, never-compiled RustyNES-lineage CPU implementation (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`, ~3,560 lines, no `mod` declarations ever wired them in) plus stale NES-flavored comment prose in the one live file; both fully resolved, see `docs/cpu.md`. |
| `rusty2600-tia` | TIA — video + audio | Beam-raced video (RESPx/HMOVE/playfield/players/missiles/ball/collisions) + audio poly-counter synthesis implemented and unit-tested. RIOT-timer-adjacent edge cases, AUDC 0xA/0xB pinning, TIA-revision modeling, and power-on RAM determinism are open (v0.2.0). |
| `rusty2600-riot` | MOS 6532 RIOT | RAM/DDR ports/timer implemented and unit-tested (prescale, underflow, INSTAT, read-after-write). `T-0601-008` fixed (v0.9.0): reading `INTIM` now reverts the post-underflow (divide-by-1) decrement rate back to the normal prescale (confirmed against Stella's `M6532::peek`/`updateEmulation`), matching real 6532 silicon — see `docs/riot.md`. |
| `rusty2600-cart` | Bankswitch boards | All 8 Curated schemes (2K, 4K, F8, F6, F4, CV, FA/CBS-RAM, Superchip, DPC, E7) implemented and wired into `detect()` (v0.3.0). BestEffort (v0.4.0-v0.6.0): F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB, X07 implemented and wired. `v1.5.0` added `Bank4A50` (`T-0402-014`): three independently relocatable ROM/RAM segments (2K/1.5K/256B) plus a fixed 256B trailer, a previous-access-gated hotspot state machine ported from Stella's `Cartridge4A50::checkBankSwitch`, wired into `detect()`'s 64K/128K branches via `is_probably_4a50()` — **23 of 25 catalogued schemes total**. Two hooks, `Board::snoop_write`/`snoop_read` (`crates/rusty2600-core/src/bus.rs`), let boards react to accesses the console routes to TIA/RIOT space — needed for 3F/3E's `$3E`/`$3F` write hotspots, UA/0840/X07's read+write hotspots, FE's `$01FE` stack-frame-value pickup, SB's address-low-bits bank select, and 4A50's whole previous-access state machine (both below `$1000` via `snoop_read`/`snoop_write` AND a smaller in-window instance at `$1F00-$1FFF`). Unchanged in v1.6.0 — no `Board`/`Cartridge` variant yet consumes `rusty2600-thumb` (see below); DPC+/CDF/CDFJ/CDFJ+ wiring is the `v1.6.x` patch train. Only AR/Supercharger (`T-0402-015`, deferred per `[1.5.0]`) and DPC+/CDF/CDFJ/CDFJ+ (`T-0401-006`) remain to close the catalogue. |
| `rusty2600-thumb` | ARM7TDMI Thumb-1 interpreter | New in v1.6.0: a real Thumb-1 interpreter ported from Gopher2600's Go implementation (not Stella's C++ `Thumbulator`) — registers, N/Z/C/V status flags, a generic `ThumbMemory` trait seam, an approximate N/S/I cycle + MAM prefetch-latch timing model (ported faithfully, documented as an approximation matching the reference's own admitted uncertainty), and all 19 Thumb-1 instruction-format classes. 27 hand-authored conformance tests. `no_std + alloc`, `#![forbid(unsafe_code)]`, zero dependency on any other Rusty2600 crate. **Not yet wired into any `Board`** — see `docs/thumb.md` for the full scope and the `v1.6.x` wiring plan. |
| `rusty2600-core` | Bus + scheduler + save-states + movies | lockstep loop + seeded phase live; bus decode complete. `save_state` (v1.1.0, ADR 0007) wraps the already-`serde`-derived `System` in a versioned header (magic, format version, caller-supplied `rom_tag`), encoded via `postcard`; the frontend's rewind ring reuses the same encoding. v1.3.0 added `Bus::write_log` (`WriteLog`, `#[serde(skip)]`): an optional, capped, scanline/color-clock-tagged per-write log for the debugger's Events panel and (indirectly) the watch engine. **v1.7.0 adds `movie`**: the `.r26m` TAS format (`Movie`/`MovieFrame`/`MovieRegion`/`MovieError`, magic `"R26M"`), mirroring `save_state.rs`'s header conventions without a shared dependency beyond reusing `SaveState` encoding for embedded branch points — see `docs/movie.md`. |
| `rusty2600-frontend` | egui shell | Rendering, audio, pacing, input, WASM support, the emu-thread path, the real debugger (`debug-hooks`, default-on), and RetroAchievements (`retroachievements`, off by default) all real and tested (v0.5.0-v0.7.0). v1.1.0 fixed three real frontend bugs: the `emu-thread` present path's dead-black-buffer fallback (rapid flicker), the blit shader's full-texture UV sampling instead of the active sub-rect, and Settings-window changes never reaching disk. v1.2.0 added `runahead` (off by default, `0..=4` frames, live via a Settings slider). v1.3.0 added debugger depth: `debugger::expr`/`watch_panel`, `callstack`, `event_panel`, `pmb_panel`, and `cheevos_panel` (`T-0802-005`, DONE). v1.4.0 added `shader_pass` (the composable post-process stack `Gfx::present` chains after the base blit — `CrtScanline` + an honestly-labeled `CompositeArtifact` approximation, toggleable from Settings, empty-stack default byte-identical) and `sprite_pack` (`hd-pack` feature: the `(GRPx, NUSIZx)`-keyed replacement-bitmap data model + TOML manifest loader — live rendering splice honestly deferred pending a TIA object-ID mask, a genuine prerequisite not yet built). **v1.7.0 adds `debugger::tastudio_panel`** (a piano-roll input grid over `rusty2600_core::Movie`, jump-to-frame via the existing rewind ring, branch points as separate `.r26m` files — live per-frame auto-recording into `run_frame` honestly deferred, same partial-landing pattern as `sprite_pack`) plus two riders, `debugger::access_counter` (per-address write-count heatmap) and `debugger::memory_compare_panel` (byte-diff two memory snapshots). |
| `rusty2600-gfx-shaders` | shader sources | New in v1.4.0: `no_std`, zero-dependency-besides-serde WGSL source constants (`CompositeArtifact`, `CrtScanline`) + the `PassKind` selector enum, consumed by `rusty2600-frontend::shader_pass`'s wgpu orchestration. Both passes derive everything from `textureDimensions()`/`@builtin(position)` — no per-pass uniform buffers needed. |
| `rusty2600-cheevos` | RetroAchievements FFI | Vendors the `rcheevos` C library (MIT); safe `RaClient` wrapper adapted from RustyNES's own `rustynes-cheevos` (console-agnostic except the memory map + one console-ID constant). `ra_addr_to_riot` maps RA's flat address space directly onto the RIOT's 128 bytes of RAM. Native-only (`#![cfg(not(target_arch = "wasm32"))]`); 7 tests passing, including real FFI smoke tests (v0.7.0). |
| `rusty2600-test-harness` | accuracy oracle | Real as of v0.8.0: `Sentinel`/`run_cpu_until_sentinel` (the shared Layer 2 runner both bundled Klaus oracles now use), a real `AccuracyScore`-gated `tests/accuracy_battery.rs` (2/2, 100%), and a tolerance-aware `SnapComparator`. **`T-0602-007` closed (v1.8.0)**: `GoldenLogDiffer` now bundles a genuine externally-oracled golden CPU trace (`tests/golden/klaus_functional_test_gopher2600.trace`, 20,000 instructions captured from Gopher2600's `hardware/cpu` package) — `bundled()` reports `true`, `tests/golden_log_test.rs` confirms `first_divergence() == None` against Rusty2600's own CPU. `run_until_complete` (Layer 3, full-`System`) remains — and stays — a stub: `T-0602-006` is a permanent scope boundary, no freely-redistributable TIA/RIOT test-ROM corpus exists (researched again this release; see `docs/testing-strategy.md`). |

## Accuracy (per-suite pass counts)

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | test-harness (`--features test-roms`) | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | test-harness (`--features test-roms`) | 1 / 1 — wired v0.2.0; exhaustive 256×256×2-carry-in `ADC`/`SBC` decimal-mode sweep, `ERROR=0` (bit-exact) |
| SingleStepTests/`65x02` `6502` (trimmed: 20 cases/opcode) | cycle-exact audit | 4,660 / 4,660 cases, 233 / 233 opcodes |
| SingleStepTests/`65x02` `6502` (full corpus, ~10K cases/opcode) | cycle-exact audit | wired v0.2.0 — `.github/workflows/singlestep-full.yml`, weekly cron + manual dispatch (not per-push: ~700 MB download across 233 opcodes) |
| **Golden CPU trace (externally-oracled)** | test-harness (`--features test-roms`) | **1 / 1** — closed v1.8.0 (`T-0602-007`): 20,000 instructions vs. an independent Gopher2600 CPU trace, `first_divergence() == None` |
| TIA timing / draw ROMs | test-ROM corpus | permanently unavailable (`T-0602-006`) — no freely-redistributable corpus exists; see `docs/testing-strategy.md` |
| Stella regression corpus | test-ROM corpus | same as above (`T-0602-006`) |
| **Accuracy battery (AccuracyCoin-equivalent)** | battery | **2 / 2 (100%)** — stood up v0.8.0, `tests/accuracy_battery.rs`, CI-enforced via the existing `--features test-roms` step, ≥90% v1.0 threshold |
| **Workspace test suite** | `cargo test --workspace` | **238 / 238** (unchanged — the new golden-trace test is `--features test-roms`-gated) |
| **Workspace test suite (`--features test-roms`)** | `cargo test --workspace --features test-roms` | **242 / 242** (+1 vs. v1.7.0 for the new golden-trace test) |
| `rusty2600-frontend` (`--features hd-pack`) | `cargo test -p rusty2600-frontend --features hd-pack` | **70 / 70** (+3 sprite-pack loader tests; `hd-pack` off by default, not part of the two workspace-wide counts above) |

## Board / mapper matrix

Tiered (Core / Curated / BestEffort) under the honesty gate (ADR 0003) — a
BestEffort board never backs the accuracy oracle. Full catalogue (size / hotspot
/ RAM / coprocessor) in `docs/cart.md`.

| Tier | Count | Schemes | Accuracy-gated | Implemented |
|---|---|---|---|---|
| Core | 2 | 2K, 4K | yes | 2K, 4K |
| Curated | 8 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC), DPC, E7 | yes | all 8, all wired into `detect()` (`T-0401-009` closed the CV/Superchip/E7 same-size collisions via hotspot-pattern heuristics). DPC and E7 both reclassified from `docs/cart.md`'s original BestEffort listing (see its tier-totals note) |
| BestEffort | 15 | F0, FE, E0, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC+/CDF/CDFJ | **no** | F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB, X07, 4A50 (`T-0402-001..004`/`006`/`008..011`/`014`) — 13 of 15, all wired into `detect()`. Only AR (`T-0402-015`) and DPC+/CDF/CDFJ (`T-0401-006`, ARM-driven) remain, each deliberately deferred as a substantially larger, separately-scoped undertaking. |

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
