# Phase 5, Sprint 1: Shell and Input

**Status:** Complete

**Goal:** Establish the `winit + wgpu + egui` present path, wire the core inputs, and populate the debugger panels.

## Tickets

- `T-0501-001` (DONE): Implement the beam-racing framebuffer accumulation in `emu_thread.rs`.
- `T-0501-002` (DONE): Inject NTSC, PAL, and SECAM palettes into `palette.rs`.
- `T-0501-003` (DONE): Wire the `InputState` from the frontend into the core's `RIOT` and `TIA` input ports before each `run_frame()`.
- `T-0501-004` (DONE): Populate the egui debugger panels in `shell.rs` with live 6507, TIA, RIOT, and memory views.
- `T-0501-005` (TODO): Confirm the exact `SWCHA` bit order (`Joystick::swcha_nibble` in `input.rs`) against a real `SWCHA` read-back test ROM before it is treated as pinned.
- `T-0501-006` (TODO): Pin `ConsoleSwitches::swchb` (`input.rs`) against a `SWCHB` read-back test ROM, then route it through the RIOT port-1 latch.
- `T-0501-007` (TODO): Fold the paddle fire buttons into `SWCHA` and honour the data-direction registers (`SWACNT`/`SWBCNT`) in `InputState::riot_ports` (`input.rs`) so output bits read back the last written value.
- `T-0501-008` (TODO): Wire `Frame::put_dot` (`present_buffer.rs`) to the TIA's emitted `(luma, chroma)` via `crate::palette`, replacing the raw-RGB stub.
- `T-0501-009` (TODO): Replace `present_buffer.rs`'s mutex-guarded most-recent slot with a lock-free 3-slot ring (back/ready/front + atomic index swap), matching the RustyNES shape, so the present path never blocks the emu-thread.
- `T-0501-010` (TODO): Widen `emu_thread.rs`'s `SharedInput` packed `AtomicU32` to also carry the analog paddle bytes (a second atomic).

## Technical Notes
We take a brief lock on the `EmuCore` during `app.rs`'s `render()` function to pull the `framebuffer` and read the hardware state. This lock is dropped before the `wgpu` texture upload and the `egui` pass, ensuring that the UI never holds the emulator lock.
