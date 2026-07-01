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
