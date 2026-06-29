# ADR 0004 — The determinism contract

## Status

Accepted.

## Context

Save-states, regression goldens, TAS replay, and (later) netplay rollback all
require exact reproducibility: replaying the same inputs from the same start must
reproduce the run bit-for-bit. The beam-raced TIA has no framebuffer, so the
"output" being reproduced is the composed scanline buffer + the mixed audio
samples — both must be deterministic. Per ref-docs/research-report.md §2, §12.

The one place non-determinism could leak in is **power-on state**: real hardware
powers on with randomized CPU/color-clock phase alignment and uninitialized RAM.
That randomness must be modelled (some games depend on it) without becoming a
source of irreproducibility.

## Decision

**Same seed + ROM + input sequence ⇒ bit-identical scanline output + audio.**

- The per-power-on CPU/color-clock phase alignment is drawn from a **seeded**
  PRNG (the `System::new(seed)` `phase` field, 0/1/2), never the OS RNG.
  Uninitialized RAM (RIOT 128 B) and power-on register values are likewise seeded.
- **Reset preserves** the phase alignment; a cold power-cycle re-rolls it from
  the seed.
- No system time, thread scheduling, or OS RNG may enter the core synthesis.
- **Rate control and run-ahead live in the frontend** — a resampler stage /
  snapshot-restore orchestration — never in the core. The core stays a pure
  deterministic function of (seed, ROM, input).

## Consequences

- Save-state round-trip is a full `System` serialization that restores
  bit-identically — the basis for rewind, run-ahead, and netplay rollback.
- The golden-log differ and the snap comparator (`rusty2600-test-harness`) are
  stable across runs and machines.
- Contributors must not "optimize" with wall-clock or OS-RNG shortcuts in the
  core (this overrides micro-optimization — see `docs/performance.md`).
- The seeded power-on model still lets games that read uninitialized RAM behave
  realistically, without sacrificing reproducibility.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
