# ADR 0007 — Save-state format and versioning policy

## Status

Accepted.

## Context

`System` (and everything it owns — `Cpu`, `Bus`, `Tia`, `Riot`, the
`Cartridge` enum) already derives `serde::Serialize`/`Deserialize`
throughout, under `no_std` + `alloc`, and already compiles against the
`thumbv7em-none-eabihf` no_std CI gate. Unlike a project that has to avoid
`serde` in its core (e.g. to dodge a transitive dependency's `std`-only
`serde` feature), Rusty2600 paid that cost everywhere already — a save-state
system here is a thin wrapper around existing derives, not new serialization
infrastructure. `Tia::video_buffer`/`audio_buffer` are already
`#[serde(skip)]` (correct: they're rebuilt at the next frame boundary, never
meaningfully "restored").

A save-state format still needs an explicit compatibility policy, or a future
struct-field addition silently breaks old save files (or worse, silently
misinterprets their bytes) with no diagnostic.

## Decision

`rusty2600-core::save_state::SaveState` wraps a small header (`magic:
[u8; 4]`, `format_version: u16`, a caller-supplied opaque `rom_tag: u64`)
around a captured `System`, encoded via `postcard` (a compact,
`no_std`+`alloc`-native serde binary format — chosen over hand-rolling a
tagged-section encoder since `postcard` already solves that problem and adds
no `std` dependency).

Versioning policy (three tiers, by SemVer distance from the writing build):

- **Same MAJOR.MINOR**: a save-state must round-trip byte-identically.
  Anything that breaks this within a minor release is a bug, not a version
  bump.
- **Same MINOR, different PATCH**: additive-only. New `System`-owned fields
  must carry `#[serde(default)]` so an older save-state (missing the field)
  still decodes; `FORMAT_VERSION` does not need to change for this case.
- **Different MINOR/MAJOR**: best-effort. `FORMAT_VERSION` bumps, and
  `SaveState::decode`/`restore` return a typed `SaveStateError::
  UnsupportedFormat { file_version, min_supported }` rather than attempting a
  silent, possibly-wrong reinterpretation.

`rom_tag` is deliberately opaque to `rusty2600-core` — the crate doesn't
know or care how a ROM's identity is computed (a full SHA-256, a fast
FNV-1a, whatever a given frontend/tool prefers); it only checks the tag
matches on restore, so a save file can't silently load against the wrong
cartridge. `SaveStateError::RomMismatch` surfaces that case explicitly.

## Consequences

- Rewind (v1.1.0), run-ahead (v1.2.0), TAS movies (v1.7.0), and rollback
  netplay (v1.10.x) all consume this same `SaveState`/`System` snapshot
  primitive rather than each inventing their own — see `to-dos/ROADMAP.md`'s
  v1.1.0→v2.0.0 line for how each later feature builds on it.
- Every future `System`-owned field addition needs a `#[serde(default)]`
  (or an explicit `FORMAT_VERSION` bump if it can't be sensibly defaulted) —
  a review checklist item for any PR touching `Cpu`/`Bus`/`Tia`/`Riot`/
  `Cartridge`'s fields.
- A restored save-state's first presented frame needs one buffer-refill
  cycle before its video/audio output is valid, since `video_buffer`/
  `audio_buffer` are intentionally not part of the serialized state — callers
  that want to skip presenting that first stale frame should do so
  explicitly (mirrors the same restore/restore-quiet distinction other
  emulator save-state systems make).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
