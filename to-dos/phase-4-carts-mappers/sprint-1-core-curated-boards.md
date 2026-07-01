# Sprint 4.1 — Core and Curated Boards

**Context:** Part of Phase 4 — Carts / Mappers.

## Tickets (`T-0401-NNN`)

- [x] `T-0401-001` (DONE): `detect()`'s 8 KiB branch defaults to `BankF8`; add
  hotspot-pattern + ROM-DB-assisted disambiguation from E0 / FE / 3F (all
  BestEffort) so an 8 KiB image isn't silently mis-detected. See
  `crates/rusty2600-cart/src/lib.rs::detect`.
- [ ] `T-0401-002`: Implement the E7 (M-Network) scheme in `detect()`.
- [ ] `T-0401-003`: Implement the Superchip variants (F8SC/F6SC/F4SC, +128 B
  RAM) in `detect()`.
- [ ] `T-0401-004`: Disambiguate 3F (Tigervision) / 3E (Boulder Dash) / 3E+
  from the other 8 KiB+ schemes in `detect()`.
- [ ] `T-0401-005`: Implement the DPC (Pitfall II) custom Display Processor
  Chip board via `Board::tick_coprocessor`.
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


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
