# Rusty2600 — STATUS (single source of truth)

This file is authoritative for per-suite pass counts, the board matrix, and
version policy. Everything else defers to it. References:
`ref-docs/research-report.md` §11; `docs/testing-strategy.md`; `docs/cart.md`;
`docs/adr/0003`.

**Current release:** v1.10.0 "Rollback" — the tenth release of the
`v1.1.0 -> v2.0.0` RustyNES-parity line (see `to-dos/ROADMAP.md` for the
full plan and `CHANGELOG.md`'s `[1.10.0]` entry). Adds a new
`rusty2600-netplay` crate: 2-player rollback netplay wrapping the mature
`ggrs` (GGPO-style) rollback engine rather than reimplementing it from
scratch, over GGRS's own built-in UDP transport (direct-IP/LAN only this
release). Rollback's `resync()` loop reuses `[1.1.0]`'s `SaveState`
substrate directly — no new determinism infrastructure needed, per ADR
0004's own citation of save-states as "the basis for rewind, run-ahead,
and netplay rollback." Input-delay (2 frames) and max-prediction-window
(8 frames) defaults match GGPO convention exactly. A genuine
rollback-desync test (`ggrs::SyncTestSession` driving a real `System` with
a synthetic input-reactive ROM across varied two-player input,
`SyncTestSession` panicking on any checksum mismatch) proves the
save/restore/resimulate path is correct — validated for real by
deliberately reintroducing a bug and confirming the test catches it.
**This release lands the session crate only** — not yet wired into
`rusty2600-frontend` (no host/join-game menu, no live input capture); a
`v1.10.x` follow-up does that wiring plus STUN/hole-punch NAT traversal
and the WebRTC browser transport, the same pattern `rusty2600-thumb`
(`[1.6.0]`) and `rusty2600-script` (`[1.9.0]`) already established. See
`docs/netplay.md` for the full architecture and scope.

Earlier in the line: `v1.9.0 "Scriptable"` added a new
`rusty2600-script` crate: a real, tested Lua scripting engine (`mlua`
native backend, off by default) exposing a deliberately-smaller-than-
RustyNES `emu` table (`peek`/`poke`/`cpu`/`onFrame`/`setJoystick`/
`setConsoleSwitch`/`drawText`/`drawRect`/`drawPixel`/`pause`/`saveState`/
`loadState`) over a host-agnostic `ScriptBus` trait seam, gated by a
`WritesLocked` determinism lock (folds RetroAchievements hardcore mode
today; `.r26m` movie and rollback-netplay locks are documented, not
stubbed, future additions). **This release lands the engine only** — it
is **not yet wired into `rusty2600-frontend`** (no `scripting` feature
flag, no live `ScriptBus` implementation, no overlay compositing, no
`onFrame` hook tied to `EmuCore::run_frame`); a `v1.9.x` follow-up does
that wiring, the same pattern `rusty2600-thumb` (`[1.6.0]`) and `.r26m`
movies (`[1.7.0]`) already established for this project. A `piccolo`
wasm-fallback backend is also deferred (`piccolo` is materially less
mature than `mlua`; the `emu`/`ScriptBus` design is backend-agnostic so
this stays a scoped follow-up, not a rewrite). See `docs/scripting.md`
for the full architecture and scope.

Full release-by-release detail for `[1.1.0]` through `[1.8.0]` (save-states,
run-ahead, debugger depth, the shader stack, `Bank4A50`, the
`rusty2600-thumb` ARM interpreter, `.r26m` movies, and the golden CPU
trace) lives in `CHANGELOG.md` — not duplicated here to avoid this section
drifting out of sync with the authoritative per-release record as the
line grows. The full 8-scheme Curated cart tier
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
| `rusty2600-script` | Lua scripting engine | New in v1.9.0: `mlua` native backend (off by default), a deliberately-smaller-than-RustyNES `emu` table (`peek`/`poke`/`cpu`/`onFrame`/`setJoystick`/`setConsoleSwitch`/`drawText`/`drawRect`/`drawPixel`/`pause`/`saveState`/`loadState`) over a host-agnostic `ScriptBus` trait, gated by `WritesLocked` (folds RetroAchievements hardcore mode today; `.r26m` movie/netplay locks are documented future additions, not stub fields). 18 tests passing. `std`-only, `unsafe`-permitted (the one exception besides `rusty2600-cheevos`, both for C-FFI reasons). **Not yet wired into `rusty2600-frontend`** — no `scripting` feature flag, no live `ScriptBus` impl, no overlay compositing yet; see `docs/scripting.md` for the full scope and the `v1.9.x` wiring plan. A `piccolo` wasm-fallback backend is also deferred. |
| `rusty2600-netplay` | Rollback netplay | New in v1.10.0: 2-player rollback netplay wrapping `ggrs` (GGPO-style), not a from-scratch reimplementation — `resync()` reuses `[1.1.0]`'s `SaveState` substrate directly. `PortInput` (per-player, distinct from `MovieFrame`'s whole-machine packing) + `RustyConfig` (the `ggrs::Config` binding) + `RollbackSession` (wrapping `ggrs::P2PSession` over GGRS's own built-in UDP transport, direct-IP/LAN only). Input-delay=2/max-prediction-window=8 match GGPO convention. A genuine rollback-desync test (`ggrs::SyncTestSession` + a synthetic input-reactive ROM, validated by deliberately reintroducing a bug and confirming it's caught) proves the save/restore/resimulate path is correct. 6 tests passing. **Not yet wired into `rusty2600-frontend`** — no host/join-game menu, no live input capture; STUN/hole-punch NAT traversal and the WebRTC transport are also deferred to `v1.10.x`. See `docs/netplay.md` for the full scope. |

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
| **Workspace test suite** | `cargo test --workspace` | **262 / 262** (+6 vs. v1.9.0 for the new `rusty2600-netplay` rollback-desync + unit tests) |
| **Workspace test suite (`--features test-roms`)** | `cargo test --workspace --features test-roms` | **266 / 266** |
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
