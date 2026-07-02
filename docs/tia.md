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

## Object-ID mask (`hd-pack` feature, v2.7.0)

`render_pixel` resolves `current_color` from `pf_pixel`/`bl_pixel`/`p0_pixel`/`p1_pixel`/
`m0_pixel`/`m1_pixel` (the per-object "on at this dot" booleans) plus the `CTRLPF` bit 2
priority order. Behind the off-by-default `hd-pack` Cargo feature, `Tia` records a SECOND,
parallel per-pixel output alongside `video_buffer`: `object_mask: Vec<ObjectTag>`, indexed
identically (`scanline * 160 + x`). `ObjectTag { object: ObjectId, grp: u8, nusiz: u8 }` names
which object won color-priority resolution at that dot (`ObjectId::{Background, Playfield,
Ball, Missile0, Missile1, Player0, Player1}`), plus -- for `Player0`/`Player1` pixels -- the
exact `GRPx` (VDELP-resolved) and `NUSIZx` byte live when that pixel was rendered.

This is a read-only tap on values `render_pixel` already computes; it does not alter
`current_color`'s resolution order, and is compiled out entirely when `hd-pack` is off (the
default), leaving `video_buffer` and every existing behavior byte-identical. Within the
color-priority groups that share one color (`p0_pixel || m0_pixel`, `p1_pixel || m1_pixel`,
`pf_pixel || bl_pixel`), the mask picks a single concrete object with a fixed, documented
precedence (player over its own missile; playfield over ball) since color resolution itself
never distinguishes them.

Capturing `GRPx`/`NUSIZx` fresh on every `render_pixel` call (not once per frame/scanline) is
what makes sprite multiplexing (a game rewriting `GRP0`/`GRP1` mid-scanline, between drawn
columns, to draw more than 2 player objects per line) resolve correctly: pixels before and
after such a rewrite carry different captured values, matching what was actually drawn at
each one.

The mask is output-only, consumed by `rusty2600-frontend`'s HD-pack live rendering splice
(`crate::sprite_pack`, `crate::emu_thread::EmuCore::step_frame`) -- never read back into TIA
behavior. Missile/ball/playfield/background pixels are tagged but have no HD-pack replacement
key (`sprite_pack`'s data model is player-only by design; see its own doc comment).

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

**This table (and `crates/rusty2600-tia/src/audio.rs`'s current 16-entry
match on `AUDC`) is the classic TIASOUND model, confirmed to be a
simplification, not what Stella's current source actually implements.**
Investigating AUDC `0xA`/`0xB` specifically (research report §14, open
question 2) surfaced a much larger finding — see
`ref-docs/2026-07-01-supplemental-audio-hardware-model.md` for the full
writeup. In short: Stella's `AudioChannel.cxx` splits `AUDC`'s 4 bits into
two independent 2-bit noise/pulse-feedback fields driving two counters
(`myPulseCounter`, `myNoiseCounter`), clocked via two phases (`phase0`/
`phase1`) that fire at four *fixed* color-clock positions per scanline (9,
37, 81, 149 — not an evenly-spaced or simple-modulo pattern), with volume
sampled and averaged every color clock rather than read out once per tick.
0xA/0xB only make sense as specific field combinations in that model — there
is no clean patch to the existing lookup table that fixes just those two
entries. Full audio-clocking accuracy needs a dedicated rearchitecture (see
`to-dos/phase-3-audio/sprint-2-hardware-accurate-model.md`), not a two-mode
pin — deliberately deferred rather than rushed, same as the collision-HBLANK
gap above.

Output mixing: this doc previously said "non-linear by volume" while
`audio.rs`'s own module doc says "the 2600 mix is a simple linear sum of the
two 4-bit volumes" — those two statements conflict; resolving which is
correct is folded into the same rearchitecture ticket above rather than
guessed at here. Per ref-docs/research-report.md §6.2.

## Timing

The TIA is the master clock: it advances one color clock per
`tick_color_clock()`, the CPU steps every third, and audio advances every 114th.
See `docs/scheduler.md` for the divisor table.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
