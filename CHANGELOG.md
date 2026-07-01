# Changelog

All notable changes to Rusty2600 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.6.0] - 2026-07-01 - "Coprocessor"

A new `rusty2600-thumb` crate: a real ARM7TDMI Thumb-1 interpreter, the
substrate the DPC+/CDF/CDFJ/CDFJ+ coprocessor family will wire into.

### Added

- **`rusty2600-thumb`** (new crate, `no_std + alloc`,
  `#![forbid(unsafe_code)]`, zero dependency on any other Rusty2600 crate):
  a real ARM7TDMI Thumb-1 interpreter ported from Gopher2600's Go
  implementation (`hardware/memory/cartridge/arm/`), not Stella's C++
  `Thumbulator` — its memory-safety-first style maps far more naturally
  onto this project's own `#![forbid(unsafe_code)]` house style.
  - `registers.rs` — the 16-register file, with the "stored PC is always
    `next fetch address + 2`" pipeline bookkeeping ported faithfully from
    the reference.
  - `status.rs` — N/Z/C/V flags (`bitflags`-backed), `condition()` for all
    14 real Thumb-1 branch condition codes.
  - `memory.rs` — the `ThumbMemory` trait (mirrors Gopher2600's
    `SharedMemory`), the generic seam a future `Board` implements, plus a
    typed `Fault` enum (illegal/unimplemented/null/misaligned) returned as
    a `Result`, not a Go-style panic-and-log.
  - `cycles.rs` + `mam.rs` — the N/S/I cycle model and MAM
    (Memory Accelerator Module) prefetch-latch approximation, ported
    faithfully and documented as an approximation (matching the
    reference's own admitted uncertainty about some of its constants).
  - `thumb.rs` — the actual Thumb-1 decode/execute, all 19 instruction-format
    classes from the ARM7TDMI Data Sheet, plus 27 hand-authored conformance
    tests exercising every format class (shifts including shift-by-zero
    edge cases, ALU operations, hi-register ops + `BX`-to-return, every
    load/store addressing mode, push/pop including LR/PC, multiple
    load-store including the base-register-in-list edge case, branches,
    and `SWI` faulting rather than panicking).

### Notes

- **This release lands the interpreter core only.** `rusty2600-thumb` is
  not yet wired into any `rusty2600-cart` `Board`/`Cartridge` variant — no
  `detect()` change, no new bankswitch scheme available to users yet. The
  `v1.6.x` patch train wires DPC+, then CDF, then CDFJ/CDFJ+ into `detect()`
  one family at a time (`T-0401-006`), each supplying its own `ThumbMemory`
  implementation (register map, RNG/timer peripherals,
  `tick_coprocessor()` driving `Arm7Tdmi::step()`). See `docs/thumb.md` for
  the full architecture and scope.
- **Deliberately out of scope for this crate**: Thumb-2 (32-bit encoding)
  and ARMv7-M/Cortex-M0 support (the Harmony/Melody boards this targets are
  ARM7TDMI/Thumb-1 only), the `rng`/`timer` peripheral packages and
  `architecture` register-map package (cartridge-specific, deferred to the
  `v1.6.x` wiring pass), a disassembler, and the `callfn`/`ARMinterrupt`
  direct-ARM32-call mechanism (an out-of-scope cartridge integration hook;
  reported as a fault here instead).
- The N/S/I cycle-stretching model is a genuinely approximate hardware
  timing model even in the reference implementation (float-based cycle
  counts; Gopher2600's own comments admit some constants are unverified)
  — this crate does not claim cycle-exactness for the coprocessor path,
  only a faithful port of the same approximation, consistent with this
  project's "never present approximate output as exact" rule.
- 217 tests passing workspace-wide (220 with `--features test-roms`; +27
  for the new `rusty2600-thumb` conformance suite), up from 190/193 at
  `[1.5.0]`.

## [1.5.0] - 2026-07-01 - "Full Catalog"

`Bank4A50`, the most stateful bankswitch scheme in the catalogue to date —
closing it to 23 of 25 schemes.

### Added

- **`Bank4A50`** (`crates/rusty2600-cart`, `T-0402-014`, BestEffort tier):
  three independently relocatable ROM/RAM segments (`$1000-$17FF` 2K,
  `$1800-$1DFF` 1.5K, `$1E00-$1EFF` 256B) plus a fixed 256B trailer
  (`$1F00-$1FFF`, always the last 256B of the tiled 128 KiB image), driven
  by a previous-access-gated hotspot state machine ported faithfully from
  Stella's `Cartridge4A50::checkBankSwitch`. Most hotspots only arm when the
  immediately preceding access read or wrote a value matching
  `(value & 0xe0) == 0x60` from a cart-window or RIOT-zero-page address —
  handled below `$1000` via `Board::snoop_read`/`snoop_write` (the same
  hooks 3F/3E/UA/0840/FE/SB/X07 already use for TIA/RIOT-mirrored-space
  bankswitching), plus a smaller in-window instance of the same check at
  `$1F00-$1FFF` inside `cpu_read`/`cpu_write`. `detect()` resolves it via
  `is_probably_4a50()` (ported from Stella's `CartDetector::isProbably4A50`)
  at the 64K/128K size branches: the scheme's own namesake `$4A50` at the
  NMI vector, falling back to a reset-vector-targets-a-`NOP $6Exx`/`$6Fxx`
  heuristic. 32/64 KiB dumps tile to fill the full 128 KiB image, matching
  Stella's own constructor.

### Notes

- **AR/Supercharger (`T-0402-015`) deliberately NOT attempted this
  release**, despite being staged alongside 4A50 in the release plan. Even
  its "fast-load" (ROM-image-only) mode — skipping the real tape-audio
  "sound-load" path entirely, which needs a WAV/multiload decoder and is
  clearly out of scope regardless — needs a bank-config decode, a
  delayed-write protocol keyed on 5 DISTINCT bus accesses (Stella tracks
  this via a global CPU-side access counter this crate has no equivalent
  of; would need to be reconstructed from `snoop_read`/`snoop_write`/
  `cpu_read`/`cpu_write` combined, since together they cover the whole
  6507 address bus), and a synthesized dummy 6502 BIOS stub whose exact
  bytes (Stella's `ourDummyROMCode`/`scrom.asm`) haven't been sourced yet.
  Substantially larger than 4A50 or any other scheme in this catalogue —
  staying its own separately-scoped follow-up rather than being rushed.
- Per Stella's own doc comment, 4A50 itself "hasn't been fully implemented,
  and may never be" even there (missing hi-res helper functions and
  `$1E00` page-wrap, and only one known test ROM exists for it). This port
  is an equally-scoped, faithful translation of exactly what Stella
  implements — not a superset — and stays `BestEffort` tier indefinitely.
- 190 tests passing workspace-wide (193 with `--features test-roms`; +7 for
  `Bank4A50`'s register-decode, boot-smoke, and `detect()` signature
  tests), up from 183/186 at `[1.4.0]`.

## [1.4.0] - 2026-07-01 - "Signal"

A composable post-process shader stack, plus the data model half of the
2600-appropriate HD-pack analog.

### Added

- **A composable shader stack** (new `rusty2600-gfx-shaders` crate +
  `rusty2600-frontend::shader_pass`): two real passes — `CrtScanline`
  (scanline darkening, computed from `@builtin(position)` with no extra
  uniform) and `CompositeArtifact` (an honestly-labeled horizontal
  chroma-bleed *approximation*, not a genuine composite-signal YIQ decode —
  the render pipeline doesn't currently carry raw palette indices through to
  this stage, so this is styled, not authoritative). `Gfx::present`
  replaces the direct `Gfx::blit` call site: an empty pass list (the
  default) calls the unchanged direct blit, preserving the byte-identical
  default-build output the `uv_scale` fix (`[1.1.0]`) established. A new
  Settings checkbox pair (`config.video.shader_passes`) toggles each pass,
  persisted like every other Settings widget since `[1.1.0]`'s
  `MenuAction::SaveConfig` fix.
- **`rusty2600-frontend::sprite_pack`** (`hd-pack` feature): the data model
  + manifest loader for a right-sized, 2600-appropriate HD-pack analog —
  replacement bitmaps keyed by `(GRPx value, NUSIZx copy mode)`, since a
  player/missile/ball's `GRPx` byte *is* its entire visual data (no
  tile/pattern-table/CRC-hash system needed, unlike a tile-based console).
  Manifests are plain TOML pointing at raw RGBA8 files — no new
  image-decoder dependency for this first cut.

### Notes

- **Sprite-pack rendering integration is honestly deferred.** The data
  model and loader above are real and tested, but splicing a loaded
  replacement bitmap into the live wgpu render path needs an object-ID mask
  threaded through the TIA first — the rendering pipeline currently
  flattens every object into one resolved-color `video_buffer` with no
  per-pixel "this dot came from P0" tag preserved. That's a genuine
  architectural addition, not a quick splice, so it's left as a documented
  follow-up rather than rushed or silently dropped.
- 183 tests passing workspace-wide (186 with `--features test-roms`; +2 for
  the shader-stack WGSL validation tests), up from 181/184 at `[1.3.0]`.
  The sprite-pack loader's 3 tests are gated behind `hd-pack` (off by
  default) and verified separately: `cargo test -p rusty2600-frontend
  --features hd-pack` — 70 passed (67 + the 3 new sprite-pack tests).

## [1.3.0] - 2026-07-01 - "Scope"

Debugger depth plus the long-deferred RetroAchievements achievement-list/
login/toast UI (`T-0802-005`).

### Added

- **A watch/conditional-breakpoint expression engine**
  (`rusty2600-frontend::debugger::expr` + `watch_panel`): a small grammar
  (`a == $42`, `[$80] != 0`, `scanline >= 192 && color_clock < 20`, register
  names `a x y s pc scanline color_clock frame`, `[addr]` memory peeks
  against RIOT RAM, hex `$..`/decimal literals, `== != <= >= < >`, `&&`/`||`)
  evaluated against a live `EvalContext`. Watches display live PASS/FAIL/
  ERROR; a "break" checkbox arms one as a real conditional breakpoint,
  checked every iteration of `MenuAction::DebugContinue`'s step loop
  against RIOT RAM read directly (not `Bus::peek`'s full-clone path, which
  would multiply badly across up to 1,000,000 iterations).
- **`rusty2600-core::WriteLog`** (`Bus::write_log`): an optional per-write
  log (scanline/color-clock-tagged, capped at 4,096 entries), disabled by
  default and `#[serde(skip)]` on `Bus` (debug-tooling state, never part of
  a save-state — see ADR 0007). Backs the new Events panel.
- **`debugger::event_panel`**: a per-scanline scatter of TIA register
  writes (`COLUP0/1`, `COLUPF`, `COLUBK`, `GRP0/1`, `HMOVE`, `RESP0`) — the
  TIA has no VRAM/nametables, so this *is* how "the picture" gets built,
  making timing quirks like the HMOVE comb visually debuggable.
- **`debugger::callstack`**: a live JSR/RTS call stack, tracked on every
  `DebugStep` (a single side-effect-free peek per click is cheap; tracking
  was deliberately left out of `DebugContinue`'s tight loop for the same
  clone-cost reason as the watch engine).
- **`debugger::pmb_panel`**: live player/missile/ball horizontal counters,
  `NUSIZx` copy-spacing decode, `REFPx` reflect, `HMxx` fine-adjust, and a
  160-dot scanline ruler — the 2600 analog of an OAM sprite grid (the 2600
  has exactly 5 fixed objects, no sprite table to browse).
- **RetroAchievements login + achievement-list panel** (`T-0802-005`,
  `debugger::cheevos_panel`): login/logout (`CheevosState::begin_login`,
  tracked via a new `LoginState`), a live achievement list (unlocked/total,
  points), a leaderboard list, rich presence, and a "recent unlocks" toast
  list (capped at 20) surfaced alongside the existing status-bar text path.
  All UI plumbing over `RaClient`'s already-complete surface — no new
  client capability needed.
- `EmuCore::frame_count`: frames completed since power-on/ROM-load (the
  watch engine's `frame` operand; also a natural counter for future
  TAS/movie work).

### Notes

- 181 tests passing workspace-wide (184 with `--features test-roms`), up
  from 160/163 at `[1.2.0]` — 21 new tests across the expression engine,
  the write log, and the call stack tracker.
- The new panels live inside the existing Debugger window (gated by
  `debug-hooks`, default-on) rather than as a separate window — including
  the RetroAchievements panel, which is therefore only reachable when
  `debug-hooks` is also enabled alongside `retroachievements`. Both are
  default-on, so this is a non-issue for the default build; a fully
  independent RA window is a possible future refinement, not pursued here.
- Clippy verified clean under `--features retroachievements` in addition to
  the default feature set (not part of the automated `ci-gate` sequence,
  which never uses `--all-features`, but checked by hand this release since
  this is the first release touching cheevos-panel code paths).

## [1.2.0] - 2026-07-01 - "Foresight"

Run-ahead, built entirely on `[1.1.0]`'s save-state/rewind snapshot
primitives — no new serialization machinery needed.

### Added

- **Run-ahead** (`rusty2600-frontend::runahead`, off by default —
  `config.video.runahead_frames`, `0..=4` via a new Settings slider, live
  without a restart). Per real `step_frame` call with `runahead_frames = N`:
  runs the real persistent frame (rewind capture ON, audio suppressed),
  snapshots the resulting state, runs `N - 1` hidden frames plus one more
  displayed frame with the same latched input (rewind capture suppressed
  throughout; only the final frame's audio reaches the ring), then restores
  the checkpoint — discarding the entire speculative run so the canonical
  timeline is exactly where the persistent frame left it. Snapshot/restore
  only ever happens at frame boundaries (the VSYNC edge), never
  mid-instruction, so the WSYNC/RDY beam-stall (a sub-instruction,
  sub-frame mechanic) never interacts with it.
- `EmuCore` gained two suppression flags backing run-ahead:
  `rewind_capture_suppressed` (speculative frames never enter rewind
  history) and `audio_output_suppressed` (only the one frame the user
  actually hears reaches the audio device — without this, run-ahead would
  emit `N` frames of audio per real ~16.67 ms tick and drift out of sync).
- Three regression tests: run-ahead with `0` lookahead is equivalent to a
  plain `step_frame`; the persistent timeline matches a plain run
  byte-for-byte regardless of lookahead depth (`1`/`2`/`5` frames tested);
  speculative frames never leak into `EmuCore::snapshots`.

### Fixed

- `rusty2600-tia`'s `Tia::tick_color_clock` incremented `scanline` (a `u16`)
  with a plain `+=`, which could overflow and panic if a program never
  asserts VSYNC for long enough (e.g. several of run-ahead's speculative
  frames each hitting `step_frame`'s 200,000-instruction safety timeout
  back-to-back, found by this release's own regression tests). Switched to
  `wrapping_add` — a hung or misbehaving program should never be able to
  crash the emulator over this.

### Notes

- 160 tests passing workspace-wide (163 with `--features test-roms`).

## [1.1.0] - 2026-07-01 - "Persistence"

The first release of the `v1.1.0 -> v2.0.0` RustyNES-parity line
(`to-dos/ROADMAP.md`). Ships the planned save-states/rewind rework, plus a
set of real frontend bugs found and fixed during manual verification of the
running emulator — the kind of defect only surfaces once someone actually
plays a game, not from `cargo test` alone.

### Added

- **Save-states** (`rusty2600-core::save_state`): a versioned binary
  snapshot format wrapping the already-`serde`-derived `System` (Cpu + Bus +
  phase + color-clock count). Encoded via `postcard` (a compact,
  `no_std`+`alloc`-native serde format) rather than a hand-rolled
  tagged-section encoder, since every chip crate (`Cpu`, `Bus`, `Tia`,
  `Riot`, the 22-variant `Cartridge` enum) already derives
  `Serialize`/`Deserialize` and already compiles under the
  `thumbv7em-none-eabihf` no_std gate. A caller-supplied opaque `rom_tag: u64`
  guards against restoring a save file against the wrong cartridge. See
  `docs/adr/0007-save-state-versioning.md` for the 3-tier compatibility
  policy (same MAJOR.MINOR round-trips byte-identical; same MINOR/different
  PATCH is additive-only via `#[serde(default)]`; anything else is
  best-effort with a typed `SaveStateError`).
- A permanent save-state round-trip regression test
  (`round_trip_is_byte_identical`), run in the default `cargo test
  --workspace` path — no ROM fixtures needed.

### Changed

- **Rewind rework**: `EmuCore`'s rewind ring (`emu_thread.rs`) now stores
  serialized `SaveState` bytes instead of raw `System` clones. `Cartridge`'s
  enum size is pinned to its largest fixed-size variant (`BankF4`'s 32 KiB
  ROM array) regardless of which board is actually loaded, so a raw
  `.clone()` paid that cost for every game; serializing through the real
  data shrinks a 2K/4K-cart rewind entry to its true size. No compression
  needed at this scale — a 2600's entire mutable state is tiny compared to
  even a modest NES/PPU/APU footprint.

### Fixed

- **Gameplay display + debugger flicker (rapid, entire-screen).** Under the
  default-on `emu-thread` feature, `about_to_wait` requests a redraw every
  event-loop iteration with no rate limit, while the emu-thread only
  publishes a fresh frame at the region's frame rate (~60 Hz). Every render
  pass that found no new frame in the channel used to fall back to
  `EmuCore::framebuffer` — a buffer that's *only ever written* by the
  non-`emu-thread` (`run_frame`) path and stays permanently black under the
  default feature set. The result was a real-content/black strobe on every
  render pass that missed a fresh frame (the majority of them on any
  display faster than ~60 Hz). Fixed by caching the most recently published
  frame in `Active::last_frame` and reusing it instead of the dead black
  buffer; cleared explicitly on ROM close so the display actually goes
  blank instead of freezing on the last game frame.
- **Window not sized to show the entire game display.** The wgpu blit
  pipeline always sampled the *full* `MAX_W x MAX_H` (160x228,
  PAL/SECAM-worst-case) framebuffer texture via `0..1` UV, regardless of the
  active region's real sub-rect (160x192 for NTSC) — so an NTSC frame was
  effectively squished into the top ~84% of the window with the never-written
  bottom rows sampled below it. Fixed with a new `uv_scale` uniform
  (`fb_w/MAX_W, fb_h/MAX_H`) that crops sampling to the active sub-rect,
  updated whenever the framebuffer's dimensions change.
- **Settings not persisting / not auto-saving.** The Settings window
  (`shell.rs::render_settings`) mutated the live `Config` in place, but
  `Config::save()` was only ever called from the top menu bar's Region
  submenu — every other Settings-window change (present mode, integer
  scale, audio enabled/volume, region via the Settings tab) silently never
  reached disk. Fixed by tracking `.changed()` on every Settings widget and
  pushing a new `MenuAction::SaveConfig`, dispatched by the native-only app
  layer (`shell.rs` is shared with the wasm build and must never call
  platform-specific file I/O directly).
- Wired the status bar's FPS estimate (previously a hardcoded `0.0` `TODO`)
  to a real exponential-moving-average of the render-pass cadence.
- The window title now shows the loaded ROM's filename (`Rusty2600 -
  <rom>`), resetting to `Rusty2600` on ROM close — matching RustyNES's own
  convention.

### Notes

- 157 tests passing workspace-wide (up from 151 at `[1.0.0]`; +6 for the
  save-state module, the rewind-restore regression tests, and the round-trip
  suite).
- `README.md` overhauled to RustyNES-parity depth and structure (Overview,
  Why Rusty2600, Highlights, Features, Quick Start, Architecture,
  Compatibility and Accuracy, Performance, Platform Support, Documentation,
  Contributing, License, Acknowledgments) — deliberately excludes a
  changelog-shaped "Version History" section; that information belongs here.

## [1.0.0] - 2026-07-01 - "Foundation"

The first stable release. Every v1.0.0 gate in `to-dos/ROADMAP.md` is met:
a real debugger, a real RetroAchievements integration, Stella-adjacent cart
breadth, a 100%-passing accuracy battery, and a green three-platform release
matrix. No behavior changes from `[0.9.0]` — this is a version-line
milestone tag plus a full documentation/status reconciliation pass, not a
feature bump.

### Status at 1.0.0

- **6507 CPU** — documented + undocumented opcodes, cycle-exact against the
  trimmed SingleStepTests corpus (4,660/4,660 cases, 233/233 opcodes) and the
  full ~10K-case-per-opcode corpus (weekly CI cron), plus Bruce Clark's
  exhaustive decimal-mode test (all `ERROR=0`).
- **TIA** — beam-raced video (RESPx/HMOVE comb, playfield, players/missiles/
  ball, all 15 pairwise collision latches) and two-channel poly-counter audio
  synthesis, unit-tested.
- **RIOT** — 128 B RAM, DDR/I/O ports, and the interval timer (prescale,
  underflow/INSTAT, read-after-write, and the post-underflow decrement-rate
  reversion fixed in `[0.9.0]`), all unit-tested.
- **Cart/bankswitch catalog** — all 8 Curated-tier schemes (2K, 4K, F8, F6,
  F4, CV, FA/CBS-RAM, Superchip, DPC, E7) plus 12 of 15 BestEffort schemes
  (F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA, 0840, FE, SB, X07) — 22 of
  25 catalogued schemes, all wired into automatic `detect()`. 4A50, AR/
  Supercharger, and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family remain
  out of scope for 1.0 (see "Explicit non-requirements" below).
- **Debugger** (`debug-hooks`, default-on) — live 6507/TIA/RIOT/memory
  panels, breakpoints/step/continue, a side-effect-free `Bus::peek`/
  `peek_range`, a standalone disassembler.
- **RetroAchievements** (`retroachievements`, off by default) —
  `rusty2600-cheevos` vendors the `rcheevos` C library; per-frame
  achievement tracking, hardcore mode, and a menu all work. A dedicated
  achievement-list/login/toast UI is deferred post-1.0 (`T-0802-005`).
  Server-side RA.org allowlisting is explicitly not a 1.0 gate — the
  integration working correctly is the bar, not third-party approval.
- **Accuracy battery** (`rusty2600-test-harness`) — the shared `Sentinel`/
  `run_cpu_until_sentinel` Layer 2 runner, a real `AccuracyScore`-gated
  battery (2/2, 100%, CI-enforced), and a tolerance-aware `SnapComparator`.
  A genuine externally-oracled golden CPU trace log and TIA-timing
  test-ROM fixtures remain honestly deferred (`T-0602-006`/`007`) — the
  Klaus functional/decimal oracles remain the authoritative CPU gate in the
  meantime.
- **Testing** — 151 tests passing workspace-wide (154 with
  `--features test-roms`). Full CI matrix green: Linux/macOS/Windows +
  the `no_std` gate.
- **Release matrix** — three platform archives (Linux/macOS/Windows)
  built and published for every tagged release since `[0.5.0]`.

### Explicit non-requirements (unchanged from `to-dos/ROADMAP.md`)

Netplay, TAS tooling, Lua scripting, HD texture packs, shader stacks, mobile
builds, and RA server-side allowlisting were never part of the 1.0 gate —
these remain Beyond-1.0 (Phase 7 residual breadth / Phase 8 reach), same as
documented throughout the `v0.x.0` line. A handful of frontend UI stubs fall
under this umbrella and remain unwired: the TIA-audio scope/cheat-editor/
ROM-DB-editor/TAStudio tools tab, the shader/filter picklist and per-side
overscan control, the 2600 key-rebind grid, the pacer's smoothed-FPS
display, and the Reset/PowerCycle/OpenDocs menu wiring — none of these
gate 1.0 per the non-requirements list above.

### Notes

- No code changes from `[0.9.0]` — version bump only (workspace + all 8
  crates), plus this changelog entry and a full `README.md`/`docs/STATUS.md`/
  `to-dos/ROADMAP.md` reconciliation pass confirming every number and claim
  matches the shipped `v1.0.0` tag.
- Future work continues under `to-dos/ROADMAP.md`'s "Beyond v1.0.0" line:
  the three deferred cart-scheme families, the two deferred accuracy-battery
  fixtures, the deferred RA UI, and any hardening the battery surfaces next,
  each shipped as its own `v1.x.0`/`v1.x.y` release.

## [0.9.0] - 2026-07-01 - "Hardening"

Closes `T-0601-008`, the one open accuracy residual in the project: Pitfall
II's boot-time RIOT-timer wait loop, which control-flow analysis had
already confirmed was NOT a DPC decode bug (byte-identical against
Gopher2600 for ~2,000 instructions) but a data-value divergence somewhere
in the RIOT timer model.

### Fixed

- **RIOT timer never reverted from post-underflow (divide-by-1) mode.**
  Once the interval timer underflowed (`INTIM` wraps `0x00`→`0xFF`), real
  6532 silicon decrements at a forced 1-CPU-cycle rate until the NEXT time
  a program reads `INTIM` — at that point the divider reverts to the
  originally-selected prescale (unless the underflow happened on that
  exact same cycle). Rusty2600 modeled this as two separate flags
  (`underflow` for the INSTAT-visible latch, correctly cleared on every
  read; `post_underflow` for the actual decrement-rate gate, previously
  cleared ONLY by a fresh `TIMxT` write) — so once a timer underflowed
  even once, it stayed in fast mode forever. Confirmed against Stella's
  `M6532::peek`/`updateEmulation` (`ref-proj/stella/src/emucore/
  M6532.cxx`): `myInterruptFlag`'s `TimerBit` is the SAME flag both
  behaviors are gated on there, and `peek()`'s INTIM case clears it unless
  `myWrappedThisCycle`. `Timer` gains a matching `wrapped_this_cycle`
  field; `cpu_read`'s INTIM branch now applies the same same-cycle
  exception when clearing `post_underflow`. See `docs/riot.md` for the
  full writeup.
- **Pitfall II now boots correctly.** Found via a rebuilt Gopher2600/Stella
  differential probe (memory-access tracing, not just PC tracing this
  time): the timer WAS passing through zero periodically once in fast
  mode, but the 262,144-cycle-period sawtooth's phase relative to the
  boot loop's 13-cycle poll rhythm happened to never land the loop's own
  `INTIM` read exactly on `$00` — so it looked "merely slow" until this
  investigation showed it was genuinely infinite. Confirmed fixed: the CPU
  now leaves the `$F108` wait loop at instruction ~22,864 (previously
  stuck past 400,000+) and reaches varied gameplay code, matching
  Gopher2600's own behavior. `screenshots/commercial/Pitfall II - Lost
  Caverns (USA).png` regenerated — no longer a blank blue frame.

### Notes

- Regression test: `intim_read_on_a_later_cycle_reverts_post_underflow_to_prescale`
  (`crates/rusty2600-riot/src/lib.rs`) pins the fix without disturbing the
  existing `timer_instat_and_post_underflow` test, which (correctly) covers
  the DIFFERENT same-cycle case where the flag must NOT revert immediately.
- Commercial-ROM regression oracle expansion remains out of scope for this
  release: it needs locally-supplied ROM dumps this development environment
  doesn't have (per project convention, never committed) — blocked by data
  availability, not effort.
- Documentation sync pass: `docs/architecture.md`'s crate map was missing
  `rusty2600-cheevos` and still described `rusty2600-frontend` as the only
  `std`/`unsafe` crate; `docs/compatibility.md`'s TIA-quirk section
  referenced "v0.7.0/v0.8.x" as a now-stale target for the `LostMOTCK`
  (Cosmic Ark) hard problem, reworded to state the real (unimplemented)
  status instead.

## [0.8.0] - 2026-07-01 - "Battery"

Stands up `rusty2600-test-harness`'s Layer 4 accuracy battery for real.
Previously the crate's `GoldenLogDiffer`/`run_until_complete`/
`AccuracyScore`/`SnapComparator` were unused scaffolding — the real Klaus
functional/decimal tests hand-rolled their own PC-trap loops directly
against `rusty2600_cpu::Cpu`, bypassing the harness entirely.

### Added

- **`Sentinel` + `run_cpu_until_sentinel`** — the shared Layer 2 (CPU-only)
  test-ROM runner. Two variants cover the bundled Klaus oracles' own
  completion protocols exactly: `PcTrap` (success once PC reaches an
  address AFTER stepping; a stuck PC anywhere else is a failure — the
  functional test's convention) and `PcWithZeroPageCheck` (success once PC
  reaches an address BEFORE stepping, gated on a zero-page pass/fail byte —
  the decimal test's convention). `tests/klaus_test.rs`'s two tests now run
  through this shared function instead of each hand-rolling its own loop —
  a pure refactor, same ROMs, same sentinels, unchanged pass/fail behavior.
- **`tests/accuracy_battery.rs`** — the real Layer 4 battery. Runs both
  bundled Klaus oracles through `run_cpu_until_sentinel`, records each into
  a real `AccuracyScore`, and asserts the aggregate pass rate meets the
  v1.0 threshold (`docs/STATUS.md`'s Version policy: ≥90%, 100% the goal —
  currently 2/2, 100%). This lives inside the existing `cargo test
  --workspace --features test-roms` CI step, so a pass-rate regression
  already fails CI — no new workflow file was needed.
- **`SnapComparator` tolerance-aware comparison** —
  `diff_count_within_tolerance`/`matches_within_tolerance`, both
  `abs_diff`-based per-byte thresholds, alongside the existing exact
  byte-diff.
- **`GoldenLogDiffer` is now a real capture/diff engine** (a genuine
  `Vec<TraceRecord>` buffer + a real per-record diff algorithm), not just a
  counter. Honestly documented: no externally-oracled golden CPU trace log
  is bundled yet (`T-0602-007`), so `bundled()` reports `false` until one
  is — Klaus's own internal pass/fail trap remains the authoritative CPU
  oracle in the meantime.

### Notes

- **Deferred, deliberately:** a genuine per-instruction golden CPU trace
  log produced by an independent, externally-trusted oracle (an
  instrumented Stella or Gopher2600 build) for `GoldenLogDiffer`
  (`T-0602-007`), and TIA-timing/draw-ROM test fixtures + goldens for the
  Layer 3 `run_until_complete` runner and the tolerance-aware
  `SnapComparator` (`T-0602-006`). Both need real external oracle data to
  do honestly, not a guessed-at protocol.
- The `SingleStepTests` cycle-exact audit (233/233 opcodes) remains its own
  separately-enforced gate in `rusty2600-cpu`'s own test suite, not yet
  folded into the shared `AccuracyScore` (`T-0602-008`).

## [0.7.0] - 2026-07-01 - "Cheevos"

The RetroAchievements slice of Phase 8, pulled forward per the ROADMAP's
v1.0.0 gate. `rusty2600-cheevos` goes from a one-line stub to a real,
tested FFI wrapper, and `rusty2600-frontend`'s `retroachievements` feature
goes from an inert flag to real per-frame achievement tracking.

### Added

- **`rusty2600-cheevos`** — a native-only, safe Rust wrapper around the
  vendored RetroAchievements `rcheevos` C library (MIT), lifted and adapted
  from RustyNES's own `rustynes-cheevos` (same author; rcheevos itself is
  console-agnostic, so ~95% of the FFI bridge, event mirror, and HTTP worker
  carry over unchanged — only the memory-address map and one console-ID
  constant are 2600-specific):
  - `RaClient` — owns `rc_client_t`, drives per-frame achievement processing
    (`do_frame`/`idle`/`reset`), login/load-game, progress
    (de)serialization, and rich presence. Deliberately `!Send`/`!Sync`.
  - `memory::ra_addr_to_riot` — the RetroAchievements-flat -> 2600 CPU-bus
    address map. Far simpler than most consoles: the 2600's ONLY RAM is the
    RIOT's 128 bytes, so RA's flat space maps directly onto it (no
    cartridge-WRAM-window split to model).
  - An off-thread HTTP worker (`ureq`) for the RA API, and a thread-local
    event queue draining into an owned `RaEvent` enum.
  - The whole crate body is `#![cfg(not(target_arch = "wasm32"))]` — a wasm
    workspace build never needs a C toolchain or links the vendored source.
- **`rusty2600-frontend`'s `retroachievements` feature is now real**
  (`cheevos.rs`): a `CheevosState` owning the `RaClient` on the winit/main
  thread — NOT inside `EmuCore`, since `RaClient`'s `!Send` bound is
  incompatible with `EmuCore`'s `Send` requirement (needed by the
  default-on `emu-thread` feature). Pumped once per frame under the SAME
  brief emu lock the present path and the debug snapshot already take,
  peeking the bus via `|addr| bus.peek(addr)`. ROM load/close hooks call
  `begin_load_game`/`unload_game`; a new Emulation -> RetroAchievements menu
  shows game-recognition status and a hardcore-mode toggle;
  achievement-unlock/server events surface as status-bar text.

### Notes

- **Deferred, deliberately:** a dedicated achievement-list panel, a login
  dialog, and a rich-presence/unlock-toast HUD (`T-0802-005`). The backend
  is real and events fire correctly today, surfaced as plain status-bar
  text — this is the dedicated UI surface for them, scoped as a distinct,
  UI-heavy follow-up rather than folded into the initial wiring.
- Rusty2600's workspace enforces `clippy::pedantic`/`clippy::nursery`/
  `missing_docs`, which RustyNES's own workspace does not — the lifted
  files needed real doc comments added to every public struct field/enum
  variant plus several mechanical clippy fixes.
- `cargo test -p rusty2600-cheevos` runs real (not mocked) FFI calls into
  the vendored C library, including one test that performs genuine network
  I/O against an intentionally-invalid endpoint to verify the async
  login-completion bridge fires exactly once on a transport error.

## [0.6.0] - 2026-07-01 - "Catalog"

Closes 12 of the 15 schemes in the local BestEffort catalogue (`docs/cart.md`)
— 22 of 25 catalogued schemes total, up from 19. Also regenerates the full
screenshot corpus against the current frontend/cart state.

### Added

- **FE (Activision — Decathlon, Robot Tank, Space Shuttle, Thwocker):** 8 KiB
  ROM, 2×4K banks, selected by a hardware trick rather than a fixed hotspot
  address. The bank-switch routine's `JSR` pushes its return address's high
  byte to `$01FE` (a RIOT-RAM mirror, watched via `Board::snoop_read`/
  `snoop_write`) immediately followed by the low byte to `$01FD`; the value
  of THAT second access picks the bank (`(value >> 5) ^ 0b111`, masked to 2
  banks) — matches Stella's `CartridgeFE::checkSwitchBank` exactly. Detected
  via 5 known-title boot signatures, guarded against misdetecting a real F8
  image.
- **SB (Superbank):** 128/256 KiB ROM, 32/64×4K banks. Any read OR write to
  `$0800..=$0FFF` selects the bank from the LOW BITS of the accessed
  address itself, not a fixed hotspot value. Now the default fallback at
  128/256 KiB once 3E/DF/3F are ruled out, matching Stella's own detection
  chain, which defaults straight to SB at these two sizes.
- **X07 (AtariAge homebrew multicart scheme):** 64 KiB ROM, 16×4K banks. A
  direct select (address bits 4-7 pick the bank) plus a secondary toggle
  active only while the current bank is 14 or 15 (address bit 6 flips
  between them) — matches Stella's `CartridgeX07::checkSwitchBank` exactly.
- Regenerated all 126 screenshots (`screenshots/homebrew/`,
  `screenshots/commercial/`) against the current build. `Zippy the
  Porcupine` now renders (F0 landed in v0.4.0); homebrew goes to 110/110.

### Notes

- **4A50, AR/Supercharger, and DPC+/CDF/CDFJ/CDFJ+ remain unimplemented,
  deliberately.** 4A50 needs three independently relocatable ROM/RAM windows
  plus a previous-access-dependent hotspot — substantially more state than
  the schemes above. AR needs a tape/audio-based loader, architecturally
  unlike every other scheme here. The DPC+ family needs a full ARM7TDMI
  Thumb interpreter. All three are scoped as separate, larger undertakings
  for a follow-up release, not rushed additions here.
- Pitfall II's blank-frame residual (`T-0601-008`) and Communist Mutants
  from Space's detection gap are unchanged by this release, as expected —
  neither's root cause was touched.

## [0.5.0] - 2026-07-01 - "Inspector"

Closes Phase 5 (frontend): the `debug-hooks` feature goes from an unwired
forward placeholder to a real debugger, and the four chip-crate Criterion
benches go from empty stubs to populated, measured baselines.

### Added

- **The real debugger** (`crates/rusty2600-frontend/src/debugger/`),
  replacing `shell.rs`'s literal `TODO(impl-phase)` panel bodies:
  - **6507 panel** — A/X/Y/S/PC/P register grid, Step (one
    `step_instruction()`) and Continue (run to a breakpoint or a
    1,000,000-instruction safety cap) buttons, a breakpoint add/remove list,
    and a scrolling disassembly window starting at PC.
  - **TIA panel** — beam position, P0/P1/M0/M1/BL positions + colors, the
    playfield/background colors, and the 15 pairwise collision latches.
  - **RIOT panel** — `INTIM` + prescale, `SWCHA`/`SWCHB` pin state + DDRs.
  - **Memory panel** — a 256-byte hex+ASCII viewer with RIOT-RAM (`$0080`)
    and cart-window (`$1000`) quick-jump buttons.
  - **A standalone 6502 disassembler** (`debugger/disasm.rs`), independent
    of the CPU crate's private opcode dispatch — covers the full documented
    NMOS instruction set; undocumented opcodes render as `.byte $xx` rather
    than inventing a mnemonic.
- **`Bus::peek`/`Bus::peek_range`** (`rusty2600-core::bus`): side-effect-free
  reads for debugger/tooling use. A real `cpu_read` can trigger bankswitch
  hotspots, RIOT's INTIM read-clears-underflow behavior, and cart
  `snoop_read` side effects — none of which a memory-viewer peek should
  ever cause. `peek_range` clones the bus ONCE and reads every requested
  byte from that single clone, instead of paying a full `Bus` clone per
  displayed byte (the difference between one clone per frame and one clone
  per byte for a 256-byte memory panel).
- **Populated Criterion benches** for all four chip crates
  (`cpu_bench.rs`/`tia_bench.rs`/`riot_bench.rs`/`cart_bench.rs`), previously
  empty `fn main() { /* TODO */ }` stubs. Real measured baselines recorded
  in `docs/performance.md` (e.g. `tia_full_ntsc_frame` ~899µs, comfortably
  under the ≤2ms/frame target).

### Changed

- `debug-hooks` moves from an unwired forward placeholder into
  `rusty2600-frontend`'s `default` feature list (`default = ["wasm-winit",
  "help-tui", "emu-thread", "debug-hooks"]`), the same precedent as
  `emu-thread`'s own placeholder-to-default-on transition. A debugger-free
  build remains available via `--no-default-features --features
  wasm-winit,help-tui,emu-thread`.

### Notes

- Not visually screenshot-tested in this development environment: the
  native `winit + wgpu` debugger window requires a live display, and this
  session's sandbox had no safe way to pop a window without disrupting the
  user's desktop. Verified instead via unit tests (disassembler + bus-peek
  side-effect tests), a clean `cargo clippy --workspace --all-targets -- -D
  warnings`, and successful compiles for native (`debug-hooks` on and off)
  and `wasm32-unknown-unknown`.

## [0.4.1] - 2026-07-01

Continues the v0.4.x BestEffort patch train with two more Batch 2 schemes
and the read-side counterpart of v0.4.0's `snoop_write` hook.

### Added

- **`Board::snoop_read(addr, val)`:** a new default-no-op hook, called from
  `Bus::cpu_read` for every read the console routes to TIA/RIOT space,
  after the value TIA/RIOT would return is computed. Simpler than
  originally scoped in v0.4.0's notes: the schemes needing this only
  OBSERVE the access and trigger a bankswitch side effect — they still
  return exactly what TIA/RIOT would have anyway, so no read-redirection
  capability was needed.
- **UA (UA Ltd. / Brazilian Digivision):** 8 KiB ROM, 2×4K banks,
  bank-select triggered by a read OR write to `$220`/`$240` (or the
  Digivision variant's `$2C0`/`$FB0`).
- **0840 (EconoBank):** 8 KiB ROM, 2×4K banks, same shape, hotspots at
  `$800`/`$840`.

Both wired into `detect()` at 8 KiB, checked after 3E/3F and before falling
back to plain F8, matching Stella's own priority order.

### Notes

- FE (Activision SCABS) remains unimplemented: it additionally needs the
  snooped *value* (not just the address) to pick a bank — `snoop_read`'s
  `val` parameter already supports this, so FE's remaining work is its own
  decode logic, not a further interface change. SB/X07/4A50 likely only
  need `snoop_read` too, per their own Stella source (also not yet
  implemented).

## [0.4.0] - 2026-07-01 - "Breadth" (Batches 1-2)

The first installment of the BestEffort bankswitch long tail (the plan's
staged patch train toward Stella-adjacent cart-scheme parity): 7 new
schemes across two batches, plus a Bus/Board architecture extension needed
to model several of them at all. All BestEffort tier per ADR 0003 —
register-decode + boot-smoke tested only, never accuracy-oracle-gated.

### Added

- **Batch 1 (classic homebrew):**
  - **F0 (Dynacom Megaboy):** 64 KiB ROM, 16×4K banks, a single
    SEQUENTIAL-ADVANCE hotspot at `$1FF0` (wraps 15 → 0) — unlike every
    other F-series scheme, the game can't jump to an arbitrary bank.
  - **E0 (Parker Bros):** 8 KiB ROM, four 1 KiB segments; the first three
    independently selectable among 8 banks each, the fourth permanently
    fixed to the last bank.
  - **3F (Tigervision):** variable-size ROM, bank selected by writing the
    desired bank number to ANY address whose low byte is `$3F` — not a
    `$1000+` cart-window hotspot at all.
  - **3E (Tigervision + RAM, Boulder Dash):** `3F` plus a `$3E` hotspot
    selecting a RAM bank instead of ROM.
- **Batch 2 (CPUWIZ homebrew family):**
  - **EF/EFSC:** 64 KiB ROM, 16×4K banks, direct-select hotspots
    `$1FE0-$1FEF` (unlike F0's sequential-advance at the same size); EFSC
    adds the standard 128 B Superchip RAM overlay.
  - **DF/DFSC:** 128 KiB ROM, 32×4K banks, direct-select hotspots
    `$1FC0-$1FDF`, same Superchip option.
  - **BF/BFSC:** 256 KiB ROM, 64×4K banks, direct-select hotspots
    `$1F80-$1FBF`, same Superchip option.
- **`Board::snoop_write(addr, val)`:** a new default-no-op hook, called from
  `Bus::cpu_write` for every write the console routes to TIA/RIOT space
  (not just the cart window) — matching real hardware, where a cartridge's
  edge connector is wired to every address line. Required for 3F/3E's
  `$3E`/`$3F` hotspots, which live deep in TIA/RIOT-mirrored zero-page
  space, not `$1000+`.
- All 7 new schemes wired into `detect()`'s automatic dispatch, in the same
  relative priority Stella's own `CartDetector` uses at each size, so a
  same-size collision with a Curated scheme (F0/EF vs nothing at 64 KiB;
  E0/3E/3F vs plain F8 at 8 KiB; 3E/3F vs plain F4 at 32 KiB) is never
  silently misdetected. 128 KiB and 256 KiB images with no matching
  signature now return `None` rather than guessing, since the only other
  schemes at those sizes (SB, 4A50) aren't implemented yet.

### Changed

- Enabled serde's `alloc` feature workspace-wide (needed for `Vec<u8>`
  serialize/deserialize in this `no_std + alloc` crate).

### Fixed

- A `clippy::large_stack_frames` failure surfaced from inlining a 64 KiB
  array into the `Cartridge` enum (an enum is sized to its largest
  variant, so one large variant inflates every stack frame that moves a
  `Cartridge`/`Bus`/`System` by value). Fixed by storing large-ROM boards'
  data as `Vec<u8>` instead of a fixed array — applies to `BankF0`,
  `Bank3F`, `Bank3E`, `BankEF`, `BankDF`, and `BankBF`.

### Notes

- **Deliberately deferred, not overlooked:** UA, 0840, FE, SB, X07, and
  4A50 all bankswitch on CPU *reads* of TIA/RIOT-mirrored addresses, not
  just writes — `Board::snoop_write` doesn't cover them. FE additionally
  needs the snooped *value* (not just the address) to pick a bank. This is
  a bigger interface question (the board must be able to REDIRECT a read,
  not just observe it, since these hotspots overlap real RIOT RAM / TIA
  registers) than the write-only case this release solved cleanly — scoped
  as a follow-up ticket (`T-0402-006`/`011`) rather than rushed.
- Batches 3-5 (DPC-family/fractional-datafetcher schemes, ARM/peripheral-
  integrated carts, and multicart wrappers) remain for subsequent v0.4.x
  releases.

## [0.3.0] - 2026-07-01 - "Curated"

Closes out the full Curated-tier cart-scheme set the plan scoped for this
release: CommaVid (CV), CBS RAM Plus (FA), Superchip (F8SC/F6SC/F4SC), the
DPC coprocessor (Pitfall II), and E7 (M-Network) are all implemented — 8 of 8
Curated schemes, all wired into `detect()`'s automatic dispatch via
hotspot-pattern heuristics ported from Stella's `CartDetector.cxx`. Also
reorganizes the local commercial-ROM staging convention and adds a
`screenshots/commercial/` showcase corpus alongside the existing
`screenshots/homebrew/` one.

### Added

- **CV (Commavid):** 2 KiB ROM + 1 KiB on-cart RAM, no bank switching.
  Accepts either a 2 KiB ROM-only image or a 4 KiB image whose first 2 KiB
  is initial RAM content (Stella's "MagiCard saved program listing" case).
- **FA (CBS RAM Plus):** 12 KiB ROM as three 4 KiB banks (`$1FF8`/`9`/`A`) +
  256 B RAM (write-low `$1000-$10FF` / read-high `$1100-$11FF`).
- **Superchip (F8SC/F6SC/F4SC):** a 128 B RAM overlay
  (write-low `$1000-$107F` / read-high `$1080-$10FF`) added to `BankF8`/
  `BankF6`/`BankF4` via an opt-in `with_superchip()` builder rather than new
  types, since Superchip variants are ROM-size-identical to their plain
  counterparts (Stella itself can't tell them apart by size either).
- **DPC (Pitfall II's "Display Processor Chip"):** F8-style hotspot
  bankswitching + a memory-mapped register file at `$1000-$107F` — an LFSR
  random-number generator and 8 hardware "data fetchers" (graphics reads,
  level-generation RNG, and a "music mode" for fetchers 5-7). Verified with
  a Gopher2600 differential probe: byte-identical CPU control-flow through
  the first ~2,000 executed instructions of the real Pitfall II ROM. One
  deliberate residual, documented rather than silently guessed at: DF5-7's
  oscillator-driven auto-advance isn't implemented, since it only drives
  the cartridge's own analog audio-mixing hardware and Rusty2600's audio
  bus is entirely TIA-owned with no cart-audio path.
- **E7 (M-Network):** the 16 KiB / 8×2K-bank configuration, the most
  complex classic bankswitch scheme — a selectable lower 2 KiB segment
  (banks 0-6 ROM, bank 7 "switch to RAM instead"), a separate always-active
  256 B RAM window, and a fixed upper region always mapping the last bank's
  ROM (so the reset vector is always reachable).
- **ROM-DB disambiguation (`T-0401-009`):** CV vs plain 2K/4K, Superchip vs
  plain F8/F6/F4, and E7 vs plain F6 were all same-size collisions blocking
  automatic dispatch — resolved with three hotspot-pattern heuristics
  (`is_probably_cv`/`is_probably_superchip`/`is_probably_e7`) ported
  directly from Stella's `CartDetector.cxx`, checked before falling back to
  the more common plain scheme. Validated against a real commercial ROM:
  `BurgerTime (USA).a26` was previously misdetected as plain F6 (an
  all-black frame); it's now correctly identified as E7, cross-checked
  against Stella's own properties database ("M Network" manufacturer).
- **`screenshots/commercial/`:** a gameplay showcase for the local
  commercial-ROM corpus, mirroring `screenshots/homebrew/`'s convention —
  15 of 16 staged titles render (the one exception, Pitfall II, boots
  correctly but never clears a boot-time RIOT-timer wait loop; see Fixed
  below). ROMs themselves stay gitignored; only the rendered PNGs are
  committed (a frame carries no copyrighted game code).

### Changed

- **`tests/roms/external/`** commercial-ROM staging now lives under a
  `commercial/` subfolder (`tests/roms/external/commercial/`), mirroring
  RustyNES's `tests/roms/external/<family>/` convention — still gitignored,
  never committed.
- **`docs/cart.md`'s tier catalogue:** DPC and E7 were both originally
  classified BestEffort in the research-report catalogue; reclassified
  Curated to match the approved plan's v0.3.0 scope (both now implemented
  and tier-pinned by their own `Board::tier()`).

### Found (tracked, not fixed this release)

- **`T-0601-008`:** Pitfall II boots with independently-verified-correct
  DPC control-flow, but never clears a boot-time RIOT-timer (`INTIM`) wait
  loop at `$F108-$F112` — confirmed via a Gopher2600 differential probe
  that both emulators enter the identical loop, but Rusty2600 doesn't exit
  it within a billion-instruction budget where Gopher2600 does. Since the
  loop's exit condition depends only on `INTIM`, not on the DPC read it
  also performs, this points to a data-value divergence somewhere upstream
  (likely feeding the timer's reload value) rather than a DPC decode bug.

### Notes

- **Explicitly out of scope for v0.3.0** (tracked as separate tickets, not
  overlooked): 8 KiB ambiguity between the implemented `BankF8` and three
  not-yet-implemented BestEffort schemes (E0/FE/3F, `T-0401-001`); DPC+
  detection (`T-0401-006`); pirate/homebrew BMC schemes (`T-0401-007`). The
  ~50-scheme BestEffort long tail targets v0.4.x.

## [0.2.0] - 2026-07-01 - "Cycle-Exact"

Closes out the accuracy-hardening pass the plan scoped for this release:
RIOT timer semantics, TIA collision-latch behavior, the audio model's real
scope, two new ADRs resolving open research questions, the full SingleStepTests
corpus wired into CI, Klaus's decimal-mode test wired for real, and a
substantial CPU-crate cleanup/restructuring. One real bug found and fixed
along the way in `tests/roms/test_suite/` (three commercial ROMs scrubbed
from history); several deliberately-deferred findings logged as scoped
future tickets rather than rushed.

### Added

- **RIOT (`T-0601-005`):** pinned the DirtyHairy/Stella read-after-write
  timer model explicitly (`INTIM` one cycle after a `TIMxT` write already
  reads `written_value - 1`, for every prescale); confirmed the
  read/write-at-the-underflow-cycle question resolves structurally from the
  scheduler's existing tick-then-access ordering.
- **TIA collisions:** pinned continuous per-clock re-evaluation (not just
  once per object-enable) and same-cycle `CXCLR` clearing. Found and logged
  (not fixed — a real architecture change, `T-0601-007`) that collisions
  occurring during HBLANK aren't currently detected, since the position/
  pixel-coordinate model is 0..159-visible-window-relative, not full-scanline.
- **ADR 0005:** TIA revision variation modeled as independent, named,
  individually-toggleable hardware quirk flags (mirroring Gopher2600's
  eight — `LostMOTCK` is the Cosmic Ark starfield bug already tracked for
  v0.7.0/v0.8.x), not a coarse chip-revision enum.
- **ADR 0006 + real fix:** power-on RIOT RAM and CPU `A`/`X`/`Y` are now
  actually seeded from `System::new(seed)` via a small SplitMix64 mixer
  (Stella's `ramrandom=<seed>` model) — `docs/riot.md`/`docs/cpu.md` already
  claimed this was true; it wasn't, until now.
- **Full SingleStepTests corpus in CI:** `.github/workflows/singlestep-full.yml`
  (weekly cron + manual dispatch — ~700 MB across 233 opcodes, unsuitable
  for the per-push path) downloads and audits the full ~10K-cases/opcode
  upstream corpus.
- **Klaus `6502_decimal_test` wired for real:** the binary was a 0-byte
  placeholder; assembled it for real from the bundled `as65` 1.42 source.
  `ERROR=0` — decimal-mode `ADC`/`SBC` is bit-exact against the exhaustive
  256×256×2-carry-in reference. Both Klaus tests moved behind the
  `test-roms` feature (previously unconditional despite the crate's stated
  intent to gate them there).

### Fixed / Changed

- **`rusty2600-cpu` restructured (`T-0601-006`):** the crate carried a
  second, entirely dead, never-compiled RustyNES-lineage CPU implementation
  (`cpu.rs`/`bus.rs`/`disasm.rs`/`status.rs`, ~3,560 lines — no `mod`
  declarations ever wired any of it in). Deleted outright. The one live
  file (`lib.rs`) had only stale NES-flavored comment prose (no actual dead
  code) attached to correct, needed universal 6502 behavior; rewrote four
  comment blocks to describe the 2600-relevant case instead. Also split
  the 2,172-line `lib.rs` into `status.rs`/`bus.rs`/`cpu.rs` + a thin
  `lib.rs`, matching RustyNES's own live file layout. Zero regression
  (full SingleStepTests audit + both Klaus tests re-verified).
- **`release.yml`** was missing the same Linux system-deps step `ci.yml`
  needed, and never actually uploaded a release asset on any platform —
  fixed (shipped as v0.1.2).
- **`pages.yml`** was an unimplemented stub (checkout + comments only) —
  implemented for real: wasm demo (Trunk) + rustdoc, one combined artifact,
  deployed via the standard GitHub Pages actions.
- Default branch renamed `master` → `main`.

### Removed

- **Three commercial ROM dumps** (`Pac-Man 4K`, `Crazy Balloon`, `Lady Bug`)
  found committed under `tests/roms/test_suite/` — scrubbed from all git
  history via `git-filter-repo` and force-pushed (a real violation of this
  project's own "never commit commercial ROMs" rule). ~110 other files in
  that directory are legitimate homebrew/freeware (several carry embedded
  AtariAge copyright/distribution strings) and were kept.

### Notes

- **Deliberately deferred, not silently dropped:** HBLANK-region TIA
  collisions (`T-0601-007`); the TIA audio model needs a real
  rearchitecture around Stella's two-counter pulse/noise feedback network
  and fixed-position two-phase clocking, not a two-mode patch (see
  `ref-docs/2026-07-01-supplemental-audio-hardware-model.md` and
  `to-dos/phase-3-audio/sprint-2-hardware-accurate-model.md`). Both are
  real, scoped, tracked work — not overlooked.

## [0.1.2] - 2026-07-01

Patch release, primarily to ship a working release build: v0.1.1's
tag-triggered release build silently failed on Linux (missing system deps,
same class of bug fmt-fixing surfaced in ci.yml/pages.yml) and — independent
of that — never uploaded a release asset on any platform, including the leg
that did succeed. Also includes one small, already-tested accuracy item that
landed in the same commit range (v0.2.0 work proper starts after this tag).

### Fixed

- `release.yml`: added the same Linux system-dependency install step as
  `ci.yml`/`pages.yml` (ALSA/udev/xkbcommon/Wayland/X11), and added the
  packaging + `gh release upload` steps that were missing entirely — each
  platform leg now actually attaches a real archive (tar.gz for Linux/macOS,
  zip for Windows; binary + README + both licenses) to the release.

### Added

- **RIOT (`T-0601-005`):** explicitly pinned the DirtyHairy/Stella
  read-after-write timer model — `INTIM` read one cycle after a `TIMxT`
  write already reads back as `written_value - 1`, for every prescale — and
  documented why the read/write-at-the-exact-underflow-cycle question needs
  no separate fix (it resolves structurally from the scheduler's existing
  tick-then-access ordering, the same one validated for TIA's `WSYNC`
  line-boundary case).

## [0.1.1] - 2026-06-30 - "Foundation"

The first tagged release. Two earlier `[Unreleased]` entries in this file
disagreed with each other and with reality (one claimed the chip cores were
`// TODO` skeletons, the other claimed Phases 1-5 were fully complete) — this
is the truthful checkpoint of what the workspace actually contains today,
consolidating that history into one accurate entry before any tag exists to
anchor it to.

### Added

- The eight-crate cycle-accurate architecture (`rusty2600-cpu` / `-tia` /
  `-riot` / `-cart` / `-core` / `-frontend` / `-cheevos` / `-test-harness`),
  the master-clock lockstep scheduler (TIA color clock timebase, 6507 on
  every third clock, `WSYNC`/`RDY` beam-stall), and the seeded determinism
  contract (ADR 0004).
- **CPU (MOS 6507):** documented + undocumented opcodes implemented and
  cycle-exact against a trimmed SingleStepTests/`65x02` corpus (233 opcodes,
  20 cases each, 4,660 cases total, 100% passing) — catches wrong cycle
  counts, not just wrong final register values.
- **TIA (video + audio):** beam-raced video (RESPx 9-CLK positioning, HMOVE
  comb, playfield/players/missiles/ball/collisions) and two-channel
  poly-counter audio synthesis, including the WSYNC line-boundary edge-case
  fix that resolved a real frame-length jitter bug (verified byte-for-byte
  against the `Gopher2600` reference emulator — see `docs/tia.md` §WSYNC).
- **RIOT (MOS 6532):** RAM/DDR ports/interval timer implemented and
  unit-tested (prescale, underflow, `INSTAT`).
- **Cartridge boards:** 2K, 4K, F8, F6, F4 implemented (5 of 25 catalogued
  schemes) behind the accuracy-tiering honesty gate (ADR 0003;
  `mapper_tier_honesty.rs`).
- **Frontend:** the `winit + wgpu + cpal + egui` shell — rendering, audio,
  pacing (frontend-owned, core never sees wall-clock time), 2600 input map
  (joystick/paddle/console switches), NTSC/PAL/SECAM palette + region model,
  WASM/web build (Trunk), and a dedicated emu-thread path (`emu-thread`,
  default-on as of this release).
- The full documentation set (`docs/` spec + four ADRs), the immutable
  research corpus (`ref-docs/research-report.md`), and the Phase 0-8 roadmap
  (`to-dos/`).
- 74 tests passing workspace-wide (`cargo test --workspace`).

### Fixed

- **F8 cart-tier conflict:** `BankF8::tier()` returned `Core`; both
  `docs/cart.md` and the research report specify `Curated` (F8 is
  hotspot-driven, and `Core` is reserved for the two schemes needing zero
  board-specific logic). Fixed and pinned by a new honesty-gate test
  (`core_tier_is_reserved_for_unbanked_schemes`). The cart crate's own module
  doc additionally mislabeled E0/E7/FE/3F/3E/DPC as `Curated` when
  `docs/cart.md`'s authoritative catalogue lists them `BestEffort` — corrected
  to match.
- **`emu-thread` stale blocker:** the feature was off-by-default behind a
  comment claiming `Board: Send` was unresolved; `Bus::board` has been a
  concrete `Cartridge` enum (not `Box<dyn Board>`, hence trivially `Send`)
  since Phase 5 landed. Confirmed by a clean `--features emu-thread` build,
  which surfaced (and this release fixes) three latent bugs in that
  previously-never-compiled path: a private-field access in `app.rs`, and two
  pedantic-clippy violations (`if_not_else`, `map_unwrap_or`) in the same
  file. Default-on now that the path is verified clean under
  `clippy --all-targets -- -D warnings`.
- **CI:** the previous three `main`-branch CI runs all failed at `cargo fmt
  --all --check` (formatting drift in `app.rs`/`emu_thread.rs` predating this
  release); resolved as part of this pass. The fmt fix then surfaced two more
  real gaps: `ci.yml` never installed the Linux system packages
  (ALSA/xkbcommon/Wayland/X11) `cpal`/`winit`/`egui` need, so `ubuntu-latest`
  failed to even build; and `pages.yml` (activated for the first time by the
  branch rename below) was an unimplemented stub — just a checkout step and
  comments — that failed instantly with zero real steps. Both fixed: ci.yml
  now installs the needed apt packages, and pages.yml is a real pipeline
  (wasm demo via the previously-uncommitted `crates/rusty2600-frontend/web/`
  Trunk project + `cargo doc`, combined into one Pages artifact — demo at
  `/`, rustdoc at `/api/`). Also fixed along the way: `web/Trunk.toml` was
  missing `public_url = "/Rusty2600/"` (GH Pages serves this repo at a
  subpath, not the domain root); `.gitignore`'s `/web/dist/` entry was
  root-anchored and never matched the real nested path; and pages.yml's
  explicit `permissions:` block omitted `contents: read`, silently revoking
  checkout's ability to clone the repo at all.

### Changed

- Default branch renamed `master` → `main` (matching the `RustyNES` sibling
  project's convention); `origin/master` deleted. This also activates
  `pages.yml`'s `branches: [main]` deploy trigger, which had been silently
  inert since it was written against a branch name that didn't exist.

### Notes

- This release intentionally does not claim "100% accurate" — the accuracy
  battery, the full (untrimmed) SingleStepTests corpus, RIOT
  read-after-write / TIA collision-latch verification, and the BestEffort
  cart long tail are explicitly open work for `v0.2.0` onward. See
  `docs/STATUS.md` for the authoritative per-suite / per-chip state and
  `to-dos/ROADMAP.md` for the full version-to-phase plan through `v1.0.0`.
