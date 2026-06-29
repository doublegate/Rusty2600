# Compatibility — Rusty2600

References: `ref-docs/research-report.md` §5.2 (vertical timing), §5.9 (region
palette), §8 (boards); `docs/cart.md`; `docs/STATUS.md` (the authoritative board
matrix). Per-game / per-board notes; the live pass-count matrix lives in
`docs/STATUS.md`.

## Regions are data, not a build fork

NTSC / PAL / SECAM differ in master clock, line budget, and palette — all
parameterized, never `#[cfg]`-forked. Per ref-docs/research-report.md §5.2, §5.9.

| Region | Color clock | Frame lines | Visible lines | Palette |
|---|---|---|---|---|
| NTSC | 3.579545 MHz | 262 (3+37+192+30) | 192 | NTSC hue/luma |
| PAL | 3.546894 MHz | 312 (3+45+228+36) | 228 (flagged for test-ROM confirmation) | PAL hue/luma |
| SECAM | 3.546894 MHz | 312 | 228 | SECAM (8-colour; AUDC-off quirk) |

The PAL visible-line budget (192 vs 228) is an open question pinned for a PAL
test ROM (research report §14, item 6). NTSC's 192 visible is firm.

## Board tier matrix

The full scheme catalogue + per-scheme size/hotspot/RAM/coprocessor is in
`docs/cart.md`. Summary tiers (the honesty gate, ADR 0003):

| Tier | Count | Schemes | Accuracy-gated? |
|---|---|---|---|
| Core | 2 | 2K, 4K | yes |
| Curated | 6 | CV, F8, F6, F4, FA/CBS-RAM, Superchip (SC) | yes |
| BestEffort | 17 | F0, FE, E0, E7, 3F, 3E, UA, 0840, EF, BF, DF, SB, X07, 4A50, AR, DPC, DPC+/CDF/CDFJ | **no** |

## In-scope titles (initial)

The accuracy bar is exercised on Core/Curated-backed software: plain 2K/4K and
F8/F6/F4 carts (the bulk of the commercial library), Superchip and CBS RAM+ RAM
carts, and the standard joystick/paddle/console-switch peripheral set.
Out-of-scope for the initial cut (BestEffort or deferred): ARM-coprocessor carts
(DPC+/CDF/CDFJ), Supercharger (`AR`) tape loading, and exotic peripherals
(Trak-Ball, light guns, AtariVox). Per ref-docs/research-report.md §2.

## TIA revisions

Multiple TIA revisions (TIA-1A etc.) introduce subtle playfield/HMOVE/colour edge
cases; Gopher2600 models several. The default revision and whether to
parameterize is an open design decision (research report §14, item 3) — pin
against Stella + a test ROM when an edge case surfaces.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
