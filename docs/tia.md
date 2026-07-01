# TIA (Television Interface Adaptor) — Rusty2600

References: `ref-docs/research-report.md` §5 (TIA video), §6 (TIA audio), §10
(engineering challenges); `docs/scheduler.md`; `docs/architecture.md`;
`crates/rusty2600-tia/src/lib.rs`. This doc is the SPEC, not history — update it
in the same PR as the code, and pin behaviour against the test ROMs first (when
docs and a passing ROM disagree, the ROM wins; Stella is the behavioral oracle).

## What the TIA is

The TIA is the VCS's custom **video AND audio** chip. It has **no framebuffer**:
it renders one pixel of luminance/colour per *color clock* directly from its
object registers as the electron beam sweeps, and the CPU program rewrites those
registers **mid-scanline** ("racing the beam") to compose a picture. Audio lives
**here too** (two channels), not in the RIOT. This is the architectural crux of
the whole emulator. Per ref-docs/research-report.md §5–6.

## Horizontal timing

- A scanline is **228 color clocks**: **68 HBLANK** (not displayed) + **160
  visible** (1 color clock = 1 pixel, max 160 horizontal pixels).
- One CPU machine cycle spans 3 color clocks, so a line is **76 CPU cycles**
  (228 / 3).
- The TIA's HSync counter counts 0..56 (a 57-count period at 1/4 CLK; 57 × 4 =
  228).

Per ref-docs/research-report.md §5.1.

## Vertical timing and the VSYNC/VBLANK protocol

The **program** manually generates vertical sync. An NTSC frame is **3 VSYNC + 37
VBLANK + 192 visible + 30 overscan = 262 lines**; PAL is **3 + 45 + 228 + 36 =
312** (the PAL visible-line budget is flagged for test-ROM confirmation — see
§14 of the research report). `VSYNC` ($00, write D1=1 to start) and `VBLANK`
($01, D1 blank + D6/D7 input latch/dump control) are toggled by software at the
right line counts. Region line budgets are **data**, not a build fork (see
`docs/compatibility.md`). Per ref-docs/research-report.md §5.2.

## WSYNC: CPU-halt synchronization

Writing `WSYNC` ($02) drives `RDY` low and halts the CPU until the next HBLANK
starts. The TIA owns the signal (`Tia::rdy_stall`); the scheduler reads it and
freezes only the CPU — the color clock keeps running. Released on the 228 → 0
color-clock wrap. See `docs/scheduler.md` §WSYNC/RDY.

**Line-boundary edge case (`color_clock == 0` at the strobe).** The scheduler
advances the `STA WSYNC`'s own CPU cycle (its 3 color clocks) *before* applying
the write, so when the store's final cycle lands exactly on the 228 → 0 wrap the
beam is already at `color_clock == 0` — the start of the very scanline the WSYNC
was waiting for. In that case `write_register` must **not** arm `rdy_stall`:
doing so would make the next opcode fetch spin until the *following* wrap, a
phantom extra scanline. This matches Gopher2600 / Stella ("`RDY` released at the
leading edge of the next HBLANK") — the strobe is satisfied by the boundary it
coincides with, not the next one. Frogger's `STA WSYNC; STA HMOVE` object
fine-positioning routine strobes WSYNC right at the line boundary; the phantom
line made its frame wobble 262..267 instead of a rock-steady 262 (verified
byte-for-byte against Gopher2600: the whole frame's instruction stream was
identical, differing only by this single over-long stall).

## Object positioning by write timing (RESPx) — the cycle-exact crux

Five movable objects: player 0, player 1, missile 0, missile 1, ball. Each is
positioned by **when** the program writes its strobe register — `RESP0` ($10),
`RESP1` ($11), `RESM0` ($12), `RESM1` ($13), `RESBL` ($14). The horizontal
position is set to wherever the beam happens to be at the write.

Towers gives the exact reset latency: resetting the counter takes 4 CLK, decoding
the "start drawing" signal takes 4 CLK, latching the start takes a further 1 CLK
— a total **9-CLK delay** after a `RESP0/1`. Graphics then appear at copy offsets
(close 12 CLK / medium 28 CLK / far 60 CLK) per the NUSIZ spacing. Getting this
9-clock pipeline and the per-pixel position right is the single most error-prone
part of a 2600 emulator. Per ref-docs/research-report.md §5.4, §10.

## HMOVE and the motion registers — the "comb" / black-bar quirk

Each object has a 4-bit **signed** motion register holding **+7 to −8**: `HMP0`
($20), `HMP1` ($21), `HMM0` ($22), `HMM1` ($23), `HMBL` ($24). Strobing `HMOVE`
($2A) — which must immediately follow a `WSYNC` — applies the motion by
"clock-stuffing" extra pulses into each object's position counter during HBLANK:
the internal counter begins at 15 and decrements to zero at 1 decrement every 4
CLK, and each injected pulse shifts the object 1 pixel. `HMCLR` ($2B) zeroes all
five motion registers at once.

The famous quirk: an `HMOVE` strobed at the start of the line **extends HBLANK by
8 color clocks**, blanking the leftmost 8 pixels — the "HMOVE comb" / black
left-edge bar. Strobing `HMOVE` late (not right after `WSYNC`) produces the
partial "comb" teeth artifact many games and demos exploit. Reproducing this
exactly is a known accuracy test-case. Per ref-docs/research-report.md §5.5, §10.

## Playfield

`PF0` ($0D, high 4 bits used), `PF1` ($0E, 8 bits), `PF2` ($0F, 8 bits) form a
20-bit playfield drawn across the left half of the screen; `CTRLPF` ($0A)
controls **reflect** (mirror vs repeat to the right half), **score** mode (each
half takes its player's colour), **priority** (playfield over players), and ball
size. The playfield can be rewritten mid-line for higher effective resolution.
The `Objects.pf` field packs the 20 bits. Per ref-docs/research-report.md §5.6.

## Players, missiles, ball

- **Players:** `GRP0` ($1B), `GRP1` ($1C) are 8-bit graphics. `NUSIZ0` ($04) /
  `NUSIZ1` ($05) set copies and player/missile size; `REFP0` ($0B) / `REFP1`
  ($0C) mirror the sprite (write D3=1). NUSIZ is read every graphics CLK, so it
  can change mid-object.
- **Vertical delay:** `VDELP0` ($25), `VDELP1` ($26), `VDELBL` ($27) (write D0)
  select between the "new" and "old" copy of the graphics register — the classic
  alternating-line sprite trick.
- **Missiles/ball:** `ENAM0` ($1D), `ENAM1` ($1E), `ENABL` ($1F) enable (D1);
  `RESMP0` ($28), `RESMP1` ($29) lock a missile to its player's centre (D1).

Per ref-docs/research-report.md §5.7.

## Collisions

15 collision pairs are detected in hardware per pixel and latched, read through
`CXM0P` ($00), `CXM1P` ($01), `CXP0FB` ($02), `CXP1FB` ($03), `CXM0FB` ($04),
`CXM1FB` ($05), `CXBLPF` ($06), `CXPPMM` ($07) (each returns 1–2 bits on D6/D7),
and cleared by `CXCLR` ($2C). The emulator re-evaluates per-pixel object
overlap on every visible color clock (not just once when an object is first
enabled) and latches it; `CXCLR` clears all eight registers on the same cycle
its write lands. Pinned by `collision_latch_sets_when_missile_overlaps_player`
and `cxclr_clears_within_the_same_cycle` (v0.2.0). Per
ref-docs/research-report.md §5.8.

**Known gap: HBLANK-region collisions (`T-0601-007`, unscheduled).** Object
positions (`Objects::pos`) and the collision-evaluation pixel coordinate are
both modeled in the 0..159 *visible-window* space (`x = color_clock - 68`),
not the full 0..227 raw color-clock range — `render_pixel` early-returns
before ever reaching the collision block when `color_clock < 68`. On real
hardware the position counters and graphics-shift logic that drive collision
detection run continuously regardless of HBLANK blanking (only the video
DAC output is suppressed), so a collision that would occur during HBLANK is
a real, if obscure, hardware behavior this emulator does not currently
reproduce — some advanced demos/homebrew reportedly use HBLANK collisions as
an invisible timing signal. Fixing this properly means extending the
position/pixel-coordinate model to the full scanline, which risks the
already-verified RESPx/HMOVE visible-window positioning logic (see the
regression tests above it) — deliberately deferred rather than rushed;
revisit alongside a broader TIA accuracy pass once a differential-oracle
probe against Gopher2600/Stella can confirm the exact expected behavior.

## Colour

`COLUP0` ($06), `COLUP1` ($07), `COLUPF` ($08), `COLUBK` ($09) hold 7-bit
colour/luma values (4-bit hue + 3-bit luma). The hue → RGB mapping differs by
region — the same value is yellowish on NTSC, gray on PAL, aqua on SECAM — so the
palette is **region data** (`docs/compatibility.md`). Per
ref-docs/research-report.md §5.9.

## Write-register map (anchor subset)

The `regs` module pins the strobes; the full map is the table above. Strobe
registers (`WSYNC`, `RSYNC`, `RESPx`, `RESMx`, `RESBL`, `HMOVE`, `HMCLR`,
`CXCLR`) act on write regardless of the data byte.

## Audio (lives in the TIA, not the RIOT)

Two **fully independent** channels, three registers each:

| Register | Addr | Width | Meaning |
|---|---|---|---|
| `AUDC0` / `AUDC1` | $15 / $16 | 4-bit | control / distortion select (waveform) |
| `AUDF0` / `AUDF1` | $17 / $18 | 5-bit | frequency divider (divides reference by AUDF+1) |
| `AUDV0` / `AUDV1` | $19 / $1A | 4-bit | volume (0–15) |

### Clock derivation

Audio clock = system color clock / 114 ≈ **31,400 Hz** (NTSC 31399.5 Hz, PAL
31113.1 Hz). The *Stella Programmer's Guide*'s "~30 kHz" is a rounding; implement
the precise `3.579545 MHz / 114`. When AUDC selects the CPU-clock distortions
(values 12–15), the source clock is **CPU clock / 114** instead. Per
ref-docs/research-report.md §6.1 (the ~31.4 kHz value is the resolved figure; the
guide's 30 kHz is flagged as a rounding in §14).

### Polynomial-counter synthesis

Each channel is a **clock divider** (gated by AUDF) feeding a **polynomial shift
register** whose tap length is chosen by AUDC. The TIA uses shift registers of
length 4-bit (period 15), 5-bit (period 31), and 9-bit (period 511) to make pure
tones and pseudo-random noise. The 16 AUDC distortion modes map to specific
shift-register configurations (Stolberg's table, derived from Ron Fries'
TIASOUND, which Stella uses):

| AUDC | Distortion | Source clock |
|---|---|---|
| 0 | constant HIGH (set-to-1 / 4-bit sample-by-volume) | color/114 |
| 1 | 4-bit poly | color/114 |
| 2 | 4-bit poly clocked by div-15 | color/114 |
| 3 | 5-bit poly → 4-bit poly composite | color/114 |
| 4, 5 | pure tone, divide-by-2 (square) | color/114 |
| 6 | divide-by-31 pure tone | color/114 |
| 7 | 5-bit poly → divide-by-31 | color/114 |
| 8 | 9-bit poly (white noise) | color/114 |
| 9 | 5-bit poly | color/114 |
| 10 | divide-by-31 pure tone | color/114 |
| 11 | constant HIGH (sample mode) | color/114 |
| 12, 13 | pure tone, divide-by-6 (square) | **CPU/114** |
| 14 | divide-by-93 pure tone | **CPU/114** |
| 15 | 5-bit poly → divide-by-93 | **CPU/114** |

AUDC `0xA`/`0xB` behave distinctly from the Stella manual due to clock/data
alignment — pin them bit-exactly against TIASOUND/Stella, not the prose docs
(research report §14, open question 2). Output is **non-linear by volume** (AUDV
selects 1 of 16 impedance levels), and the two channels mix. Per
ref-docs/research-report.md §6.2.

## Timing

The TIA is the master clock: it advances one color clock per
`tick_color_clock()`, the CPU steps every third, and audio advances every 114th.
See `docs/scheduler.md` for the divisor table.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
