# Phase 5, Sprint 1: Shell and Input

**Status:** Complete

**Goal:** Establish the `winit + wgpu + egui` present path, wire the core inputs, and populate the debugger panels.

## Tickets

- `T-51-001` (DONE): Implement the beam-racing framebuffer accumulation in `emu_thread.rs`.
- `T-51-002` (DONE): Inject NTSC, PAL, and SECAM palettes into `palette.rs`.
- `T-51-003` (DONE): Wire the `InputState` from the frontend into the core's `RIOT` and `TIA` input ports before each `run_frame()`.
- `T-51-004` (TODO): Populate the egui debugger panels in `shell.rs` with live 6507, TIA, RIOT, and memory views.

## Technical Notes
We take a brief lock on the `EmuCore` during `app.rs`'s `render()` function to pull the `framebuffer` and read the hardware state. This lock is dropped before the `wgpu` texture upload and the `egui` pass, ensuring that the UI never holds the emulator lock.
