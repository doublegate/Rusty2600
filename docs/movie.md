# TAS movies (`.r26m`) — Rusty2600

References: `docs/adr/0004-determinism-contract.md`; `docs/adr/0006-power-on-ram-seeding.md`;
`docs/adr/0007-save-state-versioning.md`; `to-dos/ROADMAP.md` (v1.7.0
"Chronicle"); `crates/rusty2600-core/src/movie.rs`;
`crates/rusty2600-frontend/src/debugger/tastudio_panel.rs`. This doc is the
SPEC, not history — update it in the same PR as the code.

## What a `.r26m` movie is

A `.r26m` movie is a **start point** plus a **per-frame input log**.
Replaying it against the ROM it was recorded against reproduces the exact
same run, by the same determinism contract save-states already rely on
(ADR 0004: same seed + ROM + input sequence ⇒ bit-identical output).

The format lives in `rusty2600-core::movie` (`no_std`-compatible, like the
rest of the crate) and deliberately mirrors `save_state.rs`'s structure and
header conventions — magic + format version + `rom_tag`, `postcard`-encoded,
a typed decode error — rather than inventing a parallel scheme. It is its
own format with its own magic (`"R26M"`, distinct from save-state's
`"R26S"`) and an independent version counter, since a movie and a
save-state answer different questions: a whole run vs. one instant.

## Start points

- **`MovieStart::PowerOn { seed }`** — a fresh power-on, exercising ADR
  0006's deterministic seeded RIOT RAM / CPU `A`/`X`/`Y`.
- **`MovieStart::FromSaveState(Vec<u8>)`** — an embedded blob in
  `SaveState::encode`'s existing wire format. **A branch point is exactly
  this**: `Movie::new_branch` captures the running `System` via
  `SaveState::capture` and starts a new movie from there — no parallel
  snapshot system, just the same encoding `save_state.rs` already
  established.

## `MovieFrame` — the per-frame record

One frame's worth of everything a 2600 controller/console panel can drive:

| Field | Meaning |
|---|---|
| `swcha` | Packed joystick directions, both ports — same nibble layout as the RIOT's `SWCHA` (`rusty2600-frontend::input::Joystick::swcha_nibble`: bits 7-4 = port 0 up/down/left/right, bits 3-0 = port 1). Active-low. |
| `joy_fire` | The two joystick fire buttons (TIA `INPT4`/`INPT5`, NOT part of `SWCHA`): bit 0 = port 0, bit 1 = port 1. Active-HIGH (bit set = pressed) — unlike `swcha`, this isn't a direct hardware port byte, so there's no need to mirror the active-low convention. |
| `swchb` | Packed console switches — same layout as the RIOT's `SWCHB` (`rusty2600-frontend::input::ConsoleSwitches::swchb`). |
| `paddle_pos` | The four paddles' pot positions (`0..=255`, the TIA `INPTx` dump-capacitor value). |
| `paddle_fire` | The four paddles' fire buttons, one bit each. |

Console switches are **per-frame fields, not header-level constants** —
Select/Reset/Color/Difficulty can all change mid-run on real hardware,
unlike a fixed NES controller that never needs this.

**Why not `rusty2600-frontend::input::InputState`?** `rusty2600-core`
cannot depend on the frontend crate — the crate graph is one-directional
(frontend depends on core, never the reverse; see `docs/architecture.md`).
`MovieFrame` is its own self-contained type; the frontend converts
`InputState <-> MovieFrame` (the packing/unpacking logic is a direct reuse
of `InputState::riot_ports()`/`fire_inputs()`'s existing conventions, not a
re-derivation of them). `Default` is hand-implemented (not derived):
`swcha`/`swchb` default to `0xFF` (idle, matching real hardware's
active-low pull-ups) — a naive all-zero derive would instead mean "every
direction and switch held down simultaneously."

`MovieRegion` (`Ntsc`/`Pal`/`Secam`) is a small, deliberate duplication of
`rusty2600-frontend::palette::Region` for the same one-directional-graph
reason — it's just a label carried for reference/playback-config purposes
here, not palette data, so a shared type isn't warranted.

## TAStudio-lite (`debugger/tastudio_panel.rs`)

A piano-roll input grid, deliberately scoped below RustyNES's own
~609-line TAStudio panel:

- Click-to-toggle editing per input bit against an in-progress `Movie`.
- Branch points saved as separate `.r26m` files via `Movie::new_branch`.
- Jump-to-frame is driven by the **existing rewind ring**
  (`EmuCore::snapshots`/`rewind`, shipped in `[1.1.0]`) — there is no
  separate "greenzone" structure to build.
- Explicitly out of scope: foreign movie-format import (no existing 2600
  movie format to import from) and branch-tree visualization (a flat list
  of saved branch files is enough for a first cut).

Riders (cheap, generic, address-space-agnostic — not specific to TAS
tooling, just landed alongside it since they're small):

- **`debugger/access_counter.rs`** — a per-address write-count tally
  sourced from the existing `WriteEvent` log (`[1.3.0]`'s
  `Bus::write_log`).
- **`debugger/memory_compare_panel.rs`** — a byte-diff between a captured
  baseline and live memory.

## What's honestly deferred

**Live per-frame recording is not wired into `EmuCore::run_frame`'s hot
path.** The format, the panel's state machine, and manual
jump-to-frame/save-branch actions are real and tested, but nothing
automatically appends a `MovieFrame` every frame the emulator runs yet —
the same honest-partial-landing call this project made for `[1.4.0]`'s
sprite-pack data model (shipped without its render splice, clearly
documented as deferred rather than rushed or silently skipped).

## Testing

Real, hand-authored tests mirroring `save_state.rs`'s bar: round-trip
encode/decode, rom-tag mismatch rejected, bad-magic rejected,
truncated-bytes rejected, a branch-point round-trip through an embedded
save-state, and recording/replaying a short sequence of mixed
joystick/paddle/switch input frames confirming exact reproduction. The
panel's own state-machine tests (recording a frame, toggling an input bit,
jumping to a frame index, saving a branch) live in `tastudio_panel.rs`.
