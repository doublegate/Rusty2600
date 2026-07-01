# Architecture — Rusty2600 (load-bearing facts)

These cross-cutting decisions span multiple files. This doc is the **hub**;
the per-chip docs (`cpu.md`, `tia.md`, `riot.md`, `cart.md`) and `scheduler.md`
are the spokes. Read this first — reading a chip spec without these facts in
mind will mislead.

References: `ref-docs/research-report.md` §1 (executive summary), §12
(architecture options); `docs/scheduler.md`; `docs/adr/0001`, `docs/adr/0004`.

## The eight load-bearing facts

### 1. The TIA color clock is the master, and the scheduler is lockstep

The scheduler advances **one TIA color clock per `tick_one_color_clock()`**; the
6507 CPU advances on **every third** color clock (the VCS divides the 3.579545
MHz color clock by 3 to make the CPU's φ0). This is **lockstep**, not catch-up.
It is the central architectural choice and the reason mid-scanline TIA register
writes — the whole point of the 2600 — compose the very next dot without any
per-quirk patch. The 2600 is the **purest** beam-racing console, so this maps
the RustyNES "PPU is the master clock" model more directly than any other
target (per ref-docs/research-report.md §12, Option 1). See `docs/scheduler.md`
and `docs/adr/0001`.

### 2. The Bus owns everything mutable

`rusty2600-core::Bus` holds the TIA, the RIOT, the cartridge (via the boxed
`Board`), and the open-bus latch. The CPU borrows `&mut Bus` during `tick()`.
Per the TetaNES postmortem, this single choice avoids the borrow-checker fight
that the alternative ("CPU holds the TIA, but the TIA also needs the CPU bus")
creates. The video and audio paths each see a narrower trait (`VideoBus`,
`AudioBus`) for only what they need.

### 3. There is no separate work RAM — system RAM lives in the RIOT

Unlike the NES (which has 2 KiB of CPU WRAM on its own bus), the 2600 has **no
dedicated work RAM**. The console's only general-purpose RAM is the RIOT's **128
bytes** (`$80..=$FF`), and the CPU stack overlaps it. The `Bus` therefore carries
**no `wram` field**. State this when porting NES code: the WRAM owner does not
exist here. Per ref-docs/research-report.md §7.

### 4. The TIA does BOTH video AND audio; the RIOT does neither

Audio synthesis lives in the **`rusty2600-tia`** crate — the TIA is a
video **and** audio chip (two channels: `AUDC`/`AUDF`/`AUDV` × 2, polynomial-
counter synthesis). The **`rusty2600-riot`** crate is RAM + I/O ports + interval
timer **only**; it has no sound hardware. `rusty2600-core`'s `AudioBus` draws
audio **from the TIA** (`Bus::audio_sample()` → `self.tia.audio.sample()`). Do
not look for audio in the RIOT. Per ref-docs/research-report.md §6 (TIA audio),
§7 (RIOT).

### 5. The workspace dependency graph is one-directional

`rusty2600-cpu` depends on nothing console-specific (`core` + `alloc` +
`bitflags`). `rusty2600-tia` depends on `rusty2600-cart` only (its memory bus
for the cart-mediated reads the video path needs) — this is the **one declared
exception** to one-directionality, not a violation of it. `rusty2600-riot` is
fully independent. `rusty2600-cart` is independent. `rusty2600-core` ties them
together and re-exports their public types. Result: each chip (other than the
declared `tia`→`cart` edge) is fuzzable and benchmarkable in isolation. Adding
any *other* cross-chip dependency breaks this invariant — don't.

### 6. Board (bankswitch) logic lives in the cart crate, not the TIA

The cartridge `Board` trait (`rusty2600-cart`) decodes every CPU access in the
`$1000..=$1FFF` window and runs its own hotspot bank logic on both reads and
writes. Coprocessor carts (DPC, DPC+) advance on the default-no-op `Board::tick`
/ `Board::tick_coprocessor` hooks the scheduler drives. The TIA never knows the
bankswitch scheme. See `docs/cart.md`.

### 7. Determinism is a hard contract

Same seed + ROM + input sequence ⇒ bit-identical scanline output and audio. The
per-power-on CPU/color-clock phase alignment comes from a **seeded** PRNG (the
`System::new(seed)` phase field), never the OS RNG. Reset preserves alignment.
This is required for save-state round-trip, regression goldens, TAS replay, and
netplay rollback. Rate control and run-ahead live in the **frontend**, never in
the core synthesis. See `docs/adr/0004`.

### 8. The frontend is an always-on egui shell, not a bare window

`rusty2600-frontend` is `winit` + `wgpu` + `cpal` + `egui`, and egui runs every
frame: a persistent menu bar + status bar + tabbed Settings, with toggleable
debugger panels on top. The shell never holds the emu lock inside the egui
closure — menu interactions return a `MenuAction` dispatched after the egui
pass. On native the emulator runs on a dedicated thread; the winit thread only
does UI + present. See `docs/frontend.md`.

## The crate map

| Crate | Chip / role | Depends on | Audio? | RAM? |
|---|---|---|---|---|
| `rusty2600-cpu` | MOS 6507 | core + alloc + bitflags | no | no |
| `rusty2600-tia` | TIA — video **+ audio** | `rusty2600-cart` | **yes** | no |
| `rusty2600-riot` | MOS 6532 — RAM + I/O + timer | (independent) | **no** | **yes (128 B)** |
| `rusty2600-cart` | Bankswitch boards (tiered) | (independent) | no | on-cart only |
| `rusty2600-core` | `Bus` + `System` scheduler | all of the above | draws from TIA | no `wram` |
| `rusty2600-frontend` | egui shell (`std`, `unsafe`) | `rusty2600-core` | cpal sink | — |
| `rusty2600-cheevos` | RetroAchievements FFI (`std`, `unsafe` — vendors `rcheevos`) | (independent; no chip dep) | — | — |
| `rusty2600-test-harness` | accuracy oracle + honesty gate | `rusty2600-core`, `rusty2600-cpu` | — | — |

## Test ROMs are the spec

When the docs and a passing test ROM disagree, the ROM wins — the docs get
updated. The Klaus Dormann functional/decimal suite, the SingleStepTests/
ProcessorTests vectors, the TIA timing ROMs, and the Stella regression corpus
are the closed-form definition of "cycle-exact." Stella is the behavioral
oracle for per-quirk behaviour. See `docs/testing-strategy.md` and
ref-docs/research-report.md §11.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
