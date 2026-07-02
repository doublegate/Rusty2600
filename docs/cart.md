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
| DPC (Pitfall II) | 10 KiB ROM | F8-style hotspots + custom Display Processor Chip | coprocessor | Curated |
| F0 | 64 KiB | $1FF0 increments / 16×4K | — | BestEffort |
| FE (Activision) | 8 KiB | $01FE/$01FF via JSR/RTS stack frame | — | BestEffort |
| E0 (Parker Bros) | 8 KiB | $1FE0–$1FF7 select four 1K slices | — | BestEffort |
| E7 (M-Network) | 16 KiB | $1FE0–$1FEB, eight 2K banks | +2 KiB RAM | Curated (not yet implemented — `T-0401-002`) |
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
| DPC+ | var | Harmony/Melody ARM emulates DPC+ | ARM coproc | BestEffort |
| CDF / CDFJ / CDFJ+ | var | Harmony/Melody ARM | ARM coproc | BestEffort |

Tier totals: **2 Core**, **8 Curated**, **15 BestEffort** (25 schemes). Per
ref-docs/research-report.md §8.1 — DPC and E7 were both originally classified
BestEffort there; reclassified Curated per `docs/STATUS.md`'s "v1.0.0 scope"
decision that the full 8-scheme Curated set (CV, F8, F6, F4, FA/CBS-RAM,
Superchip, DPC, E7) closes in v0.3.0 (`to-dos/ROADMAP.md`). DPC is
implemented and its tier is pinned by `crates/rusty2600-cart`'s own
`BankDpc::tier()`; E7 is reclassified ahead of its own implementation
(`T-0401-002`) so this table doesn't need touching again once it lands.

## Notes on the special carts

- **FE (Activision Robot Tank / Decathlon)** uses no dedicated hotspot — it
  watches the stack frame written during `JSR`/`RTS` ($01FE/$01FF), making bank
  selection depend on the call instruction. A known emulator gotcha; hence
  BestEffort.
- **DPC** is David Crane's custom **Display Processor Chip** in Pitfall II — a
  true coprocessor (two extra sound channels, music sequencing, a hardware RNG,
  graphics streaming). **Pitfall II was the only commercial game to use it.**
  Implemented (v0.3.0) as F8-style hotspot bankswitching + a memory-mapped
  register file (`$1000..=$107F`), not via `Board::tick`/`tick_coprocessor` —
  the RNG and the 8 data fetchers (graphics + level-generation reads) are
  register-decode-driven, clocked on CPU access rather than on an independent
  clock. The one deliberate residual: the "music mode" oscillator that would
  auto-advance data fetchers 5-7 for the cart's own analog audio-mixing
  hardware isn't implemented, since Rusty2600's audio bus is entirely
  TIA-owned with no cart-audio mixing path (see `BankDpc`'s doc comment in
  `crates/rusty2600-cart/src/lib.rs`).
- **DPC+ / CDF / CDFJ** are modern Harmony/Melody formats backed by an **ARM
  microcontroller** running ARM code alongside the 6507; faithful emulation needs
  an ARM-thumb interpreter, so these are deep BestEffort (a later, optional
  dependency — research report §13).

Per ref-docs/research-report.md §8.2.

## Detection

`detect()` resolves size-unambiguous schemes purely by length (2K → `Rom2K`,
4K → `Rom4K`, 10K → `BankDpc`, 12K → `BankFA`). For sizes with a real
same-catalogue collision — 2K/4K (`BankCV`), 8K/16K/32K (Superchip vs plain
F8/F6/F4), 8K (`BankE0`/`Bank3E`/`Bank3F`/`BankUA`/`BankFe`/`Bank0840` vs
plain F8), 16K (`BankE7` vs plain `BankF6`), 32K (`Bank3E`/`Bank3F` vs plain
F4), 64K (`Bank3E`/`Bank3F`/`Bank4A50`/`BankEF`/`BankX07` vs `BankF0`), 128K
(`Bank3E`/`BankDF`/`Bank3F`/`Bank4A50` vs `BankSb`), and 256K
(`Bank3E`/`BankBF`/`Bank3F` vs `BankSb`) — `detect()` runs hotspot-pattern
heuristics first
(`is_probably_cv`/`is_probably_superchip`/`is_probably_e7`/`is_probably_e0`/
`is_probably_3e`/`is_probably_3f`/`is_probably_ua`/`is_probably_fe`/
`is_probably_0840`/`is_probably_x07`/`is_probably_4a50`, ported from Stella's
`CartDetector.cxx`, plus `ef_family_tail_signature`/
`is_probably_ef_by_opcode` for the EF/DF/BF "CPUWIZ" family, checked in the
same priority order Stella itself uses at each size) and only falls back to
the more common plain scheme if none match — or, at 128K/256K, to `BankSb`
(Superbank), matching Stella's own chain, which defaults straight to SB at
these two sizes once 3E/DF/3F/4A50 are ruled out (`T-0401-009`,
`T-0402-001..004`, `T-0402-006`, `T-0402-008..011`, `T-0402-014`, all DONE).

FE (Activision SCABS/Decathlon/Robot Tank/Space Shuttle/Thwocker) is now
implemented (`BankFe`, `T-0402-006`) via `Board::snoop_read`/`snoop_write`'s
`val` parameter — unlike E0/3E/3F/UA/0840 (which only need the ACCESS
ADDRESS), FE needs the snooped VALUE too, since the bank is derived from the
value written to `$01FE`'s companion stack-frame byte, not the address
itself. SB and X07 (`T-0402-011`, DONE) are also address/value-snoop-based.

**4A50 is now implemented** (`Bank4A50`, `T-0402-014`, v1.5.0) — the most
stateful snoop-based scheme in the catalogue: three independently
relocatable ROM/RAM segments (`slice_low`/`slice_middle`/`slice_high`) plus
a previous-access-gated hotspot state machine, ported faithfully from
Stella's `Cartridge4A50::checkBankSwitch`. Unlike FE/SB/X07 (which only
react to accesses BELOW `$1000`), 4A50 also has its own smaller instance of
the same previous-access check INSIDE the cart window, at `$1F00-$1FFF`
(always the fixed last 256 B of ROM) — so both `Board::snoop_read`/
`snoop_write` AND the tail of `Board::cpu_read`/`cpu_write` participate in
its hotspot logic. `detect()` resolves it via `is_probably_4a50()` (ported
from Stella's `CartDetector::isProbably4A50`): either the scheme's own
namesake `$4A50` at the NMI vector (`$1FFA-$1FFB`, checked as the raw
image's relative-from-end bytes so it works pre-tiling), or a fallback
heuristic (the RESET vector points into the last page and its first
instruction there is a 3-byte absolute `NOP $6Exx`/`NOP $6Fxx`). Per
Stella's own doc comment, this scheme "hasn't been fully implemented, and
may never be" even there (missing hi-res helper functions and `$1E00`
page-wrap) — this port is an equally-scoped, faithful translation of
exactly what Stella itself implements, not a superset, and stays
`BestEffort` tier indefinitely (only one known test ROM exists for it).

**AR / Supercharger (`T-0402-015`, DONE)**: 6 KiB RAM (three 2 KiB banks) +
a synthesized 2 KiB dummy BIOS ROM bank, mapped as two independent 2 KiB
windows (`$1000..=$17FF`/`$1800..=$1FFF`) selected via a 5-bit
configuration byte at hotspot `$1FF8`. Ported from Stella's
`CartridgeAR`/`CartAR.cxx`, "fast-load" (ROM-image-only) mode only — the
separate audio-cassette "sound-load" mode (streaming a real BIOS dump
against decoded WAV/MP3 tape audio) is deliberately NOT ported; it needs
an audio-decoding pipeline this `no_std` crate has no business owning, and
every known AR ROM dump already exists in the fast-load BIN format.
`BankAr` detects via its images' distinctive size (one or more 8448-byte
loads back-to-back — never a power-of-2-KiB size, so unambiguous against
every other scheme in this catalogue). BestEffort tier.

The 5-distinct-access delayed-write protocol Stella tracks via a global
`M6502` counter is reconstructed here without any `Bus`/`Cpu` change: since
the `Bus` already calls exactly one of `Board::cpu_read`/`cpu_write`
(cart-window) or `snoop_read`/`snoop_write` (every other CPU access) for
EVERY memory access, `BankAr` increments its own counter from all four
hooks, exactly mirroring Stella's global count. The BIOS's multi-load
handoff (staging a bank-switch byte + start address into zero-page RIOT
RAM for the BIOS to read back) needed one small, genuinely reusable
architecture addition: `Board::take_oob_pokes()` (default empty), drained
by `Bus::cpu_read`/`cpu_write` into `Riot::ram` directly — the same
"out-of-band poke bypassing normal bus routing" primitive Stella's own
`System::pokeOob` provides, not an AR-specific hack.

The dummy BIOS's cosmetic post-exit accumulator value (arbitrary on real
hardware; Stella seeds it from its own RNG) is a fixed constant here
rather than any random source, per this project's determinism contract
(ADR 0004) — unlike `BankDpc`'s patent-described RNG, this byte is not
gameplay-load-bearing. The `fastscbios` user setting Stella exposes (skip
vs. show the tape-loading progress bars) has no settings-plumbing
equivalent in this crate; this port always shows them (authentic stock
behavior). Stella's `finalizeLoad`/`getImage()` ROM-image re-export
machinery is not ported — this crate's own `SaveState` format already
covers persistence uniformly across every board.

**DPC+ (`T-0401-006`, DONE)**: the first Harmony/Melody ARM coprocessor
family — a real ARM7TDMI Thumb-1 interpreter (`rusty2600-thumb`, landed
`[1.6.0]`) finally wired into a `Board`/`Cartridge` variant (`BankDpcPlus`).
Ported from Gopher2600's Go `hardware/memory/cartridge/dpcplus` package
(~1,700 lines), not Stella's C++ `CartDPCPlus.cxx` — matching this
project's established precedent for ARM-adjacent code (`docs/thumb.md`'s
own "why Gopher2600, not Stella" rationale). Image layout: `[3072B
driver][N*4096B banks][4096B data][1024B freq]`; bank count is derived from
the image size, not hardcoded (real carts almost always ship 6 banks =
32 KiB total). Detection is content-signature-gated (the literal ASCII
bytes `"DPC+"` occurring at least twice — the same signature Stella's own
`isProbablyDPCplus` uses), NOT size-based: a 6-bank DPC+ image is exactly
32 KiB, the SAME size several already-implemented schemes (F4/F4SC/3E/3F)
also use, so a bare size check would misdetect real F4/3E/3F ROMs.

The register window (`$1000..=$107F`) implements the RNG (Gopher2600's
`0x10adab1e` galois-style formula), 8 plain + 8 windowed + 8 fractional
data fetchers (the windowed-fetcher `isWindow()` check is ported as a
byte-wraparound comparison, not the naive "low between top/bottom" the DPC
patent describes — Gopher2600's own comment explains why the naive version
misses real DPC+ demo ROMs at the low=bottom=0 power-on state), FastFetch
(`LDA #immediate` operand redirection for addresses `< 0x28`), and the
`$5A` "CALLFUNCTION" register — the actual ARM entry point. A write of
`254`/`255` there synchronously builds a fresh `Arm7Tdmi` against this
board's own driver/custom/data/freq ROM+RAM segments (mapped at Gopher2600's
own Harmony-architecture addresses: Flash at `0x0000_0000`, SRAM at
`0x4000_0000`) and steps it to `StepOutcome::ProgramEnded` (a `BX`/`BLX`
back to the entry `LR`) or a defensive step-count safety cap (NOT real
hardware behavior — a guard against a runaway/buggy ROM). This exactly
matches Gopher2600's own `Run()` semantics for this call shape: it only
resets ARM registers when the prior yield was `YieldProgramEnded`, and this
board's CALLFUNCTION loop always runs to `YieldProgramEnded` before
returning to the 6507, so a fresh reset-and-run-to-completion each call is
behaviorally identical to Gopher2600's persistent-instance model here (DPC+
never uses the `YieldSyncWithVCS` mid-execution-resume path Gopher2600
itself says "DPC+ does not support"). Verified with a real hand-assembled
synthetic Thumb-1 program (not just register-decode tests) proving the ARM
actually executes and mutates data RAM via a genuine `STRB`.

One new, genuinely reusable `Board` hook: none needed here (unlike AR's
`take_oob_pokes()`) — the ARM entry point is a synchronous call from within
`cpu_write`, not a per-color-clock scheduler tick, so no `Bus`/scheduler
changes were needed at all.

**Honestly deferred, not silently dropped**: DPC+'s music-mode continuous-
time audio (the reference's `Step(clock)`-driven phase accumulator) is NOT
implemented — the register plumbing (`$05`, `$5D..=$5F`, `$75..=$77`)
round-trips correctly, but the waveform-index math always samples index 0,
so DPC+ music-mode audio is silent/incorrect on that one channel. This is a
`rusty2600-tia` audio-timing follow-up, not a cart-catalogue-breadth one.
Function-call service `2` ("copy value to fetcher pointer, N times") is
ported with Gopher2600's own address formula verbatim, including what looks
like a copy-paste artifact (`Hi` is ALSO advanced by the loop index, not
held constant, so it does not fill a contiguous block) — Stella can't
cross-check this specific service since it runs the real ARM driver code
rather than short-circuiting it in C++, so it's ported exactly rather than
silently "corrected" on a guess (see `BankDpcPlus`'s own doc comment and
its matching test for the worked addresses). CDF/CDFJ/CDFJ+ (the other
three Harmony/Melody families) remain their own future, separately-scoped
follow-up — closing the catalogue to 25/25 needs those too, and rushing
them alongside DPC+ risked landing something half-correct.

BestEffort tier for `BankDpcPlus`. `tests/mapper_tier_honesty.rs`'s oracle
set is correctly NOT extended — that gate is `Core`/`Curated`-only (exists
precisely to keep `BestEffort` boards OUT of the accuracy-oracle corpus),
confirmed by both this board and `BankAr` before it.

### `Board::snoop_write`/`snoop_read` — bankswitching outside the cart window

Several classic schemes bankswitch on accesses the CONSOLE thinks are plain
TIA/RIOT traffic, not cart accesses: 3F/3E (Tigervision) trigger on any
WRITE whose low byte is `$3F`/`$3E` (e.g. a zero-page `STA $3F`); UA on
`$220`/`$240`, 0840 on `$800`/`$840`, and X07 on its two hotspot patterns all
trigger on either a READ or a WRITE; FE on `$01FE` and SB on `$0800..=$0FFF`
both need the accessed VALUE (FE) or the address's own low bits (SB) too —
all deep in TIA/RIOT-mirrored space (`$0000..=$0FFF`), not `$1000+`. This
matches real hardware: a cartridge's edge connector is wired to every
address line, and what the console's own decode logic thinks an address
means has no bearing on what the cart chooses to react to.

`Board::snoop_write(addr, val)` and `Board::snoop_read(addr, val)` (both
default no-op) give boards a look at every access the Bus routes to
TIA/RIOT space, called from `Bus::cpu_write`/`Bus::cpu_read`
(`crates/rusty2600-core/src/bus.rs`) — `snoop_write` before the console
handles the write, `snoop_read` after the console computes the value it
would return (passed as `val`). Neither hook lets the board REDIRECT the
access; `Bank3F`/`Bank3E` only need `snoop_write`, `BankUA`/`Bank0840`/
`BankX07`/`BankSb` use both (their Stella originals bankswitch identically
whether the access is a peek or a poke), and `BankFe` uses `snoop_read`'s
`val` parameter to pick the bank from the JSR stack-frame byte's value.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
