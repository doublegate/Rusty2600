<!-- markdownlint-disable MD033 MD041 -->
<div align="center">

# Rusty2600

**A cycle-accurate Atari 2600 / Video Computer System emulator in Rust.**

</div>

<p align="center">
  <a href="https://github.com/doublegate/Rusty2600/actions"><img src="https://github.com/doublegate/Rusty2600/workflows/CI/badge.svg" alt="Build Status"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License: MIT OR Apache-2.0"></a>
  <a href="https://github.com/doublegate/Rusty2600/releases"><img src="https://img.shields.io/badge/version-v1.4.0-blue.svg" alt="Version"></a>
  <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.96-orange.svg" alt="Rust: 1.96"></a>
  <br>
  <a href="#compatibility-and-accuracy"><img src="https://img.shields.io/badge/accuracy%20battery-100%25%20(2%2F2)-brightgreen.svg" alt="Accuracy battery"></a>
  <a href="#compatibility-and-accuracy"><img src="https://img.shields.io/badge/SingleStepTests-233%2F233%20opcodes-brightgreen.svg" alt="SingleStepTests"></a>
  <a href="https://doublegate.github.io/Rusty2600/"><img src="https://img.shields.io/badge/play-in%20browser-success.svg" alt="Try in browser"></a>
  <br>
  <a href="#platform-support"><img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Web-lightgrey.svg" alt="Platform"></a>
</p>

## Overview

**Rusty2600 is a cycle-accurate Atari 2600 (VCS) emulator written in pure
Rust.** It targets the Mesen2 / ares / higan accuracy bar: a master-clock
lockstep scheduler running at TIA-color-clock resolution (3.579545 MHz NTSC),
with the 6507 CPU advancing on every third clock and `WSYNC`/`RDY`
beam-stalls freezing the CPU mid-cycle while the rest of the machine keeps
running. It clears its bundled Klaus2m5 functional and decimal-mode oracles
bit-exact and passes the trimmed SingleStepTests `6502` corpus
**4,660/4,660 cases across all 233 opcodes**, with the full ~10K-case-per-opcode
corpus verified weekly in CI.

Beyond reference accuracy, Rusty2600 is a real emulation platform, not just a
timing-accurate core: **23 of the console's 25 catalogued bankswitch
schemes** (all 8 Curated-tier schemes plus 13 BestEffort schemes) wired into
automatic detection, a real **debugger** (live 6507/TIA/RIOT/memory panels,
breakpoints, a standalone disassembler), a real **RetroAchievements**
backend (`rcheevos`-vendored, per-frame achievement tracking and hardcore
mode), **save-states and rewind**, and a growing **accuracy battery**
(`rusty2600-test-harness`) gated in CI so a regression can never silently
ship. The frontend is pure Rust (`winit` + `wgpu` + `cpal` + `egui`) with
native binaries for Linux, macOS, and Windows, plus a WebAssembly build that
runs in the browser.

**[Try it in your browser](https://doublegate.github.io/Rusty2600/)** — no
install required.

---

## Why Rusty2600?

Rusty2600 combines **accuracy-first emulation** of one of the industry's
oldest and most idiosyncratic consoles with **modern tooling** and the
**safety guarantees of Rust**. The 2600 has no framebuffer of its own — the
TIA races the electron beam and the program itself is responsible for timing
every visible pixel — which makes cycle-exactness a *correctness*
requirement, not a nice-to-have: get the timing wrong and games don't just
look different, they visibly break.

**Key differentiators:**

- **Reference-grade timing** — an integer TIA-color-clock lockstep
  scheduler (not catch-up), the 6507 on every third clock, `WSYNC`/`RDY`
  beam-stalls modeled as a genuine sub-instruction freeze — see
  `docs/scheduler.md` and ADR 0001.
- **Determinism as a hard contract** — same seed, ROM, and input sequence
  yield a bit-identical framebuffer and audio (ADR 0004). This is what makes
  save-state round-trips and regression testing correct by construction, and
  what save-states, rewind, run-ahead, and the planned rollback-netplay work
  all build on.
- **An honesty gate, not a marketing number** — every bankswitch scheme is
  classified Core / Curated / BestEffort (ADR 0003); a BestEffort scheme
  can *never* silently back the accuracy oracle, enforced structurally by
  CI, not just documented.
- **Safe, modular Rust** — the chip stack (`rusty2600-cpu`, `-tia`, `-riot`,
  `-cart`, `-core`) is `no_std + alloc`, one-directional, and independently
  testable; the only `unsafe` lives behind the frontend/FFI boundary.

---

## Highlights

| Feature | Description |
|---|---|
| **Cycle-Accurate Core** | Integer TIA-color-clock lockstep scheduler; the 6507 (documented + undocumented opcodes) cycle-exact against SingleStepTests (233/233 opcodes) and Bruce Clark's exhaustive decimal-mode test |
| **23 of 25 Bankswitch Schemes** | 2K/4K/F8/F6/F4/CV/FA/Superchip/DPC/E7 (all 8 Curated-tier) + F0/E0/3F/3E/EF/DF/BF/UA/0840/FE/SB/X07/4A50 (13 BestEffort) — classified behind a CI-enforced Core/Curated/BestEffort honesty gate (ADR 0003) |
| **Real Debugger** | Live 6507/TIA/RIOT/memory panels, breakpoints/step/continue, a side-effect-free memory peek, a standalone disassembler — default-on |
| **Debugger Depth** *(v1.3.0)* | A watch/conditional-breakpoint expression engine, a live JSR/RTS call stack, a per-scanline TIA write-scatter viewer, and a player/missile/ball position panel |
| **RetroAchievements** | Native `rcheevos` integration: login, a live achievement list, leaderboards, rich presence, per-frame achievement tracking, hardcore mode, and a recent-unlocks toast list (off by default) |
| **Save-States + Rewind** *(v1.1.0)* | A versioned binary snapshot format (ADR 0007) reusing the core's own `serde` derives; a rewind ring built on the same format |
| **Run-Ahead** *(v1.2.0)* | Speculatively simulates a few frames ahead to hide a game's internal input lag, built on the save-state snapshot primitives — off by default, `0..=4` frames, adjustable live from Settings |
| **Shader Stack** *(v1.4.0)* | A composable post-process stack (`rusty2600-gfx-shaders`) — CRT scanline darkening and an honestly-labeled composite-artifact color-bleed approximation, toggleable from Settings; empty stack (the default) is byte-identical to the direct blit |
| **4A50 Bankswitching** *(v1.5.0)* | Three independently relocatable ROM/RAM segments plus a previous-access-gated hotspot state machine, ported faithfully from Stella's `Cartridge4A50` — BestEffort tier |
| **Accuracy Battery** | A real `AccuracyScore`-gated battery (`rusty2600-test-harness`), CI-enforced, growing honestly rather than claiming an inflated pass rate |
| **WebAssembly** | Runs in-browser via `wasm-winit` (full winit/wgpu/egui) or a lightweight `wasm-canvas` embed mode |
| **Pure Rust** | `winit` + `wgpu` + `cpal` + `egui` frontend; a safe `no_std + alloc` chip stack behind a one-directional crate graph |

Planned via the iterative `v1.x.0` line toward `v2.0.0` — see
[`to-dos/ROADMAP.md`](to-dos/ROADMAP.md) for the full plan, and
[`CHANGELOG.md`](CHANGELOG.md) for exactly what's shipped in each release:
closing the remaining 2 bankswitch schemes (AR/Supercharger, the ARM-driven
DPC+/CDF/CDFJ/CDFJ+ family), TAS movie tooling, Lua scripting, rollback
netplay, and Android/iOS builds. The sprite-pack data model
(`sprite_pack`, `hd-pack` feature) shipped in v1.4.0; its live rendering
splice awaits a TIA object-ID mask.

---

## Features

### Emulation core

- **6507 CPU** — documented and undocumented opcodes, cycle-exact against
  the full SingleStepTests corpus (weekly CI) and Bruce Clark's exhaustive
  ADC/SBC decimal-mode sweep (`ERROR=0`, bit-exact).
- **TIA** — beam-raced video (RESPx/HMOVE comb, playfield, players/missiles/
  ball, all 15 pairwise collision latches) and two-channel poly-counter audio
  synthesis, unit-tested against Stella/Gopher2600 as differential oracles.
- **RIOT** — the console's only 128 B of RAM, the DDR/I/O ports, and the
  interval timer (prescale, underflow/`INSTAT`, read-after-write, and the
  post-underflow decrement-rate reversion — see `docs/riot.md`).
- **Master-clock lockstep scheduler** — the Bus owns every chip; the 2600
  has no separate work RAM, so save-states/rewind only ever need to capture
  the CPU + Bus (TIA + RIOT + cart) state.

### Cartridges

All 8 Curated-tier schemes (2K, 4K, F8, F6, F4, CV, FA/CBS-RAM, Superchip,
DPC, E7) plus 13 of 15 BestEffort schemes (F0, E0, 3F, 3E, EF/EFSC, DF/DFSC,
BF/BFSC, UA, 0840, FE, SB, X07, 4A50) are implemented and wired into
automatic `detect()`. Two `Board` hooks (`snoop_write`/`snoop_read`) let a
scheme react to accesses the console routes to TIA/RIOT space, not just the
`$1000+` cart window — needed for the 3F/3E/UA/0840/FE/X07/SB/4A50 families
(4A50 also uses a smaller in-window instance of its hotspot state machine at
`$1F00-$1FFF`). Only AR/Supercharger and the ARM-driven DPC+/CDF/CDFJ/CDFJ+
family (which needs a full ARM7TDMI Thumb interpreter) remain, each
deliberately deferred as a substantially larger, separately-scoped
undertaking — see [`docs/cart.md`](docs/cart.md) for the full catalogue and
tiering.

### Modern features

- **Debugger** (`debug-hooks`, default-on) — live 6507/TIA/RIOT/memory
  panels, breakpoints/step/continue, a side-effect-free `Bus::peek`/
  `peek_range`, a standalone disassembler, a watch/conditional-breakpoint
  expression engine, a JSR/RTS call stack, a TIA write-scatter viewer, and a
  player/missile/ball position panel.
- **RetroAchievements** (`retroachievements`, off by default) —
  `rusty2600-cheevos` vendors the `rcheevos` C library; login, a live
  achievement list, leaderboards, rich presence, per-frame achievement
  tracking, hardcore mode, and a recent-unlocks toast list all work.
- **Save-states + rewind** — a versioned binary snapshot format
  (`rusty2600-core::save_state`, ADR 0007) built on the chip stack's
  existing `serde` derives; the rewind ring reuses the same encoding rather
  than paying a raw-clone's worst-case cost.
- **Run-ahead** — speculatively simulates a few frames ahead to hide a
  game's internal input lag, built on the save-state snapshot primitives.
- **WebAssembly** — `wasm-winit` (full winit + wgpu + egui) or a lightweight
  `wasm-canvas` embed mode; native-only features compile out automatically.

---

## Quick Start

### Download binaries

Grab the latest release for your platform from the
[Releases page](https://github.com/doublegate/Rusty2600/releases).

```bash
# Linux / macOS
tar xzf rusty2600-<version>-<target>.tar.gz
./rusty2600

# Windows (PowerShell)
Expand-Archive rusty2600-<version>-x86_64-pc-windows-msvc.zip
.\rusty2600.exe
```

### Build from source

```bash
# Clone the repository
git clone https://github.com/doublegate/Rusty2600.git
cd Rusty2600

# Build the workspace (release)
cargo build --release --workspace

# Run a ROM you legally own (or launch bare and use File -> Open ROM)
cargo run --release -p rusty2600-frontend -- path/to/rom.a26

# Optional: build with RetroAchievements (needs a C compiler for vendored rcheevos)
cargo build --release -p rusty2600-frontend --features retroachievements

# The no_std embedded-target gate (confirms the core stays no_std + alloc)
cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features
```

On Linux the frontend needs the wgpu/winit/cpal system dependencies
(CachyOS / Arch shown; substitute your distro's equivalents):

```bash
sudo pacman -S --needed libxkbcommon wayland alsa-lib systemd-libs
```

---

## Default Controls

| Input | Key |
|---|---|
| Joystick (P0) D-pad | Arrow keys |
| Joystick (P0) fire | Z |
| Joystick (P1) D-pad | W A S D |
| Joystick (P1) fire | Q |
| Console: Select / Reset | F1 / F2 |
| Console: Color ↔ B&W | F3 |
| Console: Left / Right difficulty A↔B | F4 / F5 |
| Toggle debugger overlay | `` ` `` |
| Open ROM / Quit | F12 / Esc |

Paddles (`INPT0`–`3`, analog) bind to the mouse / gamepad axes. USB gamepads
auto-bind to P0. All bindings live in `config.toml` (see
[`docs/frontend.md`](docs/frontend.md)); a key-rebind UI is planned.

---

## Architecture

Rusty2600 is a Cargo workspace with a strictly one-directional chip-crate
graph — no chip crate depends on another except `rusty2600-tia`, which reads
cart-mediated bus state via `rusty2600-cart`:

| Crate | Role |
|---|---|
| `rusty2600-cpu` | The MOS 6507 (a 6502 in a 28-pin package: A0–A12, no exposed NMI/IRQ pins) |
| `rusty2600-tia` | The TIA (Television Interface Adaptor) — beam-raced **video AND audio** |
| `rusty2600-riot` | The MOS 6532 RIOT — the console's only RAM (128 B) + I/O ports + interval timer |
| `rusty2600-cart` | The tiered bankswitch catalogue, gated by the ADR-0003 honesty marker |
| `rusty2600-core` | The Bus (owns every chip) + the master-clock lockstep scheduler + save-states |
| `rusty2600-frontend` | The `winit` + `wgpu` + `cpal` + `egui` shell (binary `rusty2600`), including the debugger |
| `rusty2600-cheevos` | Native-only RetroAchievements integration — a safe wrapper around vendored `rcheevos` |
| `rusty2600-gfx-shaders` | `no_std` WGSL post-process shader sources for the composable shader stack |
| `rusty2600-test-harness` | The accuracy oracle + the bankswitch-tier honesty gate |

The Bus owns everything mutable; the CPU borrows `&mut Bus` for the duration
of a step. The 2600 has no separate work RAM, so there's no WRAM field on
the Bus — the only RAM in the whole console lives in the RIOT's 128 bytes.
See [`docs/architecture.md`](docs/architecture.md) for the full picture and
[`docs/adr/`](docs/adr/) for the numbered architectural decisions (the
master-clock scheduler, the fractional-timebase question, the accuracy
honesty gate, the determinism contract, TIA revision modeling, power-on RAM
seeding, and save-state versioning).

---

## Compatibility and Accuracy

| Suite | Layer | Pass / Total |
|---|---|---|
| Klaus `6502_functional_test` | CPU oracle | 1 / 1 |
| Klaus `6502_decimal_test` (BCD) | CPU oracle | 1 / 1 — exhaustive 256×256×2-carry-in `ADC`/`SBC` sweep, bit-exact |
| SingleStepTests `6502` (trimmed) | cycle-exact audit | 4,660 / 4,660 cases, 233 / 233 opcodes |
| SingleStepTests `6502` (full corpus) | cycle-exact audit | weekly CI cron (~700 MB across 233 opcodes, not per-push) |
| **Accuracy battery** | `rusty2600-test-harness` | **2 / 2 (100%)**, CI-enforced, ≥90% v1.0 threshold — growing honestly as real test-ROM fixtures are sourced, not inflated |
| Workspace test suite | `cargo test --workspace` | 183 / 183 |

Every bankswitch scheme is classified **Core** (register-decode trivial,
always oracle-gated), **Curated** (a redistributable fixture + full test
coverage, oracle-gated), or **BestEffort** (reference-ported, register-decode
+ boot-smoke tested only — **never** allowed to back the accuracy oracle).
This isn't a documentation promise: `mapper_tier_honesty.rs` asserts the
invariant in CI on every push. See [`docs/STATUS.md`](docs/STATUS.md) for
the authoritative, currently-accurate per-suite numbers and board matrix,
and [`docs/testing-strategy.md`](docs/testing-strategy.md) for the full
layered methodology (unit tests → CPU golden-log → the accuracy battery →
tolerance-aware snapshot comparison → a commercial-ROM regression oracle
gated behind locally-supplied, never-committed ROM dumps).

Known accuracy work fixed along the way: a RIOT interval-timer bug that
permanently stalled Pitfall II's boot-time wait loop (found via a rebuilt
Gopher2600/Stella differential probe, confirmed against Stella's own
`M6532::peek`/`updateEmulation`) — see `docs/riot.md`.

---

## Performance

Rusty2600's per-frame workload is a fraction of a modern console's — a 2600
frame is ~262 scanlines × 228 color clocks with a handful of chips, not a
PPU/APU pipeline juggling thousands of tiles. Real Criterion baselines for
the CPU/TIA/RIOT/cart crates live in [`docs/performance.md`](docs/performance.md),
measured with `cargo bench` on a pinned dev host — treat the documented
numbers as deltas across changes, not absolute cross-machine guarantees.

---

## Platform Support

| Platform | Status |
|---|---|
| Linux (x86_64) | Native binary, CI-built every release |
| macOS (aarch64) | Native binary, CI-built every release |
| Windows (x86_64) | Native binary, CI-built every release |
| WebAssembly | Runs in-browser — [try it live](https://doublegate.github.io/Rusty2600/) |
| `no_std` embedded target | `rusty2600-core` builds for `thumbv7em-none-eabihf` with `--no-default-features` — a CI gate, not a shipped product |

---

## Documentation

| Doc | Covers |
|---|---|
| [`docs/STATUS.md`](docs/STATUS.md) | The single source of truth: current pass counts, board matrix, version policy |
| [`docs/architecture.md`](docs/architecture.md) | The Bus/scheduler design, the crate graph, the determinism contract |
| [`docs/scheduler.md`](docs/scheduler.md) | The master-clock lockstep scheduler in detail |
| [`docs/cpu.md`](docs/cpu.md) / [`docs/tia.md`](docs/tia.md) / [`docs/riot.md`](docs/riot.md) | Per-chip specs |
| [`docs/cart.md`](docs/cart.md) | The full bankswitch catalogue and Core/Curated/BestEffort tiering |
| [`docs/frontend.md`](docs/frontend.md) | The frontend crate's shape and behavior |
| [`docs/testing-strategy.md`](docs/testing-strategy.md) | The layered accuracy-oracle methodology |
| [`docs/performance.md`](docs/performance.md) | Measured Criterion baselines and profiling guidance |
| [`docs/compatibility.md`](docs/compatibility.md) | Per-game/board compatibility notes and open questions |
| [`docs/adr/`](docs/adr/) | Numbered architectural decision records |
| [`to-dos/ROADMAP.md`](to-dos/ROADMAP.md) | The full phase/sprint plan through `v2.0.0` |
| [`CHANGELOG.md`](CHANGELOG.md) | What actually shipped in each release |

---

## Contributing

Contributions are welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) for the
workflow, commit-message conventions, and the pre-tag quality gate
(`cargo fmt`, `cargo test --workspace`, `cargo clippy -- -D warnings`, the
`no_std` build). Never commit commercial ROMs; the `commercial-roms` feature
and `tests/roms/external/` exist precisely so local dumps stay local.

## License

Rusty2600 is dual-licensed under **MIT OR Apache-2.0**. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).

## Acknowledgments

Rusty2600's differential-oracle workflow leans on two open-source
references studied (never vendored — both are GPL-licensed, incompatible
with this project's MIT/Apache-2.0 dual license) for behavioral
cross-checking: [Stella](https://stella-emu.github.io/) (the canonical C++
Atari 2600 emulator) and [Gopher2600](https://github.com/JetSetIlly/Gopher2600)
(a Go implementation used as a headless differential-testing harness). Test
ROMs are drawn from Klaus Dormann's public-domain 6502 functional/decimal
test suites and the trimmed SingleStepTests `65x02` corpus.
