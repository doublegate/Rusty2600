# MOS 6507 CPU — Rusty2600

References: `ref-docs/research-report.md` §4 (the 6507 deep-dive), §11 (the test
oracle); `docs/scheduler.md`; `docs/testing-strategy.md`;
`crates/rusty2600-cpu/src/lib.rs`. This doc is the SPEC, not history — update it
in the same PR as the code, and pin behaviour against the test ROMs first.

## What the 6507 is

The 6507 is a **MOS 6502 die in a 28-pin DIP**. The package drops the 6502's
A13–A15 pins (leaving **A0–A12, a 13-bit / 8 KiB address bus**) and **omits both
the IRQ and NMI pins** — those lines were sacrificed precisely to make room for
A12. The 6507 and 6502 "share the same underlying silicon layers and differ only
in the final metallisation layer," so **for emulation the 6507 core is a 6502
core**: same registers, same documented and undocumented NMOS instructions, same
NMOS decimal mode. Per ref-docs/research-report.md §4.

## Registers / state

The register file (`Cpu` in `rusty2600-cpu`):

| Field | Width | Notes |
|---|---|---|
| `a` | 8 | accumulator |
| `x`, `y` | 8 | index registers |
| `sp` | 8 | stack pointer; the `$0100` stack page maps into the RIOT's 128-byte RAM mirror |
| `pc` | 16 | program counter (only A0–A12 reach the bus) |
| `p` (`Status`) | 8 | `C Z I D B U V N`; bit 5 (`U`) reads 1, `B` exists only in the pushed copy |
| `cycles` | 64 | total CPU cycles since power-on (for the golden-log differ) |

Power-on register values (`A`/`X`/`Y` — reset does not touch these) are
randomized from the **seeded** PRNG in the owning `System` (determinism
contract, `docs/adr/0004`; the seeding mechanism itself is `docs/adr/0006`),
never the OS RNG.

## Consequences of the 6507 packaging

- **No hardware interrupts.** There is no IRQ/NMI dispatch path (see
  `docs/scheduler.md` §No interrupt dispatch). The `BRK` opcode and the RESET
  vector (`$FFFC/$FFFD`) still work; the IRQ/BRK vector (`$FFFE/$FFFF`) is read
  on `BRK` but never fired by hardware. The only timing mechanisms are polling
  the RIOT timer or `WSYNC`.

  **Known vestige (`T-0601-006`, targeted v0.2.0):** `crates/rusty2600-cpu/src/`
  (`cpu.rs`, `lib.rs`, `bus.rs`) still carries a substantial NES-lineage
  IRQ/NMI service-sequence + `irq_level`/`nmi_level`/`poll_nmi`/`poll_irq`
  surface (mapper-IRQ, APU frame-counter/DMC IRQ, PPU-driven NMI, branch-delays-
  IRQ microcode, referencing nesdev wiki / AccuracyCoin / TriCNES / Mesen2 —
  all NES concepts) inherited wholesale from the RustyNES port and never
  stripped for the 6507. It is confirmed **dead, not live**: `rusty2600-core`
  never overrides any of these trait methods, so every one resolves to the
  crate's own default impl (`false` / no-op), and the SingleStepTests audit
  (which doesn't exercise interrupts) already passes 100%. It costs no
  correctness today, but it contradicts this doc, is confusing to a reader,
  and is real removable weight — strip it in v0.2.0, keeping only the
  behavior that's genuinely universal 6502 timing regardless of interrupts
  (e.g. the taken-branch C3 dummy-PC read, which is real hardware behavior on
  any 6502/6507 and has nothing to do with IRQ polling).
- **Address mirroring is severe.** With only 13 address lines decoded, and the
  TIA/RIOT each decoding only a few low lines, the same registers appear at many
  mirror addresses, and the cart hotspots live in the same cramped map. The CPU
  masks every effective address to 13 bits (`addr & 0x1FFF`) before it reaches
  the bus; the bus owner does the sparse/mirrored decode (`docs/cart.md`,
  `docs/riot.md`, `docs/tia.md`).

## Decimal (BCD) mode is real and must be exact

Unlike the NES's 2A03 (which disables BCD), the 2600's 6507 has a **working
decimal mode**, and games use it. The `DECIMAL` status flag gates BCD `ADC`/`SBC`.
Bruce Clark's decimal test (bundled in the Klaus Dormann suite) is the oracle —
implement until it passes 0-diff. Per ref-docs/research-report.md §4, §11.

## Undocumented opcodes are a hard requirement

The full NMOS illegal opcode set must be implemented correctly:

- Stable: `LAX`, `SAX`, `SLO`, `RLA`, `SRE`, `RRA`, `DCP`, `ISC`, `ANC`, `ALR`,
  `ARR`, `AXS`/`SBX`.
- Unstable (magic-constant / analog behaviour): `AHX`, `SHX`, `SHY`, `TAS`,
  `LAS`, `XAA`.

2600 developers reportedly *avoided* relying on them (fearing a mask revision
could "fix" them and break shipped games), but the emulator must still implement
them: the ProcessorTests vectors exercise them, and edge ROMs use them. Treat
them as correctness, not optional. Per ref-docs/research-report.md §4.

## Cycle accuracy: per-cycle bus access

Each instruction's read/write bus cycles — **including the "dummy" reads on
page-cross and the dummy read-then-write on RMW instructions** — must land on the
exact CPU cycle, because the TIA advances 3 color clocks per CPU cycle and a
write to a TIA strobe register on the wrong cycle moves graphics. The hot path
(`Cpu::tick`) is allocation-free, no per-instruction `Box`/dyn. The
SingleStepTests/ProcessorTests `6502` set (NMOS, decimal active) is the per-
opcode, per-cycle golden — use it, **not** the `nes6502` set (which ignores
decimal). Per ref-docs/research-report.md §4, §11.

## Timing

The CPU advances on **every third** TIA color clock (master / 3 ≈ 1.193182 MHz),
offset by the seeded power-on phase. RDY (the TIA's `WSYNC` beam-stall) freezes
the CPU mid-step without advancing it. See `docs/scheduler.md` for the divisor
table and the run loop.

## Test oracle

| Suite | What it pins |
|---|---|
| Klaus Dormann `6502_functional_test` | all valid NMOS opcodes + addressing modes |
| Klaus Dormann `6502_decimal_test` (Bruce Clark) | BCD `ADC`/`SBC` correctness |
| SingleStepTests/ProcessorTests `6502` | per-opcode, per-cycle bus activity (incl. undocumented) |

The harness `GoldenLogDiffer` (`rusty2600-test-harness`) captures a
`(PC, A, X, Y, SP, P, cycle)` record per retired instruction and reports the
first divergence. See `docs/testing-strategy.md`.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
