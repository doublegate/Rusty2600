# Scheduler — Rusty2600

References: `ref-docs/research-report.md` §1 (clock topology), §5 (beam-racing
model), §12 (architecture options); `docs/architecture.md`; `docs/adr/0001`,
`docs/adr/0002`; `crates/rusty2600-core/src/scheduler.rs`.

## Purpose

The scheduler is the heart of the cycle-exact emulator: it advances the TIA, the
6507 CPU, the RIOT timer, and any cart coprocessor in tight lockstep at **TIA
color-clock resolution**. It lives in `rusty2600-core` (the `System` type) and is
the single owner of the run loop.

## Clock topology

The master clock is the **NTSC color subcarrier, 3.579545 MHz** (PAL 3.546894
MHz). Everything else is a fixed integer divisor of it. Per
ref-docs/research-report.md §1.

```text
            color clock  3.579545 MHz  (NTSC; master)
                  | /3
            6507 CPU φ0   ~1.193182 MHz
                  | (RIOT timer advances on the CPU cycle)
            audio clock   color/114  ~31.4 kHz  (CPU/114 for AUDC 12..15)
```

### Divisor table

`tick_one_color_clock()` advances the master one unit; every other unit advances
on its divisor — LOCKSTEP, not catch-up.

| Unit | Advances every N color clocks | Rate (NTSC) | Notes |
|---|---|---|---|
| TIA color clock (master) | 1 | 3.579545 MHz | one pixel emitted per visible color clock |
| 6507 CPU | 3 | ~1.193182 MHz | offset by the seeded power-on phase (0/1/2) |
| RIOT interval timer | 3 (one CPU cycle) | ~1.193182 MHz | further prescaled by 1 / 8 / 64 / 1024 |
| Cart coprocessor (DPC etc.) | 3 (`Board::tick`) | ~1.193182 MHz | default no-op; DPC oscillator may free-run |
| TIA audio | 114 | ~31.4 kHz | `color/114`; `CPU/114` for AUDC 12..15 |

PAL (3.546894 MHz) keeps the same `/3` and `/114` integer divisors; only the
absolute master frequency and the frame line budget differ (see
`docs/compatibility.md`).

## Resolution: integer color-clock lockstep

Resolution is **integer color clocks**. A scanline is **228 color clocks** = 68
HBLANK + 160 visible = exactly **76 CPU cycles** (228 / 3). Because the 2600
program already races the beam at color-clock granularity, integer resolution is
the natural — and sufficient — timebase: there is no sub-color-clock event the
program can observe. This is why ADR 0002's fractional refactor is marked
**likely unneeded** for the 2600 (contrast the NES/SNES, where φ1/φ2 sub-cycle
phase matters). Per ref-docs/research-report.md §12.

## The run loop

```rust
fn tick_one_color_clock(&mut self) {
    self.bus.tia.tick_color_clock();              // emit one dot, advance beam
    if self.color_clocks % 3 == self.phase {       // CPU φ0 fires every 3rd clock
        if self.bus.tia.rdy_stall() {
            // RDY held: CPU frozen this cycle (the WSYNC beam-stall).
        } else {
            self.cpu.tick(&mut CpuView(&mut self.bus));
            self.bus.riot.tick();                  // RIOT timer on the CPU cycle
            if let Some(b) = self.bus.board.as_mut() { b.tick(); }
        }
    }
    self.color_clocks += 1;
}
```

`phase` is the per-power-on CPU/color-clock alignment (0, 1, or 2), seeded from
a deterministic PRNG (`System::new(seed)`), never the OS RNG. Reset preserves
it; a cold power-cycle re-rolls it from the seed (determinism contract,
`docs/adr/0004`).

## WSYNC / RDY: the CPU beam-stall

Writing `WSYNC` ($02) clears the RDY latch and **halts the CPU until the next
HBLANK starts** (the start of the next scanline). This is how software locks to
the beam: do all per-line register setup, `STA WSYNC`, and the CPU stalls
precisely to line start. Per ref-docs/research-report.md §5.3.

The TIA **owns** the signal (`Tia::rdy_stall`). The scheduler reads it and
simply skips the CPU step while it is asserted — **the color clock keeps
running, only the CPU is frozen**. The TIA releases RDY when its color-clock
counter wraps at the end of HBLANK (`tick_color_clock` clears `rdy_stall` on the
228 → 0 wrap). The exact mid-instruction freeze point is `T-0201-006`.

Because the CPU access ticks its own cycle *before* the register write is
applied (`write1` = tick-then-write), a `STA WSYNC` whose final cycle lands on
the 228 → 0 wrap arrives with the beam already at `color_clock == 0`. Arming the
stall then would over-wait a full extra scanline, so `Tia::write_register` skips
arming `rdy_stall` at `color_clock == 0` (the strobe is already satisfied by the
boundary it coincides with). See `docs/tia.md` §WSYNC — this was Frogger's
positioning-kernel jitter.

## Bus design

The `Bus` owns the TIA, the RIOT, the cart (boxed `Board`), and the open-bus
latch — there is **no `wram` field** (the 2600's only RAM is in the RIOT). The
CPU borrows `&mut Bus` through the narrow `CpuBus` adapter (`CpuView`). The video
and audio paths each see a smaller trait:

- `VideoBus`: `video_read` (cart-mediated reads the beam needs).
- `AudioBus`: `audio_sample` (the core draws the TIA's mixed sample — audio
  lives in the TIA, so this is read-side only).

See `docs/architecture.md` for the full crate map.

## No interrupt dispatch

The 6507 has **no IRQ or NMI pins** (they were dropped to fit A12 in the 28-pin
package). There is no interrupt path in the scheduler — the only "timing
interrupt" the program gets is polling the RIOT timer (`INTIM`) or `WSYNC`. The
`BRK` software interrupt and the RESET vector still exist. Per
ref-docs/research-report.md §4.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
