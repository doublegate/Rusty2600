# Cartridges and bankswitching — Rusty2600

References: `ref-docs/research-report.md` §8 (cartridges + tiering), §10.5
(breadth + honesty gating); `docs/adr/0003` (the honesty gate);
`docs/compatibility.md`; `crates/rusty2600-cart/src/lib.rs`. This doc is the
SPEC, not history — update it in the same PR as the code, and pin behaviour
against the test ROMs first (Stella is the behavioral oracle for mappers).

## Why bankswitching exists

The 6507 exposes only a **4 KiB cartridge window** (`$1000..=$1FFF` in the
decoded map). Anything larger than 4 KiB — and many "exactly 4 KiB plus extra RAM
or a coprocessor" carts — bankswitches by **hotspot addresses**: a read **or** a
write to a magic address selects a bank, adds on-cart RAM, or drives a
coprocessor. Per ref-docs/research-report.md §8.

## The `Board` API (matches the crate)

`rusty2600-cart::Board` is the trait every scheme implements:

```rust
trait Board {
    fn cpu_read(&mut self, addr: u16) -> u8;   // runs read-triggered hotspots too
    fn cpu_write(&mut self, addr: u16, val: u8); // write-triggered hotspots + on-cart RAM
    fn tier(&self) -> Tier;                      // the honesty marker
    fn tick(&mut self) {}                        // per-CPU-cycle coprocessor hook (default no-op)
    fn tick_coprocessor(&mut self) {}            // independently-clocked coproc (default no-op)
}
```

Both `cpu_read` and `cpu_write` must run the bank logic even when the access is
nominally a fetch (many schemes switch on reads). `detect(rom: &[u8]) ->
Option<Cartridge>` resolves the scheme (a closed enum over every implemented
`Board`, not a trait object — keeps `no_std` dispatch static); today it picks
the sized boards by length and stubs the rest (each unimplemented branch
carries its intended tier as a `T-0401-NNN` TODO).

## The tier model

`Tier { Core, Curated, BestEffort }` is an **honesty marker**, not a behavioural
one — runtime behaviour is identical regardless of tier. `Tier::is_accuracy_gated`
returns true for `Core`/`Curated` and false for `BestEffort`; the honesty gate
(`tests/mapper_tier_honesty.rs`, ADR 0003) forbids a `BestEffort` board ever
backing the accuracy oracle. Per ref-docs/research-report.md §8.3.

> Resolved (`T-0401-008`): the cart crate briefly pinned F8 as `Core`
> pre-v0.1.1; `BankF8::tier()` now returns `Curated`, matching this doc's
> catalogue and the research-report split. `Core` is reserved for the two
> schemes needing zero board-specific hotspot logic (2K, 4K); every
> hotspot-driven scheme, including F8/F6/F4, is `Curated`. Pinned by
> `crates/rusty2600-test-harness/tests/mapper_tier_honesty.rs`'s
> `core_tier_is_reserved_for_unbanked_schemes` test so it can't regress.

## Scheme catalogue (size / hotspot / RAM / coprocessor / tier)

| Scheme | Size | Hotspots / mechanism | RAM / coproc | Tier |
|---|---|---|---|---|
| 2K | 2 KiB | none (data repeats in the 4K window) | — | **Core** |
| 4K | 4 KiB | none (single bank) | — | **Core** |
| CV (Commavid) | 2 KiB | fixed bank + 1 KiB on-cart RAM | +1 KiB RAM | Curated |
| F8 | 8 KiB | $1FF8/$1FF9 select 2×4K | — | Curated |
| F6 | 16 KiB | $1FF6–$1FF9 select 4×4K | — | Curated |
| F4 | 32 KiB | $1FF4–$1FFB select 8×4K | — | Curated |
| FA / CBS RAM Plus | 12 KiB | $1FF8/9/A select 3×4K | +256 B RAM | Curated |
| F8SC / F6SC / F4SC (Superchip) | 8/16/32 KiB | F-series hotspots + Superchip | +128 B RAM | Curated |
| F0 | 64 KiB | $1FF0 increments / 16×4K | — | BestEffort |
| FE (Activision) | 8 KiB | $01FE/$01FF via JSR/RTS stack frame | — | BestEffort |
| E0 (Parker Bros) | 8 KiB | $1FE0–$1FF7 select four 1K slices | — | BestEffort |
| E7 (M-Network) | 16 KiB | $1FE0–$1FEB, eight 2K banks | +2 KiB RAM | BestEffort |
| 3F (Tigervision) | up to 512 KiB | `STA $3F` with A = bank (low 2K window) | — | BestEffort |
| 3E (Tigervision + RAM) | var | `STA $3E` RAM-bank / `STA $3F` ROM-bank | +RAM | BestEffort |
| UA (UA Ltd.) | 8 KiB | $0220/$0240 hotspots | — | BestEffort |
| 0840 (EconoBank) | 8 KiB | $0800/$0840 hotspots | — | BestEffort |
| EF / EFSC | 64 KiB | $1FE0–$1FEF, 16×4K (+SC RAM) | optional RAM | BestEffort |
| BF / BFSC | 256 KiB | CPUWIZ 64×4K (+SC RAM) | optional RAM | BestEffort |
| DF / DFSC | 128 KiB | CPUWIZ 32×4K (+SC RAM) | optional RAM | BestEffort |
| SB (Superbank) | 128–256 KiB | $0800–$083F | — | BestEffort |
| X07 | 64 KiB | AtariAge custom | — | BestEffort |
| 4A50 | up to 128 KiB | complex r/w hotspots + RAM | +RAM | BestEffort |
| AR (Supercharger) | 6 KiB RAM | tape/audio load, 3×2K RAM banks | RAM-based | BestEffort |
| DPC (Pitfall II) | 10 KiB ROM | custom Display Processor Chip | coprocessor | BestEffort |
| DPC+ | var | Harmony/Melody ARM emulates DPC+ | ARM coproc | BestEffort |
| CDF / CDFJ / CDFJ+ | var | Harmony/Melody ARM | ARM coproc | BestEffort |

Tier totals: **2 Core**, **6 Curated**, **17 BestEffort** (25 schemes). Per
ref-docs/research-report.md §8.1.

## Notes on the special carts

- **FE (Activision Robot Tank / Decathlon)** uses no dedicated hotspot — it
  watches the stack frame written during `JSR`/`RTS` ($01FE/$01FF), making bank
  selection depend on the call instruction. A known emulator gotcha; hence
  BestEffort.
- **DPC** is David Crane's custom **Display Processor Chip** in Pitfall II — a
  true coprocessor (two extra sound channels, music sequencing, a hardware RNG,
  graphics streaming). **Pitfall II was the only commercial game to use it.** It
  rides the `Board::tick` / `tick_coprocessor` hooks.
- **DPC+ / CDF / CDFJ** are modern Harmony/Melody formats backed by an **ARM
  microcontroller** running ARM code alongside the 6507; faithful emulation needs
  an ARM-thumb interpreter, so these are deep BestEffort (a later, optional
  dependency — research report §13).

Per ref-docs/research-report.md §8.2.

## Detection

`detect()` resolves the sized boards by length now (2K → `Rom2K`, 4K → `Rom4K`,
8K → `BankF8` Curated, 16K → `BankF6` Curated, 32K → `BankF4` Curated). The
full scheme set needs hotspot-pattern + ROM-DB-assisted detection (8 KiB alone
is ambiguous between F8, E0, FE, and 3F). The tiered TODOs (`T-0401-001..007`)
track each board family; the honesty gate's oracle set must be extended in
lockstep so the pass-rate stays truthful as boards land.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
