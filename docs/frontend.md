# Frontend — Rusty2600

References: `ref-docs/research-report.md` §13 (frontend dependencies), §9 (Javatari
as a portable/wasm reference); `docs/architecture.md` (fact 8); `docs/adr/0004`
(determinism). The `rusty2600-frontend` crate is owned by the frontend agent;
this doc describes the SHAPE it implements and the 2600-specific details.

## The always-on egui shell

`rusty2600-frontend` is `winit` + `wgpu` + `cpal` + `egui` — **not** a bare
window. egui runs **every frame**: a persistent menu bar (File / Emulation /
Tools / View / Debug / Help) + a status bar + a tabbed Settings window, with
toggleable CPU/TIA/RIOT/memory debugger panels layered on top.

The shell **never holds the emu lock inside the egui closure**: menu interactions
return a `MenuAction` that the app dispatches **after** the egui pass, and the
render branch copies the scanline buffer under a brief lock, drops it, then
presents. On native the emulator runs on a **dedicated thread** communicating via
an `Arc<Mutex<…>>` handle + lock-free shared input; the winit thread only does
UI + present. This is the only `std`/`unsafe`-carrying crate.

## 2600-specific display

- **Output geometry.** The TIA emits **160 visible pixels** per line over **192
  visible scanlines** (NTSC) — a roughly 160×192 active raster. Pixels are wide
  (the common presentation doubles them horizontally to ~320 for a ~4:3 look);
  the frontend owns the aspect correction. The beam-raced core hands up a composed
  scanline buffer, not a chip framebuffer (`docs/tia.md`).
- **Region palette.** The TIA's 7-bit colour value maps to RGB differently per
  region — **NTSC / PAL / SECAM** are distinct palettes (region data, not a build
  fork). The same value is yellowish on NTSC, gray on PAL, aqua on SECAM. See
  `docs/compatibility.md`.
- **Frame line budget.** NTSC 262 lines / PAL 312 lines drives the present cadence
  (≈60 Hz NTSC, ≈50 Hz PAL).

## The debugger (`debug-hooks`, v0.5.0)

Real as of v0.5.0 — `crates/rusty2600-frontend/src/debugger/` (default-on
feature, same precedent as `emu-thread`: turn it off with `--no-default-
features --features wasm-winit,help-tui,emu-thread` for a debugger-free
build). Structure mirrors the shell's non-negotiable rule: nothing under
`debugger/` ever touches the emu lock. `app.rs` builds a `DebugSnapshot`
(registers, TIA/RIOT state, disassembly, a memory-window byte slice) once
per frame **only while the overlay is open and a ROM is loaded**, under the
same brief lock the present path already takes; the panel-render functions
in `debugger/mod.rs` are pure functions over that snapshot.

- **6507 panel** — A/X/Y/S/PC/P register grid, Step (`step_instruction()`
  once) and Continue (run to a breakpoint or a 1,000,000-instruction safety
  cap) buttons, a breakpoint add/remove list, and a scrolling disassembly
  window starting at PC (`>` marks the current PC, `*` marks a breakpoint).
- **TIA panel** — beam position (scanline/color clock), P0/P1/M0/M1/BL
  positions + colors, the playfield/background colors, and the 15 pairwise
  collision latches.
- **RIOT panel** — `INTIM` + its prescale divisor, `SWCHA`/`SWCHB` pin state
  + DDRs.
- **Memory panel** — a 256-byte hex+ASCII viewer with quick-jump buttons for
  RIOT RAM (`$0080`) and the cart window (`$1000`), backed by
  `Bus::peek_range` (see below).
- **Disassembler** (`debugger/disasm.rs`) — a standalone, display-only 6502
  mnemonic table, independent of the CPU crate's private opcode dispatch.
  Undocumented opcodes render as `.byte $xx` rather than inventing a
  mnemonic.

**Side-effect-free reads.** A real `Bus::cpu_read` can trigger bankswitch
hotspots, RIOT's INTIM read-clears-underflow behavior, and cart
`snoop_read` side effects — none of which a debugger peek should ever
cause. `Bus::peek`/`Bus::peek_range` (`rusty2600-core::bus`) solve this by
reading from a full clone of the bus rather than the live one; `peek_range`
clones once and reads every requested byte from that single clone (not once
per byte), which is the difference between one clone per frame and one
clone per displayed byte for a 256-byte memory panel or a multi-instruction
disassembly window.

## RetroAchievements (`retroachievements`, v0.7.0)

`rusty2600-cheevos` vendors the RetroAchievements `rcheevos` C library (MIT)
and wraps it in a safe `RaClient`, native-only and off by default. Owned by
`crate::cheevos::CheevosState` on the winit/main thread — deliberately NOT
inside `EmuCore`: `RaClient` is `!Send`/`!Sync`, incompatible with
`EmuCore`'s `Send` requirement (needed by the default-on `emu-thread`
feature). Pumped once per frame under the same brief emu lock the present
path and the debug snapshot already take, peeking the bus via
`|addr| bus.peek(addr)` rather than holding the lock any longer.

- **Memory map.** The 2600's only mutable game-state RAM is the RIOT's 128
  bytes — RA's flat address space maps directly onto it
  (`rusty2600_cheevos::memory::ra_addr_to_riot`: RA `0x00..=0x7F` -> CPU bus
  `$0080..=$00FF`), far simpler than a typical console's split RAM/WRAM map.
- **Game identification.** ROM bytes hash via rcheevos' own generic
  whole-buffer MD5 path (`RC_CONSOLE_ATARI_2600`; no console-specific hash
  case exists for the 2600 in the vendored source, since it needs none of
  the header/region disambiguation some consoles do).
- **What's wired today:** client construction, ROM load/close ->
  `begin_load_game`/`unload_game`, per-frame `do_frame`/`idle` pumping,
  hardcore mode (Emulation -> RetroAchievements menu), and achievement-
  unlock/server events surfaced as status-bar text.
- **What's deferred:** a dedicated achievement-list panel, a login dialog,
  and a rich-presence/unlock-toast HUD — the backend fires real events
  today; these are the dedicated UI surfaces for them
  (`to-dos/phase-8-reach/sprint-2-ra-and-tas.md`, `T-0802-005`).

## 2600-specific input

- **Joystick** — the standard CX40: 4 directions + 1 fire, fed into RIOT `SWCHA`
  (P0/P1) and the TIA `INPT4`/`INPT5` trigger latches.
- **Paddles** — the CX30 analog paddles, read via the TIA `INPT0..3` dump-capacitor
  inputs (a per-paddle charge timer); two paddles per port.
- **Console switches** — Reset, Select, Color/B&W (`TV TYPE`), and the two
  difficulty switches, all on RIOT `SWCHB`. The frontend surfaces these as UI
  toggles (a real-panel affordance), since many games read them at boot.

## Save-states, rewind, run-ahead

All snapshot/restore and rate control live **in the frontend**, never in the core
synthesis (ADR 0004). Because the core is deterministic (seeded power-on phase,
no OS RNG), a snapshot is a full serialization of `System` state and restores
bit-identically — the basis for rewind, run-ahead, and (later) netplay rollback.

### Manual save-state slots (`v2.4.0`)

`File -> Save State` / `Load State` expose 8 numbered slots per ROM — the
ordinary "save my game" menu feature, built entirely on top of the rewind
ring's already-real `SaveState` format (`rusty2600-core`, ADR 0007); no new
wire format was needed.

- **ROM identity**: `EmuCore::load_rom` computes a 64-bit FNV-1a hash over
  the raw ROM bytes (`rom_tag`), stored for the session's lifetime. This is
  the same opaque `rom_tag: u64` `SaveState::capture`/`restore` already
  required — the frontend just had no caller supplying one until now.
- **On-disk layout**: `<platform-data-dir>/Rusty2600/saves/<rom_tag as
  16-digit lowercase hex>/slot_<N>.r26s` (`crate::config::save_slot_dir`/
  `save_slot_path`). Keying by `rom_tag` means two different ROMs' slots can
  never collide, and one game's whole save history is a single deletable/
  relocatable directory. `.r26s` matches the project's existing `.r26m`
  TAS-movie extension convention. Native-only — the wasm build has no
  filesystem save path yet (a later release's scope, alongside
  `localStorage`/IndexedDB persistence for Settings).
- **Menu status**: each slot's existence + last-modified timestamp is
  probed fresh every frame via plain filesystem `stat` calls
  (`SaveSlotInfo::probe`), deliberately AFTER the brief emu lock is
  dropped — the probe only needs the `rom_tag` copied out under the lock,
  never the emulator itself, so the File menu never adds emu-lock
  contention to show real per-slot info.
- **Load safety**: `SaveState::restore`'s existing `rom_tag` mismatch check
  means a slot file can never be silently loaded against the wrong
  cartridge; a missing, corrupt, or mismatched slot file surfaces a clear
  status-bar message instead of panicking or silently doing nothing.

## wasm

`wasm-winit` (default feature) and `wasm-canvas` (lightweight embed) both
build for `wasm32-unknown-unknown`. The Trunk project lives at
`crates/rusty2600-frontend/web/` (not a repo-root `web/`) — `trunk build
--release` from that directory produces `dist/`, which `.github/workflows/
pages.yml` deploys to GitHub Pages (demo at `/`, rustdoc at `/api/`) on every
push to `main`. `web/Trunk.toml` pins `public_url = "/Rusty2600/"` to match
where Pages actually serves this repo, and pins the `wasm_bindgen` CLI
version to match Cargo.lock's resolved library version (a mismatch fails the
build). The core's `no_std` + `alloc` chip stack cross-compiles independent
of any of this.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
