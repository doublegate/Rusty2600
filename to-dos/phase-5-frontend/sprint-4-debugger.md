# Phase 5, Sprint 4: The Real Debugger (v0.5.0 "Inspector")

**Status:** Complete

**Goal:** Replace the `debug-hooks` stub (literal `TODO(impl-phase)` bodies in
`shell.rs`, unwired forward placeholder in `Cargo.toml`) with a real debugger:
live 6507/TIA/RIOT register views, a side-effect-free memory hex viewer, a
standalone disassembler, and breakpoints/step/continue.

Note on `T-0501-004`: sprint 1's ticket list marked this "DONE" when in fact
only the panel *scaffold* (window, panel selector, placeholder labels) existed
— the bodies were `TODO(impl-phase)` stubs reading nothing real. This sprint is
the actual closure of that work; treat this file, not `T-0501-004`, as the
source of truth for when the debugger became real.

## Tickets

- `T-0504-001` (DONE): `rusty2600-core::Bus::peek`/`peek_range` — side-effect-
  free reads for tooling. A real `cpu_read` can trigger bankswitch hotspots,
  RIOT's INTIM read-clears-underflow behavior, and cart `snoop_read` side
  effects; `peek`/`peek_range` read from a cloned `Bus` instead (`peek_range`
  clones once and reads every requested byte from that single clone, not once
  per byte — the difference between one clone per frame and one clone per
  displayed byte for a 256-byte memory panel).
- `T-0504-002` (DONE): `crates/rusty2600-frontend/src/debugger/disasm.rs` — a
  standalone, display-only 6502 disassembler (independent of the CPU crate's
  private opcode dispatch), covering the full documented NMOS instruction set;
  undocumented opcodes render as `.byte $xx`.
- `T-0504-003` (DONE): `crates/rusty2600-frontend/src/debugger/mod.rs` — the
  persistent `DebuggerState` (breakpoints, memory cursor), the structured
  `DebugSnapshot` (CPU/TIA/RIOT + disassembly + memory view), and the four
  panel-render functions (CPU/TIA/RIOT/memory), all lock-free per the shell's
  non-negotiable rule.
- `T-0504-004` (DONE): Wire `shell.rs`'s `render_debugger` to the new panel
  functions, translating `DebugAction::{Step,Continue}` into
  `MenuAction::{DebugStep,DebugContinue}` dispatched after the egui pass.
- `T-0504-005` (DONE): `app.rs` builds the `DebugSnapshot` under the existing
  brief emu lock, only when the overlay is open AND a ROM is loaded (no point
  paying the copy cost otherwise); dispatches `DebugStep` (one
  `step_instruction()`) and `DebugContinue` (run to a breakpoint or a
  1,000,000-instruction safety cap).
- `T-0504-006` (DONE): Move `debug-hooks` into `Cargo.toml`'s `default`
  feature list (same precedent as `emu-thread`'s placeholder-to-default-on
  transition) and confirm both the on and off builds compile
  (`--no-default-features --features wasm-winit,help-tui,emu-thread`).
- `T-0504-007` (DONE): Populate the four previously-empty `benches/*.rs`
  Criterion stubs (`cpu_bench.rs`/`tia_bench.rs`/`riot_bench.rs`/
  `cart_bench.rs`) and record real measured baselines in
  `docs/performance.md`.

## Technical notes

The memory panel and the disassembly window both read through
`Bus::peek_range` rather than per-byte `Bus::peek` calls — a single clone
per frame the overlay is open, not one clone per displayed byte. A
bankswitch hotspot hit mid-range is visible to subsequent bytes read from
that same clone (an honest reflection of "whatever the bank state currently
is", the same caveat any bank-switched-system memory viewer has — Stella's
own debugger included).

Not visually screenshot-tested in this environment: the native `winit +
wgpu` window requires a live display, and this session's sandbox had no
safe way to pop a window on the user's desktop without being disruptive.
Verified instead via: unit tests (`disasm.rs`'s 5 cases, `bus.rs`'s 2 peek/
peek_range side-effect tests), `cargo clippy --workspace --all-targets -- -D
warnings` clean, and successful compiles for native (`debug-hooks` on and
off) and `wasm32-unknown-unknown` (`--features wasm-winit,debug-hooks`).
