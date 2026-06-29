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

> **Status: v0.1.0 — compiling skeleton.** The workspace, the architecture, the docs, and the
> frontend shell are in place; the chip cores (6507 / TIA / RIOT / cart) are TODO-laden skeletons,
> so the emulator opens a window and presents a cleared frame while the per-chip models land. See
> `docs/STATUS.md` for the authoritative per-suite / per-chip state and `to-dos/ROADMAP.md` for the
> plan.

## Crates

| Crate | Chip / role |
|-------|-------------|
| `rusty2600-cpu` | MOS 6507 (the 6502 in a 28-pin package: A0–A12, no exposed NMI/IRQ pins) |
| `rusty2600-tia` | TIA (Television Interface Adaptor) — beam-raced **video AND audio** |
| `rusty2600-riot` | MOS 6532 RIOT — the console's only RAM (128 B) + I/O ports + interval timer |
| `rusty2600-cart` | Tiered bankswitch catalog (2K/4K/F8/F6/F4/E0/E7/FE/3F/FA/DPC…) behind an honesty gate |
| `rusty2600-core` | the Bus + the master-clock lockstep scheduler (the tie crate) |
| `rusty2600-frontend` | the `winit + wgpu + cpal + egui` shell (binary `rusty2600`) |
| `rusty2600-test-harness` | the AccuracyCoin-equivalent oracle + the bankswitch-tier honesty gate |

### Feature flags (frontend; all additive / off by default)

| Feature | Effect |
|---------|--------|
| `debug-hooks` | arms the core's run-loop trace/breakpoint/event hooks for the debugger panels |
| `hd-pack` | output-only TIA tile-source export for a texture-replacement loader |
| `retroachievements` | native-only RetroAchievements integration |
| `emu-thread` | runs frame production on a dedicated thread (off until `Board: Send` lands) |
| `help-tui` | the ratatui terminal help browser (`rusty2600 help --interactive`) |

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

## Test ROMs & Oracles Setup (2026-06-29)
The project has been seeded with commercial test ROMs (in `tests/roms/external`) spanning multiple mappers (Core, Curated, and BestEffort). Test suites including Klaus2m5 and ProcessorTests have been staged for 6502/6507 CPU golden log validation. Sprints and Phase documentation are available in `to-dos/`.
