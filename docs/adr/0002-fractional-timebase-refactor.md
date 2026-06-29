# ADR 0002 — The fractional-timebase refactor (the future "v2.0")

## Status

Proposed — **deferred, and for the Atari 2600 most likely unnecessary.**

## Context

An integer "N master-ticks per CPU cycle" scheduler cannot represent
sub-color-clock bus phases (a φ1/φ2 access split, sub-cycle brackets). On some
systems (the NES's 2A03 DMA timing, the SNES) the hardest test ROMs need that
finer resolution, which is why RustyNES carries a future "Timebase" rewrite
(ADR 0002 there) collapsing the multi-counter substrate into one fractional
master clock with every-cycle bus access.

**The 2600 is different.** Its timing is rigidly integer: the CPU is exactly
color clock / 3, a scanline is exactly 228 color clocks = 76 CPU cycles, and —
critically — **the program already races the beam at color-clock granularity**.
There is no sub-color-clock event the software can observe or depend on, so an
integer color-clock timebase (ADR 0001) is not an approximation; it is the native
resolution of the machine. Per ref-docs/research-report.md §1, §5, §12.

## Decision

Stay on the **integer color-clock master clock** (ADR 0001). Do **not** build a
fractional timebase up front. Should a hard-tier residual ever surface that
genuinely needs sub-color-clock phase (none is anticipated — the open questions in
research report §14 are about RESPx/HMOVE edge cases and TIA revisions, which are
*modelling* questions resolvable within integer resolution, not *resolution*
questions), it is **documented and deferred to this refactor, never point-fixed**.

If this refactor is ever undertaken, it would be the one milestone expected to
break byte-identity / save-state compatibility, and the one-clock + every-cycle-
bus-access model must be designed up front.

Do **not** conflate "the master clock exists (the v0.1 scheduler)" with "this
future fractional refactor" — they are different milestones (the RustyNES
versioning trap). For the 2600, expect this ADR to remain **Proposed / unneeded**
indefinitely.

## Consequences

- The v0.1 integer scheduler is expected to carry the project to v1.0 and beyond
  without a timebase rewrite — a simpler, lower-risk path than the NES/SNES.
- Any genuinely sub-color-clock residual is parked here with a written
  justification rather than patched into the integer loop.
- No save-state-breaking rewrite is on the roadmap unless this ADR is ever
  promoted to Accepted (not anticipated).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
