# Glossary — Atari 2600 / VCS

Domain terms for Rusty2600. References: `ref-docs/research-report.md` §4–8;
`docs/tia.md`, `docs/riot.md`, `docs/cart.md`, `docs/scheduler.md`.

| Term | Meaning |
|---|---|
| **VCS** | Video Computer System — Atari's 1977 console, later branded "2600." |
| **Color clock** | The master clock: NTSC 3.579545 MHz (the colour subcarrier). One color clock = one visible pixel. |
| **CPU cycle** | One 6507 machine cycle = 3 color clocks (~1.19 MHz). 76 per scanline. |
| **Beam racing** | Composing the picture by rewriting TIA registers mid-scanline in lockstep with the electron beam, since the TIA has no framebuffer. |
| **TIA** | Television Interface Adaptor — the VCS's custom video **and** audio chip. |
| **RIOT** | RAM-I/O-Timer (MOS 6532) — 128 B RAM + two I/O ports + interval timer. Called "PIA" in the Stella guide. No audio. |
| **6507** | The CPU: a 6502 in a 28-pin package — 13 address pins (8 KiB), no IRQ/NMI pins. |
| **Scanline** | One horizontal line = 228 color clocks (68 HBLANK + 160 visible) = 76 CPU cycles. |
| **HBLANK** | Horizontal blank — the first 68 color clocks of a line, not displayed. |
| **VSYNC / VBLANK** | Software-generated vertical sync ($00) / vertical blank ($01); the program toggles them at the right line counts. |
| **WSYNC** | Strobe ($02) that pulls RDY low, halting the CPU until the next scanline — how software locks to the beam. |
| **RDY** | The CPU ready line; the TIA drives it low on WSYNC. The scheduler freezes the CPU while it is asserted. |
| **RESPx** | Strobe registers (RESP0/1, RESM0/1, RESBL) that set an object's horizontal position to wherever the beam is at the write (9-CLK reset pipeline). |
| **HMOVE** | Strobe ($2A) that applies the 4-bit signed motion registers (HMxx) by clock-stuffing during HBLANK. |
| **HMOVE comb** | The black left-edge bar / partial "teeth" artifact from HMOVE extending HBLANK by 8 color clocks. |
| **HMCLR** | Strobe ($2B) that zeroes all five motion registers at once. |
| **Playfield (PF)** | The 20-bit background (PF0/PF1/PF2) drawn across the left half; CTRLPF controls reflect/score/priority/ball-size. |
| **Player / GRP** | The two 8-bit sprite graphics (GRP0/GRP1). |
| **Missile / Ball** | Single-pixel-wide movable objects (ENAM0/1, ENABL). |
| **NUSIZ** | Number-size register (NUSIZ0/1): object copies + player/missile width; read every graphics CLK. |
| **VDELP / VDELBL** | Vertical-delay flags selecting the "new" vs "old" graphics copy (alternating-line sprite trick). |
| **Collision (CXxx)** | The 15 hardware-latched object-overlap pairs (CXM0P..CXPPMM), cleared by CXCLR. |
| **COLUPx / COLUBK** | The 7-bit colour/luma registers (4-bit hue + 3-bit luma); region-dependent palette. |
| **AUDC / AUDF / AUDV** | The two TIA audio channels: control/distortion (4-bit), frequency divider (5-bit), volume (4-bit). |
| **Poly counter** | The 4/5/9-bit polynomial shift registers the TIA audio uses for tones and noise. |
| **SWCHA / SWCHB** | RIOT Port A (joysticks) / Port B (console switches). |
| **INTIM / INSTAT** | The RIOT interval-timer current value / status (read); written via TIM1T/8T/64T/1024T. |
| **Hotspot** | A magic cartridge address whose read/write triggers a bank switch. |
| **Tier** | Honesty marker on a board: Core / Curated (accuracy-gated) / BestEffort (never gated). |
| **Superchip / SC** | On-cart 128-byte RAM added to an F-series scheme (F8SC/F6SC/F4SC). |
| **DPC** | David Crane's Display Processor Chip coprocessor (Pitfall II only). |
| **AccuracyCoin (-equivalent)** | The honesty-gated accuracy-battery pass-rate metric, ported from RustyNES. |
| **Open bus** | The last value driven on the data bus, returned for reads of unmapped / write-only addresses. |


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
