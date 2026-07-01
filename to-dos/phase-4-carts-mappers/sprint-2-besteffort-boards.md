# Sprint 4.2 — BestEffort Boards

**Context:** Part of Phase 4 — Carts / Mappers. Spans v0.4.0 "Breadth"
through v0.6.0 "Catalog" (`to-dos/ROADMAP.md`): the local 15-scheme
BestEffort catalogue (`docs/cart.md`), staged as a patch train batched by
family (mirrors Stella's own `Cart*.cxx` grouping) — 12 of 15 now land
(only 4A50/AR/DPC-family remain, `T-0402-014`/`015`/`T-0401-006`). Every
scheme here is BestEffort tier per ADR 0003 — register-decode +
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
- [x] `T-0402-006` (DONE — UA/0840 v0.4.1, FE v0.6.0): Extended `Board` with a
  `snoop_read(addr, val)` hook (default no-op) and wired `Bus::cpu_read`
  (`crates/rusty2600-core/src/bus.rs`) to call it AFTER computing the value
  TIA/RIOT would return for a non-cart-window address. Turned out simpler
  than first scoped: Stella's `CartridgeUA`/`Cartridge0840::peek()`
  overrides exist ONLY to OBSERVE the access and trigger the bankswitch
  side effect — they still return exactly what TIA/RIOT would have
  returned anyway, so no "redirect the read" capability was needed, just
  an observe-after-the-fact hook (the write-side mirror of
  `snoop_write`). Implemented `BankUA` (8 KiB, 2×4K banks, hotspots
  `$220`/`$240`, plus the Digivision `$2C0`/`$FB0` variant) and `Bank0840`
  (8 KiB, 2×4K banks, hotspots `$800`/`$840`) on top of it; both wired into
  `detect()` at 8 KiB via `is_probably_ua`/`is_probably_0840` (ported from
  Stella), checked after 3E/3F and before falling back to plain F8.
  `BankFe` (v0.6.0) closes the deferred half: 8 KiB, 2×4K banks, selected by
  the value written to `$01FD` (the low byte of a JSR's stack-pushed return
  address) the access immediately AFTER a touch to `$01FE` (the high byte) —
  `(val >> 5) ^ 0b111`, masked to the 2 available banks, matching Stella's
  `CartridgeFE::checkSwitchBank` exactly. Detected via
  `is_probably_fe` (5 known-title boot signatures, ported from Stella),
  guarded by `!is_probably_f8_signature` so a real F8 image is never
  misdetected, checked after UA and before 0840 at 8 KiB.
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
- [x] `T-0402-011` (DONE — v0.6.0): SB (Superbank) and X07, the address-only
  half of the remaining Batch 2 schemes. `BankSb` (128/256 KiB, 32/64×4K
  banks): any read OR write to `$0800..=$0FFF` selects the bank from the
  LOW BITS of the accessed address itself (`address & (bank_count - 1)`),
  not a fixed hotspot value — matches Stella's `CartridgeSB::
  checkSwitchBank` (modulo its outer address-mirroring pre-mask, an
  implementation detail of Stella's own paged-address model this crate's
  fully-decoded `Bus` has no equivalent for). Wired into `detect()` as the
  DEFAULT fallback at 128/256 KiB once 3E/DF/3F are ruled out, matching
  Stella's own chain exactly (it defaults straight to SB at these sizes).
  `BankX07` (64 KiB, 16×4K banks): a direct select (`address & 0x180F ==
  0x080D` picks address bits 4-7 as the bank) plus a secondary toggle
  active only while the current bank is 14 or 15 (`address & 0x1880 == 0`
  flips the bank's low bit via address bit 6) — matches Stella's
  `CartridgeX07::checkSwitchBank` exactly. Detected via `is_probably_x07`
  (6 known opcode encodings, ported from Stella), checked after EF and
  before falling back to `BankF0` at 64 KiB.
- [ ] `T-0402-014` (not started): 4A50 — up to 128 KiB, three independently
  relocatable ROM/RAM windows (`$1000-$17FF`, `$1800-$1DFF`, `$1E00-$1EFF`)
  plus a hotspot whose behavior depends on the PREVIOUS access's address
  AND value (Stella's `Cartridge4A50::checkBankSwitch` tracks `myLastData`/
  `myLastAddress` across calls). Substantially more state than SB/X07/FE;
  scoped as its own ticket rather than folded into the "quick" schemes
  above.
- [ ] `T-0402-015` (not started): AR (Supercharger) — 6 KiB RAM loaded from
  a tape/audio-encoded multiload format (Stella supports both a raw binary
  multiload format and an actual WAV waveform decoder), 3×2 KiB RAM banks.
  Architecturally unlike every other scheme in this catalogue (no fixed ROM
  image to bankswitch at all); needs its own loader path, not just a new
  `Board` impl.

## Batch 3 (DPC-family / fractional datafetchers) — not yet scheduled

DPC+, CDF/CDFJ, BUS — watch for the `DFxFRACINC` non-reinitialization
jitter bug Stella's changelog documents. All four need a full ARM7TDMI
Thumb interpreter (`T-0401-006`; Gopher2600's `arm.go`/`thumb.go`/
`thumb2*.go` is the reference implementation to study) before any of them
can be implemented — deliberately not attempted piecemeal.

## Batch 4 (ARM/peripheral-integrated) — not yet scheduled

ELF (ARM7TDMI Thumb bus-stuffing — same ARM interpreter dependency as
Batch 3 above), PlusROM (network-connected carts), Movie Cart, CompuMate,
GameLine. Supercharger/AR moved to `T-0402-015` above (tractable without an
ARM interpreter, just a different loader path).

## Batch 5 (multicarts / remaining) — not yet scheduled

The `2IN1`...`128IN1` wrapper variants, TVBoy, WF8, JANE, DevCard.

---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
