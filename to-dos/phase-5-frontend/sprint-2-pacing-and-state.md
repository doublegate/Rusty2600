# Phase 5, Sprint 2: Pacing and State

**Status:** Done

**Goal:** Implement dynamic rate control, the audio ring buffer, and the determinism-driven save-state / rewind / run-ahead orchestration.

## Tickets

- `T-52-001` (DONE): Implement a lock-free SPSC (Single Producer, Single Consumer) queue in `audio_ring.rs` (borrowing from RustyNES).
- `T-52-002` (DONE): Wire `cpal` to consume the audio ring buffer.
- `T-52-003` (DONE): Implement dynamic rate control (modulating the emulated clock rate slightly to keep the audio buffer half-full).
- `T-52-004` (DONE): Implement `Clone` and `Serialize`/`Deserialize` across all state in `rusty2600-core` and its chips.
- `T-52-005` (DONE): Wire a ring buffer for keyframe states to support rewind and run-ahead.

## Technical Notes
Per ADR 0004, the core must remain entirely deterministic and unaware of wall-clock time. All pacing logic must reside here in the frontend. Save states must round-trip bit-identically.
