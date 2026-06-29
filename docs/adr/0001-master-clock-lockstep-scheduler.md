# ADR 0001 — Integer color-clock lockstep scheduler

## Status

Accepted.

## Context

Rusty2600 must reach cycle-exact accuracy (the Stella / Gopher2600 / ares bar)
without per-quirk patches. The Atari 2600 is the **purest beam-racing console**:
the TIA has no framebuffer, and the CPU program rewrites TIA registers
mid-scanline in lockstep with the electron beam to compose every picture. A
register write landing one color clock early or late visibly moves graphics
(RESPx positioning, HMOVE comb, mid-scanline scroll). Per
ref-docs/research-report.md §5, §10, §12.

The clock topology is rigidly integer: master = NTSC color clock 3.579545 MHz;
the 6507 CPU runs at exactly **color clock / 3**; a scanline is **228 color
clocks = 76 CPU cycles** exactly. There is no fractional relationship to model.
Per ref-docs/research-report.md §1.

## Decision

A single lockstep scheduler in `rusty2600-core` (`System`) advances the **TIA
color clock one unit per `tick_one_color_clock()`**; the 6507 CPU advances on
**every third** color clock; the RIOT timer and any cart coprocessor advance on
the CPU cycle. This is **lockstep, not catch-up** — so a mid-instruction TIA
write composes the very next dot without a special case.

This is the most direct port of RustyNES's "PPU is the master clock / Bus owns
everything mutable" model (research report §12, Option 1), structured with
Gopher2600-style narrow bus traits (`CpuBus`, `VideoBus`, `AudioBus`; Option 4).
The WSYNC beam-stall is modelled by the TIA owning an `rdy_stall` signal the
scheduler reads to freeze (only) the CPU while the color clock keeps running.

## Consequences

- No per-quirk hacks for mid-scanline events — RESPx, HMOVE, collisions, and
  WSYNC all fall out of the lockstep substrate.
- One global run loop; the divisor table (`docs/scheduler.md`) must be exact.
- The hottest loop runs at 3× the CPU rate (~3.58 MHz) — it must stay
  allocation-free (`docs/performance.md`).
- The 6507 has no interrupt lines, so the scheduler has no IRQ/NMI dispatch — one
  fewer moving part than the NES scheduler it ports from.
- Integer resolution is sufficient (see ADR 0002): the program cannot observe a
  sub-color-clock event, so no fractional timebase is needed for the 2600.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
