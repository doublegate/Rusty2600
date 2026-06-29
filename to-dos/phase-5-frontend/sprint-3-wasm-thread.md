# Phase 5, Sprint 3: WASM and Threading

**Status:** Not Started

**Goal:** Build the `wasm32` target and enable the dedicated `emu-thread` feature for native builds.

## Tickets

- `T-53-001` (TODO): Guarantee that `Board` implements `Send`. Currently, dynamic trait objects for `Board` may lack the `Send` bound, preventing `System` from being transferred across threads.
- `T-53-002` (TODO): Wire the dedicated `emu-thread` run loop (which continuously drives the core and publishes frames to a lock-free triple buffer) and remove the synchronous `run_frame` blocking path.
- `T-53-003` (TODO): Implement the WebAssembly bootstrap in `wasm.rs`, mapping `requestAnimationFrame` and the Web Audio API.
- `T-53-004` (TODO): Configure the `trunk` build for the wasm target and ensure it runs in-browser.

## Technical Notes
The `wasm32` build has no access to standard threads. `cfg(target_arch = "wasm32")` must properly gate the native winit loops and the native thread implementations.
