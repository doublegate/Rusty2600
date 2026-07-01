# MOS 6532 RIOT — Rusty2600

References: `ref-docs/research-report.md` §7 (the RIOT deep-dive);
`docs/architecture.md`; `crates/rusty2600-riot/src/lib.rs`. This doc is the SPEC,
not history — update it in the same PR as the code, and pin behaviour against the
test ROMs first.

## What the RIOT is

The RIOT (RAM-I/O-Timer; called "PIA" in the *Stella Programmer's Guide*) is a
standard **MOS 6532**, **independent of the TIA**. It supplies the three things
the VCS needs outside the TIA: the only RAM, the two I/O ports, and an interval
timer. Per ref-docs/research-report.md §7.

**Audio is NOT here.** The VCS's two sound channels live in the TIA
(`rusty2600-tia::audio`); the 6532 has no sound hardware. See `docs/tia.md` §Audio
and `docs/architecture.md` fact 4.

## RAM — the console's only general RAM

**128 bytes** at `$80..=$FF`. There is no separate work RAM on the 2600; this is
it, and the **CPU stack overlaps this region** (the 6502 `$0100` stack page maps
into the RIOT RAM mirror). The `Bus` therefore has no `wram` field. Power-on RAM
contents are randomized from the **seeded** PRNG in the owning `System`
(determinism contract, `docs/adr/0004`), never the OS RNG. Per
ref-docs/research-report.md §7.

## I/O ports

| Port | Addr | DDR | Contents |
|---|---|---|---|
| `SWCHA` | $280 | `SWACNT` ($281) | Port A: the two joystick directionals (and paddle/keypad multiplexing) |
| `SWCHB` | $282 | `SWBCNT` ($283) | Port B: console switches — Reset, Select, Color/B&W, the two difficulty switches |

Idle inputs read high (pulled up): `ports` power on to `0xFF`. `SWCHB` DDR is
effectively hardwired to input. Per ref-docs/research-report.md §7.

## Interval timer

Write the count to one of four prescaler-selecting addresses; the timer then
decrements at that prescale:

| Write addr | Name | Prescale | Tick period (NTSC) |
|---|---|---|---|
| $294 | `TIM1T` | 1 CPU cycle | 838 ns |
| $295 | `TIM8T` | 8 cycles | 6.7 µs |
| $296 | `TIM64T` | 64 cycles | 53.6 µs |
| $297 | `T1024T` | 1024 cycles | 858.2 µs (power-on default) |

Read the current value at `INTIM` ($284) and the status at `INSTAT` ($285,
undocumented). After hitting 0 the timer **holds 0 for one prescale interval**,
then wraps to `$FF` and decrements **once per CPU cycle** (so software can measure
overshoot). This timer is the 2600's substitute for a frame interrupt — and
because the 6507 has no interrupt lines, **the timer cannot fire an IRQ; software
polls `INTIM`**. Per ref-docs/research-report.md §7.

The `Prescale` enum (`By1` / `By8` / `By64` / `By1024`) and the `Timer` struct
(`value` = INTIM, `prescale`, accumulated `elapsed`) model this. The
`INSTAT` underflow flag + the post-underflow 1-cycle mode are `T-0601-005`.

## Timing

The RIOT advances on the **CPU cycle** (every third TIA color clock); the
interval timer is further prescaled by 1 / 8 / 64 / 1024. See `docs/scheduler.md`
for the divisor table. `Riot::tick()` is the per-CPU-cycle hook.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
