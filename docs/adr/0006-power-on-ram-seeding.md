# ADR 0006 — Power-on RAM/register state is seeded, not zero, not the OS RNG

## Status

Accepted.

## Context

Real Atari 2600 hardware powers on with **indeterminate** RIOT RAM (128 B,
the console's only general RAM) and CPU register (`A`/`X`/`Y` — reset does
not touch these on a real 6502/6507, only `S`/`P`/`PC`) contents, influenced
by capacitance, prior cartridge activity, and ambient conditions. Some
commercial games and homebrew depend on this being "random-looking" at boot
(a few even use it as a crude entropy source). `docs/riot.md` and
`docs/cpu.md` already asserted "power-on RAM/register values are randomized
from the seeded PRNG in the owning `System`, never the OS RNG" — but
investigating for this ADR found that claim was **false**: `Riot::new()`
hardcoded `ram: [0; 128]` and `Cpu::new()` hardcoded `a: 0, x: 0, y: 0`, with
no seed threaded to either. `System::new(seed)` only ever used the seed to
derive the CPU/color-clock phase alignment.

This directly conflicts with ADR 0004's determinism contract if resolved
naively: true hardware randomness would break "same seed + ROM + input ⇒
bit-identical output." Stella resolves the identical tension with a
`ramrandom=<seed>` option: deterministic **given a seed**, not truly random,
so replays/regression tests stay reproducible while still exercising
uninitialized-RAM-dependent code paths realistically.

## Decision

Seed RIOT RAM and CPU `A`/`X`/`Y` from the same `seed` `System::new` already
takes, via a small dependency-free SplitMix64 byte generator
(`rusty2600_core::scheduler::SplitMix64` — chosen only for being a tiny,
well-known-good bit mixer, not for cryptographic or statistical-quality
requirements). Same seed ⇒ byte-identical power-on RAM/registers; different
seeds ⇒ different values — exactly Stella's `ramrandom=<seed>` model, and
consistent with the phase-alignment seeding ADR 0004 already established.

**Supercharger/AR titles are a known future exception, not handled by this
ADR.** Supercharger cartridges load their program from an audio-tape-style
signal into RAM at runtime, and their loader logic is documented to depend
on the loaded RAM region actually being zeroed first (research report's
cartridge notes) — when the Supercharger board lands (BestEffort tier,
v0.4.x), it will need its own always-zero-init override for the RAM range
it manages, distinct from the general seeded-random default this ADR sets.
That override belongs with the Supercharger board implementation itself,
not here.

## Consequences

- `docs/riot.md`/`docs/cpu.md`'s existing claims about seeded power-on
  randomization are now true, closing a real doc-vs-code gap found while
  writing this ADR.
- Golden-log/regression tests that don't want RAM-dependent variance should
  pin an explicit seed (e.g. `0`) rather than relying on unspecified
  behavior — the seed is already a required `System::new` parameter, so no
  new API surface is needed.
- `Cpu::new()`/`Riot::new()` themselves stay simple, zero-initializing,
  seed-agnostic constructors (used directly by unit tests that don't go
  through `System::new`); the seeding happens in `System::new` by writing
  into the constructed `cpu`/`bus` afterward, so no constructor signatures
  changed and no existing test that builds a bare `Cpu`/`Riot` is affected.
- A future save-state/rewind system (`docs/frontend.md`) must serialize the
  *resulting* RAM/register bytes, not the seed alone plus a re-derivation —
  the seed only matters at cold power-on, not after any RAM write occurs.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
