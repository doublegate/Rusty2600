# Sprint 4.2 — BestEffort Boards

**Context:** Part of Phase 4 — Carts / Mappers. Targets v0.4.0 "Breadth"
(`to-dos/ROADMAP.md`): the ~50-scheme BestEffort long tail, staged as a
patch train batched by family (mirrors Stella's own `Cart*.cxx` grouping).
Every scheme here is BestEffort tier per ADR 0003 — register-decode +
boot-smoke tested only, never accuracy-oracle-gated.

## Batch 1 (classic homebrew) — `T-0402-NNN`

- [x] `T-0402-001` (DONE): Implement F0 (Dynacom Megaboy) — 64 KiB ROM,
  16×4K banks, sequential-advance hotspot at `$1FF0` (wraps 15 -> 0; unlike
  every other F-series scheme the game can't jump to an arbitrary bank).
  Wired into `detect()` at 64 KiB (unambiguous — EF/EFSC/X07, also 64 KiB
  per `docs/cart.md`, aren't implemented yet).
- [x] `T-0402-002` (DONE): Implement E0 (Parker Bros) — 8 KiB ROM, four
  1 KiB segments; the first three are each independently selectable among
  8 banks (`$1FE0-$1FE7`/`$1FE8-$1FEF`/`$1FF0-$1FF7`), the fourth
  permanently fixed to the last bank. Wired into `detect()` at 8 KiB via
  `is_probably_e0` (ported from Stella's `CartDetector::isProbablyE0`),
  checked after Superchip and before falling back to plain F8 — matches
  Stella's own priority order at this size.
- [x] `T-0402-003` (DONE): Implement 3F (Tigervision) — variable-size ROM
  (a multiple of 2 KiB), bank selected by writing the desired bank number
  to ANY address whose low byte is `$3F` (not a `$1000+` cart-window
  hotspot — a plain `STA $3F` zero-page write triggers it). Required a new
  `Board::snoop_write` hook (see below) since the existing `cpu_write` is
  only called for cart-window addresses. Wired into `detect()` at 8/32 KiB
  via `is_probably_3f` (2+ occurrences of `STA $3F`, ported from Stella).
- [x] `T-0402-004` (DONE): Implement 3E (Tigervision + RAM, Boulder Dash) —
  `Bank3F` plus a `$3E` hotspot selecting a 1 KiB RAM bank into the low
  segment instead of ROM. Wired into `detect()` at 8/32 KiB via
  `is_probably_3e` (ported from Stella), checked before `is_probably_3f`
  (a 3E image also satisfies 3F's signature, so order matters).
- [x] `T-0402-005` (DONE): Extended `Board` with a `snoop_write(addr, val)`
  hook (default no-op) and wired `Bus::cpu_write`
  (`crates/rusty2600-core/src/bus.rs`) to call it for every write the
  console routes to TIA/RIOT space, BEFORE the console itself handles it —
  matching real hardware, where the cartridge edge connector is wired to
  every address line, not just A12. This unblocks 3F/3E now and UA/0840/FE
  later (`T-0402-006`).
- [ ] `T-0402-006` (deferred, needs `snoop_read` too): UA, 0840, and FE all
  bankswitch on accesses to TIA/RIOT-mirrored addresses like `snoop_write`
  targets, but ALL THREE also snoop on READS, not just writes (their
  Stella `peek()` overrides call the same `checkSwitchBank` reads do) — FE
  additionally uses the snooped VALUE itself (the JSR return-address byte
  sitting at `$01FE` mid-stack-push) to pick the bank, not just the access
  address. Needs a `snoop_read(addr) -> Option<u8>`-shaped hook (the board
  must be able to REDIRECT the read, not just observe it, since these
  hotspots overlap real RIOT RAM / TIA registers) — a bigger interface
  question than the write-only case `T-0402-005` solved cleanly. Scoped as
  its own ticket rather than rushed.
- [ ] `T-0402-007`: Found a `clippy::large_stack_frames` failure while
  adding `BankF0`'s 64 KiB array inline in the `Cartridge` enum (an enum is
  sized to its largest variant, so a 64 KiB variant inflates every stack
  frame that moves a `Cartridge`/`Bus`/`System` by value). Fixed for
  `BankF0`/`Bank3F`/`Bank3E` by storing their ROM as `Vec<u8>` instead of a
  fixed array (this crate `forbid`s `unsafe`, so there's no zero-copy way
  to keep a large compile-time-sized array off the stack during
  construction either). **Apply the same pattern to every future
  BestEffort board whose ROM can be large** (EF/BF/DF/X07/4A50 in later
  batches, some up to 256 KiB) — don't reintroduce the failure per-scheme.

## Batch 2 (SuperChip-RAM BestEffort variants) — not yet scheduled

EF/EFSC, BF/BFSC, DF/DFSC, SB, X07, 4A50 — see `to-dos/ROADMAP.md`'s v0.4.x
batch plan.

## Batch 3 (DPC-family / fractional datafetchers) — not yet scheduled

DPC+, CDF/CDFJ, BUS — watch for the `DFxFRACINC` non-reinitialization
jitter bug Stella's changelog documents.

## Batch 4 (ARM/peripheral-integrated) — not yet scheduled

ELF (ARM7TDMI Thumb bus-stuffing — Gopher2600's `arm.go`/`thumb.go` is the
reference), PlusROM, Movie Cart, Supercharger/AR, CompuMate, GameLine.

## Batch 5 (multicarts / remaining) — not yet scheduled

The `2IN1`...`128IN1` wrapper variants, TVBoy, WF8, JANE, DevCard.

---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
