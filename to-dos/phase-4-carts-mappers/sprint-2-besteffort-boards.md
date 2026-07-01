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
  Wired into `detect()` at 64 KiB as the LAST-RESORT default (matching
  Stella's own priority order at this size), now that EF also collides
  there (`T-0402-008`).
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
- [x] `T-0402-007` (DONE): Found a `clippy::large_stack_frames` failure
  while adding `BankF0`'s 64 KiB array inline in the `Cartridge` enum (an
  enum is sized to its largest variant, so a 64 KiB variant inflates every
  stack frame that moves a `Cartridge`/`Bus`/`System` by value). Fixed for
  `BankF0`/`Bank3F`/`Bank3E` by storing their ROM as `Vec<u8>` instead of a
  fixed array (this crate `forbid`s `unsafe`, so there's no zero-copy way
  to keep a large compile-time-sized array off the stack during
  construction either); applied the same pattern to `BankEF`/`BankDF`/
  `BankBF` (Batch 2) from the start.

## Batch 2 (SuperChip-RAM BestEffort variants) — `T-0402-NNN` (continued)

- [x] `T-0402-008` (DONE): Implement EF/EFSC (CPUWIZ) — 64 KiB ROM, 16×4K
  banks, direct-select hotspots `$1FE0-$1FEF` (unlike `BankF0`'s
  sequential-advance at the same size); EFSC adds the standard 128 B
  Superchip RAM overlay. Detected via a shared tail-signature check
  (`ef_family_tail_signature`, ported from Stella's `isProbablyEF`/`BF`/
  `DF`: newer carts store `"EFEF"`/`"EFSC"` in the last 8 bytes), with an
  opcode-pattern fallback (`is_probably_ef_by_opcode`) for older EF carts
  that predate the marker convention. Checked at 64 KiB after 3E/3F and
  before falling back to `BankF0` — matches Stella's relative priority
  among the schemes implemented here.
- [x] `T-0402-009` (DONE): Implement DF/DFSC (CPUWIZ) — 128 KiB ROM, 32×4K
  banks, direct-select hotspots `$1FC0-$1FDF`. Same tail-signature
  detection pattern as EF (`"DFDF"`/`"DFSC"`), checked after 3E and before
  3F at 128 KiB (matching Stella). No opcode fallback — DF is a newer
  format that Stella itself only detects via the tail signature.
- [x] `T-0402-010` (DONE): Implement BF/BFSC (CPUWIZ) — 256 KiB ROM, 64×4K
  banks, direct-select hotspots `$1F80-$1FBF`. Same tail-signature
  detection pattern as EF/DF (`"BFBF"`/`"BFSC"`), checked after 3E and
  before 3F at 256 KiB (matching Stella). `BankEF`/`BankDF`/`BankBF` share
  their read/write/hotspot logic via free functions (`ef_family_read`/
  `ef_family_write`/`ef_family_hotspot`) rather than three near-duplicate
  copies, since the three schemes differ only in size/bank-count/hotspot
  base.
- [ ] `T-0402-011` (deferred, same `snoop_read` blocker as `T-0402-006`):
  SB (Superbank), X07, and 4A50 — the remaining Batch 2 schemes — all
  snoop CPU READS of TIA/RIOT-mirrored addresses (`$0800-$0FFF` for SB,
  almost all of `$0000-$0FFF` for X07/4A50), not just writes. `detect()`
  currently returns `None` (not a guess) for any 128/256 KiB image without
  a 3E/3F/DF/BF signature, rather than silently misdetecting an SB image.

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
