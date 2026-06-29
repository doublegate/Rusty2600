# Rusty2600 Research Report — Atari 2600 / Video Computer System

Immutable research corpus for `Rusty2600`, a cycle-exact Atari 2600 (VCS)
emulator in Rust, architected to match the RustyNES reference emulator.

- Status: research phase, frozen on completion
- Date authored: 2026-06-24
- Accuracy bar: Stella (the de-facto reference), Gopher2600, ares
- Source manifest: see the numbered, tier-grouped manifest at the end. Every
  non-trivial technical claim below carries an inline citation.

> Immutability note: after this report is finished, `ref-docs/` is frozen.
> Corrections and new findings go in NEW dated supplemental files
> (`ref-docs/YYYY-MM-DD-supplemental-NAME.md`), never as edits to this file.

---

## 1. Executive summary

The Atari 2600 is the canonical "racing-the-beam" console. Unlike the NES (the
RustyNES target), it has **no framebuffer and no tile/sprite RAM**: its video
chip, the TIA (Television Interface Adapter), renders a single scanline's worth
of pixels live as the electron beam sweeps, and the CPU program is responsible
for rewriting the TIA's registers *mid-scanline* in lockstep with the beam to
build a picture [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt)
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).
This makes cycle-exact timing not an accuracy nicety but the entire ballgame:
an emulator that is even one color clock off on a `RESP0` strobe or an `HMOVE`
will render visibly wrong graphics.

Key hardware facts the emulator must nail:

- **Clock topology.** Master clock = the NTSC color subcarrier
  **3.579545 MHz** (PAL **3.546894 MHz**). The 6507 CPU runs at exactly
  **color clock / 3** (~1.19 MHz); integer lockstep, CPU advances one cycle
  every 3rd color clock [[10]](https://problemkaputt.de/2k6specs.htm)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).
- **Beam geometry.** Each scanline is **228 color clocks**: **68** of HBLANK
  (not displayed) + **160** visible (= 160 pixels). One CPU machine cycle
  spans 3 color clocks, so a line is **76 CPU cycles** (228 / 3)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). NTSC frame =
  **262 lines** (3 VSYNC + 37 VBLANK + 192 visible + 30 overscan); PAL =
  **312 lines** [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[10]](https://problemkaputt.de/2k6specs.htm).
- **The CPU is a 6507**, a 6502 in a 28-pin package with only **13 address
  lines (A0-A12 = 8 KB space)** and **no IRQ/NMI pins** (the interrupt pins
  were dropped to fit A12). Internally the die is identical to the 6502
  [[11]](https://en.wikipedia.org/wiki/MOS_Technology_6507).
- **Audio lives in the TIA, not the RIOT** — two independent channels
  (`AUDC0/1`, `AUDF0/1`, `AUDV0/1`) driven by polynomial shift-register
  counters off a ~30 KHz audio clock
  [[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html)
  [[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide).
- **The RIOT (MOS 6532)** supplies the only RAM (128 bytes), the two I/O ports
  (`SWCHA` joysticks, `SWCHB` console switches), and the interval timer
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[10]](https://problemkaputt.de/2k6specs.htm).
- **Bankswitching breadth.** The 8 KB address window forces dozens of cart
  mappers (2K/4K through F8/F6/F4, FA/Superchip RAM variants, E0/E7/3F/3E/FE/
  UA/0840, up to coprocessor carts DPC/DPC+/CDF). This report enumerates them
  with a suggested accuracy tier (Section 8).

The emulator's accuracy oracle is built from the **Klaus Dormann 6502
functional + decimal tests** and the **SingleStepTests/ProcessorTests** per-
opcode golden vectors for CPU correctness, plus TIA timing/draw test ROMs and
the Stella regression corpus for the rest of the system (Section 11). This maps
directly onto RustyNES's "AccuracyCoin" honesty-gated accuracy battery concept.

---

## 2. Scope and goals

In scope:

- Cycle-exact 6507 (full documented + undocumented NMOS 6502 instruction set,
  decimal mode).
- Cycle-exact TIA: beam-racing video synthesis (playfield, two players, two
  missiles, ball, collisions, `HMOVE`/`RESPx` positioning) and the 2-channel
  polynomial-counter audio.
- The MOS 6532 RIOT: 128 bytes RAM, two I/O ports, interval timer.
- A tiered cartridge/bankswitch layer (Core/Curated/BestEffort) with an honesty
  gate so a BestEffort mapper never backs the accuracy oracle.
- NTSC / PAL / SECAM region timing as data, not a build fork.
- A deterministic core (same ROM + input + seed -> bit-identical frame + audio)
  to support save-states, regression goldens, and TAS replay, mirroring
  RustyNES's determinism contract.

Out of scope for the initial cut (candidate later work): the ARM-coprocessor
carts (DPC+/CDF/CDFJ via the Harmony/Melody ARM) beyond a BestEffort stub;
Supercharger (`AR`) tape loading; exotic peripherals (Trak-Ball, light guns,
AtariVox) beyond the standard joystick/paddle set.

---

## 3. Background

The Atari Video Computer System (later "2600") shipped in 1977. To hit its
price point Atari minimized silicon: rather than a frame buffer (RAM was
expensive), the design exposes the raster directly to software. The "Stella"
project codename survives as the name of the canonical reference emulator
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
[[12]](https://stella-emu.github.io/docs/index.html).

Three custom/standard chips make up the machine:

1. **6507 CPU** — cost-reduced 6502 (28-pin, 13 address lines, no interrupts)
   [[11]](https://en.wikipedia.org/wiki/MOS_Technology_6507).
2. **TIA** — custom Television Interface Adapter: generates the video signal and
   2-channel sound, handles object positioning and collisions
   [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
3. **RIOT / PIA (MOS 6532)** — 128 bytes RAM, two 8-bit ports, an interval
   timer [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

The canonical programmer-facing reference is Steve Wright's *Stella Programmer's
Guide* (1979, updated 1988)
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html); the
canonical hardware-level reference is Andrew Towers' *TIA Hardware Notes* (2003),
reverse-engineered from the TIA-1A schematics
[[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).

---

## 4. Technical deep-dive: the 6507 CPU

The 6507 is a MOS 6502 die in a 28-pin DIP. The package drops the 6502's A13-A15
address pins (leaving **A0-A12**, a **13-bit / 8 KB** address bus) and **omits
both the IRQ and NMI pins** — those lines were sacrificed precisely to make room
for A12 within the smaller package
[[11]](https://en.wikipedia.org/wiki/MOS_Technology_6507). The 6507 and 6502
"share the same underlying silicon layers, and differ only in the final
metallisation layer" [[11]](https://en.wikipedia.org/wiki/MOS_Technology_6507),
so for emulation purposes **the 6507 core is a 6502 core**: same registers,
same documented and undocumented NMOS instructions, same NMOS decimal mode.

Consequences for Rusty2600:

- **No hardware interrupts.** There is no IRQ/NMI dispatch path. (The `BRK`
  software interrupt opcode and the reset vector still exist.) The 2600's only
  "timing interrupt" mechanism is polling the RIOT timer or `WSYNC` — there is
  no NES-style PPU NMI.
- **Address mirroring is severe.** With only 13 address lines decoded, and the
  TIA/RIOT each using only a few low address lines, the same registers appear at
  many mirror addresses; the cart's bankswitch hotspots also live in this
  cramped map (Section 8).
- **NMOS decimal mode must be exact.** Unlike the NES's 2A03 (which disables
  BCD), the 2600's 6507 has a working decimal mode, and games use it. Bruce
  Clark's decimal test (bundled in the Klaus Dormann suite) is the oracle
  [[3]](https://github.com/Klaus2m5/6502_65C02_functional_tests).
- **Undocumented opcodes.** These are the standard NMOS illegal opcodes
  (`LAX`, `SAX`, `SLO`, `RLA`, `SRE`, `RRA`, `DCP`, `ISC`, `ANC`, `ALR`,
  `ARR`, `AXS`/`SBX`, plus the unstable `AHX`/`SHX`/`SHY`/`TAS`/`LAS`/`XAA`)
  [[8]](https://www.nesdev.org/wiki/CPU_unofficial_opcodes). 2600 developers
  reportedly *avoided* relying on them out of fear a CPU mask revision could
  "fix" them and break shipped games
  [[8a]](https://www.masswerk.at/nowgobang/2021/6502-illegal-opcodes), but the
  emulator must still implement them correctly (a few homebrew and edge ROMs use
  them, and the ProcessorTests vectors exercise them). Treat them as a hard
  correctness requirement, not optional.

Cycle accuracy is per-cycle bus access: each instruction's read/write bus cycles
(including the "dummy" reads on page-cross and RMW instructions) must land on the
exact CPU cycle, because the TIA advances 3 color clocks per CPU cycle and a
write to a TIA strobe register at the wrong cycle moves graphics
[[7]](https://github.com/SingleStepTests/ProcessorTests/blob/main/6502/README.md).

---

## 5. Technical deep-dive: the TIA video (the beam-racing model)

This is the architectural crux. The TIA has **no framebuffer**. Software
synchronizes to the beam cycle-by-cycle
[[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).

### 5.1 Horizontal timing

- A scanline is **228 color clocks**. The TIA's HSync counter "counts from 0 to
  56 ... a period of 57 counts at 1/4 CLK (57 * 4 = 228 CLK)"
  [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
- **68 color clocks** of HBLANK (not displayed) + **160 color clocks** visible
  (1 color clock = 1 pixel, max 160 horizontal pixels)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
- **76 CPU cycles per line** (228 / 3)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

### 5.2 Vertical timing and the VSYNC/VBLANK protocol

The program manually generates the vertical sync. A frame is (NTSC):
**3 lines VSYNC, 37 lines VBLANK, 192 visible, 30 overscan = 262 total**; PAL is
**3 + 45 + 228 + 36 = 312** [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
[[10]](https://problemkaputt.de/2k6specs.htm). `VSYNC` ($00, write D1=1 to
start), `VBLANK` ($01, D1 blank + D6/D7 input latch/dump control) are software
toggled at the right line counts [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

### 5.3 WSYNC: CPU-halt synchronization

Writing `WSYNC` ($02) **clears the RDY latch and halts the CPU until the next
HBLANK starts** (the start of the next scanline)
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html). This
is how software locks to the beam: do all per-line register setup, `STA WSYNC`,
and the CPU stalls precisely to line start. The emulator must freeze the CPU
clock (but keep TIA/RIOT clocks running) until the color-clock counter wraps.

### 5.4 Object positioning by write timing (RESPx) — the cycle-exact crux

There are five movable objects: player 0, player 1, missile 0, missile 1, ball.
Each is positioned by **when** the program writes its strobe register —
`RESP0` ($10), `RESP1` ($11), `RESM0` ($12), `RESM1` ($13), `RESBL` ($14). The
horizontal position is set to wherever the beam happens to be at the moment of
the write [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

Towers gives the exact reset latency: "Resetting the counter takes 4 CLK,
decoding the 'start drawing' signal takes 4 CLK, latching the 'start' takes a
further 1 CLK giving a total **9 CLK delay** after a RESP0/1"
[[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). The graphics then
appear at copy offsets (close 12 CLK / medium 28 CLK / far 60 CLK) per the NUSIZ
spacing [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). Getting this
9-clock pipeline and the per-pixel position right is the single most error-prone
part of a 2600 emulator.

### 5.5 HMOVE and the motion registers — the "comb" / black-bar quirk

For fine positioning between strobes, each object has a 4-bit signed motion
register: `HMP0` ($20), `HMP1` ($21), `HMM0` ($22), `HMM1` ($23), `HMBL` ($24),
each holding **+7 to -8** [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).
Strobing `HMOVE` ($2A) — which "must immediately follow a WSYNC" — applies the
motion by "clock-stuffing" extra pulses into each object's position counter
during HBLANK; the internal counter "begins at 15 and decrements down to zero
... at a rate of 1 decrement every 4 CLK," and each injected pulse shifts the
object 1 pixel [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). The
`HMCLR` ($2B) strobe zeroes all five motion registers at once
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

The famous quirk: an `HMOVE` strobed at the start of the line **extends HBLANK
by 8 color clocks**, blanking the leftmost 8 pixels — the "HMOVE comb" / black
left-edge bar. "The extended HBlank hides the normal playfield output for the
first 8 pixels of the line"
[[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). Strobing `HMOVE`
late (not right after `WSYNC`) produces the partial "comb" teeth artifact that
many games and demos exploit. Reproducing this exactly is a known accuracy
test-case.

### 5.6 Playfield

`PF0` ($0D, high 4 bits used), `PF1` ($0E, 8 bits), `PF2` ($0F, 8 bits) form a
20-bit playfield drawn across the left half of the screen; `CTRLPF` ($0A)
controls **reflect** (mirror to right half vs repeat), **score** mode (each half
takes its player's color), **priority** (playfield over players), and ball size
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html). The
playfield can be rewritten mid-line for higher effective resolution.

### 5.7 Players, missiles, ball

- **Players:** `GRP0` ($1B), `GRP1` ($1C) are 8-bit graphics. `NUSIZ0` ($04) /
  `NUSIZ1` ($05) set number of copies and player/missile size; `REFP0` ($0B) /
  `REFP1` ($0C) mirror the sprite (write D3=1)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).
  NUSIZ is "read every graphics CLK" so it can change mid-object
  [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
- **Vertical delay:** `VDELP0` ($25), `VDELP1` ($26), `VDELBL` ($27) (write D0)
  select between the "new" and "old" copy of the graphics register, enabling the
  classic alternating-line sprite trick
  [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
- **Missiles/ball:** `ENAM0` ($1D), `ENAM1` ($1E), `ENABL` ($1F) enable (D1);
  `RESMP0` ($28), `RESMP1` ($29) lock a missile to its player's center (D1)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

### 5.8 Collisions

Collisions are detected in hardware and latched: 15 collision pairs read through
`CXM0P` ($00), `CXM1P` ($01), `CXP0FB` ($02), `CXP1FB` ($03), `CXM0FB` ($04),
`CXM1FB` ($05), `CXBLPF` ($06), `CXPPMM` ($07) (each returns 1-2 bits on
D6/D7), cleared by `CXCLR` ($2C)
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html). The
emulator must compute per-pixel object overlap during synthesis and latch it.

### 5.9 Color

`COLUP0` ($06), `COLUP1` ($07), `COLUPF` ($08), `COLUBK` ($09) hold 7-bit
color/luma values (4-bit hue + 3-bit luma). The hue-to-RGB mapping differs by
region: the same `$1A` value is yellowish on NTSC, gray on PAL, aqua on SECAM —
so the palette is region data
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
[[13]](https://www.randomterrain.com/atari-2600-memories-tutorial-andrew-davie-11.html).

---

## 6. Technical deep-dive: the TIA audio

Audio is **part of the TIA** (not the RIOT) and has **two fully independent
channels**, each with three registers
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
[[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html):

- `AUDC0` ($15) / `AUDC1` ($16) — 4-bit **control / distortion** select. Picks
  the polynomial-counter / divider waveform.
- `AUDF0` ($17) / `AUDF1` ($18) — 5-bit **frequency divider** (divides the
  reference by AUDF+1; AUDF range 0-31).
- `AUDV0` ($19) / `AUDV1` ($1A) — 4-bit **volume** (0-15)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

### 6.1 Clock derivation

The audio clock = system color clock / 114. Sources give the NTSC audio clock
as **3.579545 MHz / 114 ~= 31,400 Hz** (the *Stella Programmer's Guide* rounds
to "approximately 30 KHz")
[[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html)
[[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide).
Problem-Kaputt's spec gives **NTSC 31399.5 Hz, PAL 31113.1 Hz**
[[10]](https://problemkaputt.de/2k6specs.htm). When the AUDC distortion selects
the CPU-clock-based modes (values 12-15), the source clock is the **1.19 MHz CPU
clock / 114** instead
[[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html)
[[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide).

> Contested/rounded claim flagged: the audio clock is ~31.4 KHz per
> Stolberg/biglist/Problem-Kaputt, but the *Stella Programmer's Guide* says
> "~30 KHz." Implement the precise 3.579545 MHz / 114 (and CPU/114 for the
> CPU-clocked distortions) and treat the guide's 30 KHz as a rounding.

### 6.2 The polynomial-counter synthesis model

Each channel is two stacked stages: a **clock divider** (gated by AUDF) feeding
a **polynomial shift register** whose tap length is chosen by AUDC. The TIA uses
shift registers of length **4-bit (period 15), 5-bit (period 31), and 9-bit
(period 511)** to make pure tones and pseudo-random noise
[[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html).
Stolberg's reverse-engineered table maps each of the 16 AUDC values to a
specific shift-register configuration
[[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide):

| AUDC | Distortion (waveform / poly behavior) | Source clock |
| ---- | -------------------------------------- | ------------ |
| 0 | Constant HIGH (set-to-1 / used for 4-bit sample-by-volume output) | color/114 |
| 1 | 4-bit poly | color/114 |
| 2 | 4-bit poly clocked by div-15 ("465-bit" composite) | color/114 |
| 3 | 5-bit poly -> 4-bit poly composite | color/114 |
| 4 | Pure tone, divide-by-2 (square) | color/114 |
| 5 | Pure tone, divide-by-2 (square) | color/114 |
| 6 | Divide-by-31 pure tone | color/114 |
| 7 | 5-bit poly -> divide-by-31 | color/114 |
| 8 | 9-bit poly (white noise) | color/114 |
| 9 | 5-bit poly | color/114 |
| 10 | Divide-by-31 pure tone | color/114 |
| 11 | Constant HIGH (set-to-1 / sample mode) | color/114 |
| 12 | Pure tone, divide-by-6 (square) | CPU/114 |
| 13 | Pure tone, divide-by-6 (square) | CPU/114 |
| 14 | Divide-by-93 pure tone | CPU/114 |
| 15 | 5-bit poly -> divide-by-93 | CPU/114 |

(The exact per-value semantics are debated at the bit level — Stolberg notes
values 0xA/0xB behave "distinctly from the Stella manual due to alignment
between clock and data source frequencies"
[[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html). The
practical, widely-cited "8 useful timbres" subset — saw, engine, square, bass,
"pitfall" log, white noise, lead, buzz — is given in the qotile music guide
[[14]](https://www.qotile.net/files/2600_music_guide.txt).) The reference
implementation to match is Ron Fries' TIASOUND model, which Stolberg's table is
derived from and which Stella uses
[[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide).

Output is non-linear by volume: `AUDV` "pull[s] down on the audio output pad
with 16 selectable impedance levels," and the two channels mix
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

---

## 7. Technical deep-dive: the RIOT (MOS 6532)

The RIOT (RAM-I/O-Timer; PIA in the *Stella Programmer's Guide*) is a standard
MOS 6532, **independent of the TIA**
[[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html):

- **128 bytes of RAM** at $80-$FF — the console's only general-purpose RAM
  (CPU stack lives here too)
  [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[10]](https://problemkaputt.de/2k6specs.htm).
- **Two 8-bit I/O ports:**
  - `SWCHA` ($280) — Port A: the two joystick directionals (and paddle/keypad
    multiplexing); `SWACNT` ($281) is its data-direction register.
  - `SWCHB` ($282) — Port B: console switches (Reset, Select, Color/B&W, the two
    difficulty switches); `SWBCNT` ($283) DDR (hardwired input)
    [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
    [[10]](https://problemkaputt.de/2k6specs.htm).
- **Interval timer.** Write the count to one of four prescaler-selecting
  addresses: `TIM1T` ($294, 1 cycle/tick = 838 ns), `TIM8T` ($295, 8 cycles =
  6.7 us), `TIM64T` ($296, 64 cycles = 53.6 us), `T1024T` ($297, 1024 cycles =
  858.2 us). Read the current value at `INTIM` ($284) and the status at `INSTAT`
  ($285, undocumented). After hitting 0 the timer holds 0 for one interval, then
  wraps to $FF and decrements once per CPU cycle (so software can measure
  overshoot) [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
  [[10]](https://problemkaputt.de/2k6specs.htm). This timer is the 2600's
  substitute for a frame interrupt.

Because the 6507 has no interrupts, the timer cannot fire an IRQ; software polls
`INTIM`. The emulator's RIOT advances on the CPU clock, in parallel with the
TIA's color clock.

---

## 8. Technical deep-dive: cartridges and bankswitching (with tiering)

The 6507's 4 KB cart window ($1000-$1FFF in the decoded map) forces
bankswitching for anything larger than 4 KB. Mappers work by **hotspot
addresses**: a read or write to a magic address selects a bank. Some add on-cart
RAM; a few add a coprocessor. The canonical enumerations are Kevin Horton's
*Cart Information / bankswitch* doc, the AtariAge `bankswitch_sizes` list, and
Stella's own supported-type list
[[4]](http://www.classic-games.com/atari2600/bankswitch.html)
[[4a]](https://www.atariage.com/2600/programming/bankswitch_sizes.txt)
[[12]](https://stella-emu.github.io/docs/index.html).

### 8.1 Enumerated scheme catalog with suggested tier

| Scheme | Size | Hotspots / mechanism | RAM / coproc | Tier |
| ------ | ---- | -------------------- | ------------ | ---- |
| 2K | 2 KB | none (data repeats in 4K window) | - | Core |
| 4K | 4 KB | none (single bank) | - | Core |
| CV (Commavid) | 2 KB | fixed + 1 KB on-cart RAM | +1 KB RAM | Curated |
| F8 | 8 KB | $1FF8/$1FF9 select 2x4K | - | Curated |
| F6 | 16 KB | $1FF6-$1FF9 select 4x4K | - | Curated |
| F4 | 32 KB | $1FF4-$1FFB select 8x4K | - | Curated |
| F0 / 3F-variants | 64 KB | $1FF0 increments / 16x4K | - | BestEffort |
| FA / CBS RAM Plus | 12 KB | $1FF8/9/A select 3x4K | +256 B RAM | Curated |
| F8SC/F6SC/F4SC (Superchip) | 8/16/32 KB | F-series + Superchip | +128 B RAM | Curated |
| FE (Activision) | 8 KB | $01FE/$01FF via JSR/RTS stack frame | - | BestEffort |
| E0 (Parker Bros) | 8 KB | $1FE0-$1FF7 select four 1K slices | - | BestEffort |
| E7 (M-Network) | 16 KB | $1FE0-$1FEB, eight 2K banks | +2 KB RAM | BestEffort |
| 3F (Tigervision) | up to 512 KB | `STA $3F` with A=bank (low 2K window) | - | BestEffort |
| 3E (Tigervision+RAM) | var | `STA $3E` RAM-bank / `STA $3F` ROM-bank | +RAM | BestEffort |
| UA (UA Ltd.) | 8 KB | $0220/$0240 hotspots | - | BestEffort |
| 0840 (EconoBank) | 8 KB | $0800/$0840 hotspots | - | BestEffort |
| EF / EFSC | 64 KB | $1FE0-$1FEF, 16x4K (+SC RAM) | optional RAM | BestEffort |
| BF / BFSC | 256 KB | CPUWIZ 64x4K (+SC RAM) | optional RAM | BestEffort |
| DF / DFSC | 128 KB | CPUWIZ 32x4K (+SC RAM) | optional RAM | BestEffort |
| SB | 128-256 KB | "Superbank" $0800-$083F | - | BestEffort |
| X07 | 64 KB | AtariAge custom | - | BestEffort |
| 4A50 | up to 128 KB | complex r/w hotspots + RAM | +RAM | BestEffort |
| AR (Supercharger) | 6 KB RAM | tape/audio load, 3x2K RAM banks | RAM-based | BestEffort |
| DPC (Pitfall II) | 10 KB ROM | custom Display Processor Chip | coprocessor | BestEffort |
| DPC+ | var | Harmony/Melody ARM emulates DPC+ | ARM coproc | BestEffort |
| CDF / CDFJ / CDFJ+ | var | Harmony/Melody ARM | ARM coproc | BestEffort |

Sources for the scheme list and sizes/hotspots:
[[4]](http://www.classic-games.com/atari2600/bankswitch.html)
[[4a]](https://www.atariage.com/2600/programming/bankswitch_sizes.txt)
[[12]](https://stella-emu.github.io/docs/index.html).

### 8.2 Notes on the special carts

- **FE (Activision Robot Tank / Decathlon)** is unusual: it does not use a
  dedicated hotspot, but watches the stack frame written during `JSR`/`RTS`
  ($01FE/$01FF), making bank selection depend on the call instruction — a known
  emulator gotcha [[4]](http://www.classic-games.com/atari2600/bankswitch.html).
- **DPC** is David Crane's custom **Display Processor Chip** in Pitfall II — a
  true coprocessor adding "two additional hardware sound channels, music
  sequencing, a hardware RNG, and a graphics streaming capability." **Pitfall II
  was the only commercial game to use it**
  [[15]](https://www.bigmessowires.com/2023/01/23/atari-2600-hardware-acceleration/).
- **DPC+ / CDF / CDFJ** are modern Harmony/Melody cart formats backed by an
  **ARM microcontroller** that runs ARM code alongside the 6507; faithful
  emulation needs an ARM core, so these are deep BestEffort
  [[15]](https://www.bigmessowires.com/2023/01/23/atari-2600-hardware-acceleration/)
  [[12]](https://stella-emu.github.io/docs/index.html).

### 8.3 Honesty-gate policy (mirrors RustyNES)

Tier the mappers Core / Curated / BestEffort and enforce a CI honesty gate (as
RustyNES does with `mapper_tier_honesty.rs`): **a BestEffort scheme must never
be allowed to back the accuracy oracle.** Only Core/Curated mappers gate the
"AccuracyCoin"-equivalent battery; BestEffort carts get boot-smoke / screenshot
coverage only and are explicitly labeled approximate.

---

## 9. State-of-the-art and prior art

| Emulator | Lang / license | What it gets right | Notes |
| -------- | -------------- | ------------------ | ----- |
| **Stella** | C++ / GPLv2 | The de-facto reference: cycle-exact TIA (from the 6502.ts core), cycle-exact audio, every mapper incl. ARM coproc carts, NTSC/PAL/SECAM, debugger w/ Distella, Time Machine rewind | Primary oracle for behavior; its source is the only doc for CDF/DPC+ internals [[12]](https://stella-emu.github.io/docs/index.html) |
| **Gopher2600** | Go / GPLv3 | "No known problems with 6507/TIA/RIOT emulation"; supports common TIA revisions; color-clock-level rewind debugger; clean conceptual memory-bus architecture via Go interfaces | Excellent architecture docs (JetSetIlly wiki) — a model for Rusty2600's bus design [[9]](https://github.com/JetSetIlly/Gopher2600) [[9a]](https://github.com/JetSetIlly/Gopher2600-Docs/wiki) |
| **z26** | C / GPL | Long-standing fast/accurate DOS-era core | Historical reference, many ROM-compat fixes |
| **ares** | C++ / ISC | higan/bsnes-lineage accuracy framework; has an original 2600 core | 2600 core marked "Experimental," behind Stella/MAME [[16]](https://emulation.gametechwiki.com/index.php/Atari_2600_emulators) |
| **Javatari** | Java / JS | Clean cross-platform web core, netplay, good compat | Good reference for a portable/wasm target [[17]](https://github.com/ppeccin/javatari) |

For Rusty2600: **Stella is the behavioral oracle** (when docs and Stella
disagree about a quirk, prefer Stella + a confirming test ROM), and
**Gopher2600's conceptual memory-bus interfaces** are the cleanest architectural
template to adapt to Rust (and to the RustyNES "Bus owns everything mutable"
pattern).

---

## 10. Principal engineering challenges

1. **The beam-racing render model.** No framebuffer; the TIA synthesizes pixels
   live and the CPU mutates TIA registers mid-scanline. The core must run TIA
   and CPU in tight color-clock lockstep (1 CPU cycle = 3 color clocks) so a
   register write lands on the exact pixel
   [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt). This is *the*
   accuracy determinant.
2. **Cycle-exact RESPx positioning.** The 9-CLK reset pipeline plus NUSIZ copy
   offsets must be modeled exactly, including the case where the strobe lands at
   different beam positions [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
3. **HMOVE timing and the comb/black-bar quirk.** The 8-clock HBLANK extension,
   the per-object clock-stuffing decrement-from-15-every-4-CLK, and partial-comb
   artifacts from late HMOVE must all reproduce
   [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
4. **Undocumented-opcode + decimal-mode correctness.** The full NMOS illegal
   opcode set and working BCD mode must pass Klaus Dormann + ProcessorTests, even
   though most commercial games avoid the illegals
   [[3]](https://github.com/Klaus2m5/6502_65C02_functional_tests)
   [[8]](https://www.nesdev.org/wiki/CPU_unofficial_opcodes).
5. **Bankswitch breadth + honesty gating.** ~25 mapper families, several with
   on-cart RAM and a few with coprocessors (DPC, ARM-based DPC+/CDF). The tiering
   plus a CI honesty gate keeps BestEffort carts out of the oracle (Section 8.3)
   [[4]](http://www.classic-games.com/atari2600/bankswitch.html)
   [[15]](https://www.bigmessowires.com/2023/01/23/atari-2600-hardware-acceleration/).
6. **Audio poly-counter exactness.** Reproduce the 16 AUDC distortion modes,
   the dual-clock (color/114 vs CPU/114) derivation, and the 4/5/9-bit shift
   registers bit-exactly against the TIASOUND/Stella model
   [[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide)
   [[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html).
7. **Region timing as data.** NTSC/PAL/SECAM differ in line counts, color
   clock, palette, and the SECAM AUDC-off quirk — all parameterized, not forked
   [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html)
   [[13]](https://www.randomterrain.com/atari-2600-memories-tutorial-andrew-davie-11.html).

---

## 11. Standards / compliance: the test-ROM oracle

Map these onto the RustyNES "AccuracyCoin" honesty-gated accuracy battery:

- **Klaus Dormann 6502/65C02 functional tests** — `6502_functional_test`
  (all valid NMOS opcodes/addressing) + `6502_decimal_test` (Bruce Clark's BCD
  test). The authoritative CPU-correctness oracle for the 6507 core
  [[3]](https://github.com/Klaus2m5/6502_65C02_functional_tests).
- **SingleStepTests / ProcessorTests (Tom Harte et al.)** — 10,000 JSON vectors
  *per opcode* with initial/final CPU+RAM state and cycle-by-cycle bus activity
  (read/write per cycle); the per-opcode + per-cycle golden, covering documented
  and undocumented opcodes. Use the `6502` set (NMOS, decimal active) rather than
  the `nes6502` set (which ignores decimal)
  [[7]](https://github.com/SingleStepTests/ProcessorTests/blob/main/6502/README.md).
- **TIA timing / draw test ROMs** — beam-racing, RESPx, HMOVE-comb, and
  collision behavior verified against Stella + reference framebuffers (the 2600
  homebrew/test-ROM corpus; Stella's own regression suite). When docs and a
  passing test ROM disagree, the ROM wins (RustyNES rule).
- **Stella regression/integration corpus** — Stella is the behavioral reference
  for mapper, audio, and per-quirk behavior
  [[12]](https://stella-emu.github.io/docs/index.html).

Tier + honesty-gate exactly as RustyNES: only Core/Curated subsystems and
mappers gate the battery; BestEffort gets boot-smoke/screenshot coverage and is
labeled approximate.

---

## 12. Architecture options (not pre-committed)

Four candidate shapes, to be decided in the design phase:

1. **Color-clock-master lockstep (recommended, mirrors RustyNES PPU-master).**
   The scheduler advances **one TIA color clock per tick**; the CPU steps once
   every 3rd color clock; the RIOT steps on the CPU clock. A `Bus` owns the TIA,
   RIOT, and cart/mapper; the CPU borrows `&mut Bus`. This is the most direct
   analog of RustyNES's "PPU is the master clock / Bus owns everything mutable"
   architecture and the cleanest fit for the beam-racing model.
2. **CPU-master with TIA catch-up.** Run the CPU per-instruction and "catch up"
   the TIA between bus accesses. Simpler loop, but mid-instruction TIA writes
   (the whole point of the 2600) force per-cycle catch-up anyway, eroding the
   benefit. Higher risk of subtle RESPx/HMOVE timing bugs.
3. **Per-cycle "one clock" unified scheduler (the future-proof option).** A
   single fractional master clock driving CPU (every 3 ticks) and TIA (every
   tick) with every CPU bus access placed on its exact cycle — the equivalent of
   the RustyNES v2.0 "Timebase" goal. Most accurate; most upfront work.
4. **Gopher2600-style conceptual memory buses.** Adopt narrow bus traits
   (`TiaBus`, `RiotBus`, `CartBus`) over a central memory model, decoupling each
   chip's view — strong for testability and a clean Rust crate split
   [[9a]](https://github.com/JetSetIlly/Gopher2600-Docs/wiki).

Recommendation to carry into design: **Option 1 as the substrate, structured
with Option 4's bus traits**, leaving Option 3 as a later "Timebase"-style
refactor — directly paralleling the RustyNES lineage.

---

## 13. External dependencies (candidate)

- Rust workspace mirroring RustyNES crate split:
  `rusty2600-{cpu,tia,riot,cart,core,frontend,test-harness}`.
- Frontend (matching RustyNES): `winit` + `wgpu` + `cpal` + `egui`.
- `no_std` + `alloc` for the chip stack (CPU/TIA/RIOT/cart), `std` only in the
  frontend.
- Test harness consuming the Klaus Dormann ROMs (assembled `.bin`) and the
  ProcessorTests JSON vectors.
- Optional later: an ARM-thumb interpreter for DPC+/CDF carts (deep BestEffort).

No third-party emulation core is needed; Stella (GPLv2) is a behavioral
reference only and must not be linked or copied (license incompatibility with a
permissive MIT/Apache dual-license).

---

## 14. Open questions and contested claims

1. **Audio clock value.** ~31.4 KHz (3.579545 MHz / 114) per
   Stolberg/biglist/Problem-Kaputt vs the *Stella Guide*'s "~30 KHz." Resolved
   in favor of 3.579545 MHz / 114 (and CPU/114 for AUDC 12-15); the guide is
   rounding [[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html)
   [[6]](https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide)
   [[10]](https://problemkaputt.de/2k6specs.htm).
2. **Exact AUDC distortion semantics for 0xA/0xB.** Stolberg notes these differ
   from the Stella manual due to clock/data alignment; bit-exact behavior should
   be pinned against the TIASOUND/Stella implementation, not prose docs
   [[5]](https://www.biglist.com/lists/stella/archives/199703/msg00207.html).
3. **TIA chip revisions.** Multiple TIA revisions (TIA-1A etc.) introduce subtle
   variations (notably in playfield/HMOVE edge cases and color); Gopher2600
   models several. Decide which revision Rusty2600 defaults to and whether to
   parameterize [[9]](https://github.com/JetSetIlly/Gopher2600).
4. **RESPx fine-timing edge cases.** The 9-CLK pipeline interacts with HMOVE and
   with writes during HBLANK; some sub-pixel cases need direct test-ROM
   verification against Stella rather than the prose in Towers' notes
   [[1]](https://www.atarihq.com/danb/files/TIA_HW_Notes.txt).
5. **Undocumented-opcode reliance.** Confirm whether any in-scope commercial ROM
   actually depends on illegal opcodes (developer lore says they were avoided);
   regardless, implement them for ProcessorTests conformance
   [[8a]](https://www.masswerk.at/nowgobang/2021/6502-illegal-opcodes).
6. **PAL "Picture 192 vs 228" discrepancy.** AtariAge prose says PAL is "228
   picture lines" within 312 total; Problem-Kaputt lists 3+45+228+36=312. NTSC's
   192 visible is firm; PAL visible-line budget should be confirmed against a
   PAL test ROM [[10]](https://problemkaputt.de/2k6specs.htm)
   [[2]](https://www.atariage.com/2600/programming/2600_101/docs/stella.html).

---

## 15. Source manifest

Grouped by tier. Tier-1 = primary hardware/programmer references and canonical
test suites; Tier-2 = strong secondary (emulator docs, reverse-engineering
write-ups, dev-community references). Dates are last-known publication/access
(accessed 2026-06-24).

Citation IDs (`[N]`) are stable and match the inline links above; some IDs are
intentionally non-contiguous within a tier (e.g. 9 is Tier 2), and `4a/8a/9a`
are sub-references of their parent source.

### Tier 1 — primary sources

- **[1]** Andrew Towers, "Atari 2600 TIA Hardware Notes" v1.0 (2003-03-06).
  <https://www.atarihq.com/danb/files/TIA_HW_Notes.txt>
- **[2]** Steve Wright (upd. Darryl May), "Stella Programmer's Guide"
  (1979/1988), unofficial HTML.
  <https://www.atariage.com/2600/programming/2600_101/docs/stella.html>
- **[3]** Klaus Dormann, "6502/65C02 functional + decimal tests" (incl. Bruce
  Clark's decimal test).
  <https://github.com/Klaus2m5/6502_65C02_functional_tests>
- **[4]** Kevin Horton, "Atari 2600 Bankswitching Methods / Cart Information."
  <http://www.classic-games.com/atari2600/bankswitch.html>
- **[4a]** AtariAge, "bankswitch_sizes.txt."
  <https://www.atariage.com/2600/programming/bankswitch_sizes.txt>
- **[5]** "[stella] The TIA sound hardware" (Stella mailing list, 1997) —
  poly-counter / clock-divisor model.
  <https://www.biglist.com/lists/stella/archives/199703/msg00207.html>
- **[6]** Eckhard Stolberg, "Atari 2600 VCS Sound Frequency and Waveform Guide"
  (derived from Ron Fries' TIASOUND).
  <https://7800.8bitdev.org/index.php/Atari_2600_VCS_Sound_Frequency_and_Waveform_Guide>
- **[7]** SingleStepTests / ProcessorTests (Tom Harte et al.), 6502 README —
  per-opcode golden vectors w/ cycle bus activity.
  <https://github.com/SingleStepTests/ProcessorTests/blob/main/6502/README.md>
- **[8]** NESdev Wiki, "CPU unofficial opcodes" (NMOS illegal opcode reference).
  <https://www.nesdev.org/wiki/CPU_unofficial_opcodes>
- **[10]** Martin Korth (Problem-Kaputt), "Atari 2600 Specifications" (2k6specs).
  <https://problemkaputt.de/2k6specs.htm>
- **[11]** Wikipedia, "MOS Technology 6507" (13 address lines, no IRQ/NMI,
  die-identical to 6502). <https://en.wikipedia.org/wiki/MOS_Technology_6507>
- **[12]** Stella — official emulator documentation (supported cart/bankswitch
  list, GPLv2, behavioral reference).
  <https://stella-emu.github.io/docs/index.html>

### Tier 2 — strong secondary / community references

- **[8a]** masswerk.at, "6502 'Illegal' Opcodes Demystified" (2021) — incl.
  2600 developer-avoidance lore.
  <https://www.masswerk.at/nowgobang/2021/6502-illegal-opcodes>
- **[9]** JetSetIlly, "Gopher2600" (Go, GPLv3) — high-accuracy core +
  architecture. <https://github.com/JetSetIlly/Gopher2600>
- **[9a]** Gopher2600 Docs wiki (conceptual memory-bus architecture).
  <https://github.com/JetSetIlly/Gopher2600-Docs/wiki>
- **[13]** Andrew Davie, "Atari 2600 Programming for Newbies — Session 11:
  Colorful Colors" (region palette differences).
  <https://www.randomterrain.com/atari-2600-memories-tutorial-andrew-davie-11.html>
- **[14]** qotile.net, "Atari 2600 Music and Sound Programming Guide" (the
  practical 8-timbre AUDC subset).
  <https://www.qotile.net/files/2600_music_guide.txt>
- **[15]** Big Mess o' Wires, "Atari 2600 Hardware Acceleration" (2023) — DPC /
  DPC+ / CDF coprocessor cart history.
  <https://www.bigmessowires.com/2023/01/23/atari-2600-hardware-acceleration/>
- **[16]** Emulation General Wiki, "Atari 2600 emulators" (Stella/MAME/ares/z26
  landscape, ares "Experimental" status).
  <https://emulation.gametechwiki.com/index.php/Atari_2600_emulators>
- **[17]** ppeccin, "Javatari" (Java/JS web 2600 emulator, netplay).
  <https://github.com/ppeccin/javatari>

Primary (Tier 1) source count: 12 (IDs 1-8, 10-12). Secondary (Tier 2): 7
(IDs 8a, 9, 9a, 13-17). Total distinct sources cited: 19+ (several with
multiple fetched mirrors/sub-pages).
