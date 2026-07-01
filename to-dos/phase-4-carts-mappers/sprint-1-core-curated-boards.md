# Sprint 4.1 — Core and Curated Boards

**Context:** Part of Phase 4 — Carts / Mappers.

## Tickets (`T-0401-NNN`)

- [x] `T-0401-001` (DONE): `detect()`'s 8 KiB branch defaults to `BankF8`; add
  hotspot-pattern + ROM-DB-assisted disambiguation from E0 / FE / 3F (all
  BestEffort) so an 8 KiB image isn't silently mis-detected. See
  `crates/rusty2600-cart/src/lib.rs::detect`.
- [x] `T-0401-002` (DONE): Implement the E7 (M-Network) scheme — `BankE7`,
  the 16 KiB / 8×2K-bank configuration only (per `docs/cart.md`'s catalogue
  commitment), following Kevin Horton's description as cited in Stella's
  `CartE7.hxx`. **Not wired into `detect()`**: 16 KiB collides with plain
  F6, same class of ambiguity as Superchip; tracked under the `T-0401-009`
  umbrella below. 5 new unit tests.
- [x] `T-0401-003` (DONE): Implement the Superchip variants (F8SC/F6SC/F4SC,
  +128 B RAM overlay, write-low `$1000-$107F` / read-high `$1080-$10FF` per
  Stella's `CartF8`/`CartEnhanced`). Added `with_superchip()` builders on
  `BankF8`/`BankF6`/`BankF4` rather than new types (SC variants are ROM-size-
  identical to their plain counterparts, so Stella itself can't tell them
  apart by size either — only ROM-DB/MD5 lookup can). **Not wired into
  `detect()`** for the same reason; that's `T-0401-009`, tracked alongside the
  8 KiB F8/E0/FE/3F ambiguity ROM-DB ticket.
- [ ] `T-0401-004`: Disambiguate 3F (Tigervision) / 3E (Boulder Dash) / 3E+
  from the other 8 KiB+ schemes in `detect()`.
- [x] `T-0401-005` (DONE): Implement the DPC (Pitfall II) custom Display
  Processor Chip board — F8-style hotspot bankswitching + a memory-mapped
  register file (RNG, 8 data fetchers, `$1000-$107F`), NOT via
  `Board::tick`/`tick_coprocessor` (register-decode-driven, clocked on CPU
  access instead of an independent clock; see `BankDpc`'s doc comment).
  Verified against `docs/cart.md`'s notes plus a Gopher2600 differential
  probe: byte-identical PC control-flow through the first ~2,000 executed
  instructions of the real Pitfall II ROM. Found (not fixed, out of this
  ticket's scope) a boot-time RIOT-timer-wait-loop discrepancy — tracked as
  `T-0601-008`. `detect()` dispatches on the unambiguous 10 KiB size
  (`0x2800..=0x2900`, tolerating the trailing dump garbage real-world DPC
  dumps carry). 8 new unit tests in `crates/rusty2600-cart/src/lib.rs`.
- [ ] `T-0401-006`: Implement DPC+ detection in `detect()` (deep BestEffort;
  may need the ARM-thumb interpreter dependency).
- [ ] `T-0401-007`: Detect pirate / homebrew BMC (bank-multi-cart) schemes in
  `detect()`.
- [x] `T-0401-008` (DONE): Fix the `BankF8::tier()` Core-vs-Curated conflict —
  the cart crate briefly pinned F8 as `Core` pre-v0.1.1; `docs/cart.md` and
  the research report specify `Curated` (F8 is hotspot-driven; `Core` is
  reserved for the two schemes needing zero board-specific hotspot logic: 2K,
  4K). Pinned by `mapper_tier_honesty.rs`'s
  `core_tier_is_reserved_for_unbanked_schemes` test.
- [x] `T-0401-009` (DONE): ROM-DB-assisted disambiguation umbrella — CV vs
  plain 2K/4K, Superchip (F8SC/F6SC/F4SC) vs plain F8/F6/F4, and E7 vs plain
  F6 (both 16 KiB) were all same-size collisions blocking `detect()`'s
  automatic dispatch. Resolved with hotspot-pattern heuristics ported
  directly from Stella's `CartDetector.cxx` (`is_probably_cv`,
  `is_probably_superchip`, `is_probably_e7` in
  `crates/rusty2600-cart/src/lib.rs`) rather than an MD5 database — each
  checked before falling back to the more common plain scheme, so a real
  CV/Superchip/E7 image is never silently misdetected while a plain image
  still resolves correctly. Validated against a REAL commercial ROM, not
  just synthetic fixtures: `BurgerTime (USA).a26` (16 KiB) was previously
  misdetected as plain F6 (rendering an all-black `screenshots/commercial/`
  frame) and is now correctly identified as E7 — cross-checked against
  Stella's own `stella.pro` properties database, which lists "M Network" as
  the manufacturer. 6 new `detect()`-level unit tests (signature-present vs
  signature-absent for each of the three schemes, plus a positive RAM-write
  check confirming Superchip mode actually engages, not just that the right
  enum variant comes back).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
