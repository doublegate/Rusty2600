<!-- markdownlint-disable MD033 MD041 -->
<div align="center">

# Rusty2600

**A cycle-exact Atari 2600 / VCS emulator in Rust.**

</div>

Rusty2600 is a cycle-accurate Atari 2600 / Video Computer System emulator in Rust, architected at
the Mesen2 / ares / higan accuracy bar: a master-clock lockstep scheduler (the TIA color clock is
the timebase, the 6507 runs on every third clock, `WSYNC`/`RDY` beam-stalls the CPU), a Bus that
owns everything mutable, a one-directional `no_std + alloc` chip-crate graph, a hard determinism
contract, and test-ROM-is-spec. The frontend is pure Rust (`winit` + `wgpu` + `cpal` + `egui`).

> **Status: v0.9.0 "Hardening" — real chip cores, full Curated tier + broad BestEffort coverage,
> real debugger, real RetroAchievements backend, a real accuracy battery, and the project's last
> known accuracy residual fixed.** The 6507 (documented + undocumented opcodes), the TIA
> (beam-raced video + two-channel audio), and the RIOT are implemented and tested — cycle-exact
> against a trimmed SingleStepTests corpus (4,660 cases, 233 opcodes, 100% passing) and 151 tests
> passing workspace-wide (154 with `--features test-roms`, including a real `AccuracyScore`-gated
> accuracy battery at 2/2, 100%). All 8 Curated-tier bankswitch schemes plus 12 BestEffort schemes
> are implemented and wired into automatic detection
> (2K/4K/F8/F6/F4/CV/FA/Superchip/DPC/E7/F0/E0/3F/3E/EF/DF/BF/UA/0840/FE/SB/X07 — 22 of 25
> catalogued schemes). Only 4A50, AR/Supercharger, and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family
> remain, each deliberately deferred as a substantially larger undertaking. The frontend's
> `debug-hooks` debugger is real (live 6507/TIA/RIOT/memory panels, breakpoints/step/continue, a
> standalone disassembler) and default-on. `rusty2600-cheevos` vendors the RetroAchievements
> `rcheevos` C library and drives real per-frame achievement tracking, hardcore mode, and a
> RetroAchievements menu (off by default; no dedicated achievement-list UI yet). A RIOT-timer bug
> that stalled Pitfall II's boot forever is fixed (`docs/riot.md`). HD-pack remains an unwired
> stub. See `docs/STATUS.md` for the authoritative per-suite / per-chip state and
> `to-dos/ROADMAP.md` for the full plan through v1.0.0.

## Crates

| Crate | Chip / role |
|-------|-------------|
| `rusty2600-cpu` | MOS 6507 (the 6502 in a 28-pin package: A0–A12, no exposed NMI/IRQ pins) |
| `rusty2600-tia` | TIA (Television Interface Adaptor) — beam-raced **video AND audio** |
| `rusty2600-riot` | MOS 6532 RIOT — the console's only RAM (128 B) + I/O ports + interval timer |
| `rusty2600-cart` | Tiered bankswitch catalog behind an honesty gate — all 8 Curated schemes implemented (2K/4K/F8/F6/F4/CV/FA/Superchip/DPC/E7); 12 of the 15-scheme BestEffort catalogue implemented (F0/E0/3F/3E/EF/DF/BF/UA/0840/FE/SB/X07) — only 4A50/AR/DPC+/CDF remain, see `docs/cart.md` |
| `rusty2600-core` | the Bus + the master-clock lockstep scheduler (the tie crate) |
| `rusty2600-frontend` | the `winit + wgpu + cpal + egui` shell (binary `rusty2600`), including the real `debug-hooks` debugger |
| `rusty2600-cheevos` | native-only RetroAchievements integration — a safe wrapper around the vendored `rcheevos` C library |
| `rusty2600-test-harness` | the AccuracyCoin-equivalent oracle + the bankswitch-tier honesty gate |

### Feature flags (frontend; all additive / off by default)

| Feature | Effect |
|---------|--------|
| `debug-hooks` | the real debugger: live 6507/TIA/RIOT/memory panels, breakpoints/step/continue, a side-effect-free `Bus::peek`/`peek_range`, a standalone disassembler — **default-on** (v0.5.0) |
| `hd-pack` | output-only TIA tile-source export for a texture-replacement loader (unwired stub) |
| `retroachievements` | native-only RetroAchievements integration: real per-frame achievement tracking, hardcore mode, a RetroAchievements menu (v0.7.0). Vendors `rcheevos` via `rusty2600-cheevos`; compiles to a no-op on wasm32 |
| `emu-thread` | runs frame production on a dedicated thread, paced by the audio ring buffer's fill ratio — **default-on** |
| `help-tui` | the ratatui terminal help browser (`rusty2600 help --interactive`) — default-on |

## Build / test

```bash
cargo check --workspace
cargo build --release --workspace
cargo test --workspace
cargo test --workspace --features test-roms          # + the accuracy / test-ROM suites
cargo run --release -p rusty2600-frontend -- path/to/rom.a26
cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features  # no_std gate
```

On Linux the frontend needs the wgpu/winit/cpal system deps (CachyOS / Arch):

```bash
sudo pacman -S --needed libxkbcommon wayland alsa-lib systemd-libs
```

## Controls (defaults)

| Input | Key |
|-------|-----|
| Joystick (P0) D-pad | Arrow keys |
| Joystick (P0) fire | Z |
| Joystick (P1) D-pad | W A S D |
| Joystick (P1) fire | Q |
| Console: Select / Reset | F1 / F2 |
| Console: Color ↔ B&W | F3 |
| Console: Left / Right difficulty A↔B | F4 / F5 |
| Toggle debugger overlay | `` ` `` |
| Open ROM / Quit | F12 / Esc |

Paddles (INPT0–3, analog) bind to the mouse / gamepad axes. USB gamepads auto-bind to P0.

## License

Rusty2600 is dual-licensed under **MIT OR Apache-2.0**. See `LICENSE-MIT` and `LICENSE-APACHE`.

## Test ROMs & accuracy oracles

`tests/cpu-timing/singlestep-6502/` carries a trimmed SingleStepTests/`65x02` corpus (20
cases/opcode; `fetch-vectors.py` regenerates or extends it from the full ~10K-cases/opcode upstream
source). `tests/roms/` holds the Klaus2m5 functional test and ProcessorTests golden logs; commercial
ROMs for mapper/coprocessor validation are staged locally under `tests/roms/external/commercial/`
(gitignored — never committed) behind the `commercial-roms` feature; `screenshots/commercial/`
carries their gameplay screenshots the same way `screenshots/homebrew/` does for the free ROM
corpus. See `docs/testing-strategy.md` for the full methodology and `to-dos/ROADMAP.md` for the
phase/sprint plan.
