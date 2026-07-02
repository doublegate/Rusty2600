# Changelog

All notable changes to Rusty2600 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Documentation-only research decision** (`v2.5.0` "Web Awakens", no code
  change) — checked whether the Atari Keyboard/Keypad Controller and the
  Trak-Ball are worth modeling, against Stella's own implementation and
  properties database rather than assumption. Trak-Ball: zero official
  Atari 2600 releases used it (only 2 homebrew ROMs in Stella's DB).
  Keyboard: 40 ROMs including one real official release (*Brain Games*,
  1978), but still niche relative to the whole catalogue. Neither is
  modeled this arc — deliberately deprioritized, not permanently ruled
  out. See `docs/frontend.md`.

## [2.4.0] - 2026-07-02 - "Save Point"

The first release of the RustyNES gap-closure arc (`v2.4.0 -> v3.0.0`) and
the first Rusty2600 release shipped through a real GitHub PR with CI +
automated review-bot adjudication (PR #1), instead of a direct-to-main
push. Landed via four independent, parallel implementation efforts (plus
a direct-authored repo-hygiene batch), each independently gate-verified
before merging, then reviewed by two automated PR bots (GitHub Copilot,
Gemini Code Assist) whose 9 findings were all genuine and fixed — most
notably a real correctness bug (loading a save-state slot didn't clear
the rewind ring, so pressing Rewind afterward jumped to the pre-load
timeline) and a real performance issue (save-slot status was probed via
8 filesystem `stat` calls every single frame at 60+ FPS; now cached and
only re-probed on ROM load or a save/load-slot action).

### Added

- **Manual save-state slots** (the headline item) — `File -> Save State`
  / `Load State` menus with 8 numbered slots per ROM, built on the already-
  real `SaveState` format (`rusty2600-core`, ADR 0007). Each ROM gets its
  own slot directory keyed by an FNV-1a hash of its raw bytes
  (`crate::config::save_slot_path`, under the platform data dir), so a
  loaded ROM's slots can never be silently loaded against a different
  cartridge (`SaveState::restore`'s existing `rom_tag` check enforces this).
  Native-only for now; wasm-side persistence (`localStorage`/IndexedDB) is
  a later release's scope. The File menu shows each slot's status (empty,
  or its last-saved timestamp) probed fresh each frame via cheap filesystem
  `stat` calls, never touching the emu lock.
- **CI-gated performance-regression check** — a new `rusty2600-core::
  system_full_ntsc_frame` Criterion bench drives one full NTSC frame (262
  lines x 228 color clocks) through the whole `System` (real CPU decode,
  TIA register writes, RIOT, and cart — not a per-chip proxy), measuring
  ~1.25 ms/frame, comfortably under the documented <=2 ms/frame target.
  `scripts/bench_regression_check.sh` runs it and fails on a measured mean
  above a fixed absolute ceiling (3.75 ms, ~3x the measured baseline —
  deliberately not relative/percentage-based, since CI-runner timing noise
  would make a relative comparison unreliable), wired as the new `perf` job
  in `.github/workflows/ci.yml`. See `docs/performance.md`.
- **Paddle-timing Stella-oracle differential test** (`T-0501-010`
  follow-up) — `crates/rusty2600-tia/src/paddle.rs` gains a `stella_oracle`
  test module: an independently re-derived copy of Stella's
  `AnalogReadout` RC-circuit formula, cross-checked against the port
  across a position sweep, multi-step charging, VBLANK-dump discharge, and
  the redundant-`set_position` edge case. Confirms every RC constant and
  formula in `paddle.rs` matches Stella's `AnalogReadout.{cxx,hxx}`
  exactly; confirms the port's two-variant `Connection` enum (`Vcc`/
  `Disconnected`) is a deliberate, correct scope match to Stella's own
  `Paddles.cxx` (which never uses `AnalogReadout`'s third `ground`
  connection type — that's exclusive to non-paddle controllers this crate
  doesn't model). A real commercial paddle-game differential cross-check
  (Breakout/Warlords/Kaboom!) remains blocked — no such ROM is legally
  obtainable/stageable in this development environment; see
  `docs/compatibility.md`.
- **ROM loading from `.zip` archives** — both the native `File > Open ROM`
  dialog and the wasm GH-Pages demo's file loader could previously only
  load a bare `.a26`/`.bin`/`.rom` file, not one packaged inside a `.zip`
  (the common ROM-redistribution format). New shared
  `rusty2600-frontend::rom_archive` module extracts the first
  `.a26`/`.bin`/`.rom` entry (by archive order) from an in-memory zip via
  the `zip` crate (`default-features = false`, `deflate` only — pure-Rust
  `flate2`/`miniz_oxide` backend, no C toolchain, no unneeded
  bzip2/lzma/zstd/xz/aes-crypto codec support), reads bounded to a 1 MiB
  ceiling regardless of what the zip's central directory claims (a
  decompression-bomb guard, not just a declared-size check), and never
  panics on malformed/corrupt input. Both the native dialog's filter list
  and the wasm demo's `<input accept>` now include `.zip`.
- **GitHub repo hygiene** — `CODE_OF_CONDUCT.md` (Contributor Covenant
  2.1), `SECURITY.md` (Rusty2600-specific threat model: ROM/zip parsing,
  the Thumb coprocessor interpreter, save-state deserialization, netplay,
  Lua scripting), `.github/CODEOWNERS`, `.github/dependabot.yml`,
  `.github/ISSUE_TEMPLATE/config.yml` + a `scheme_request.md` template
  scoped to the now-closed 26/26 bankswitch catalogue (variant/novel-scheme
  requests only), and GitHub Discussions enabled. The repo description's
  stale "23-of-25 bankswitch schemes" claim (26/26 since `[2.3.0]`) was
  also corrected directly via `gh repo edit`.

### Fixed

- Addressed all 9 findings from PR #1's Copilot + Gemini Code Assist
  automated review (all genuine, none dismissed): cached save-slot status
  instead of re-probing 8 files via `stat` every frame; cleared the
  rewind ring (`EmuCore::snapshots`) when a save-state slot is loaded, so
  `Rewind` no longer jumps to the pre-load timeline; only call
  `cheevos.load_rom()` when `emu.load_rom()` actually succeeds (both at
  startup and the `Open ROM` dialog — a pre-existing issue surfaced
  because this release's zip-loading work touched both call sites
  anyway); pre-allocated the zip-entry extraction buffer using the
  entry's own (capped) declared size; `bench_regression_check.sh` now
  uses `mktemp` instead of a fixed `/tmp` path; two doc-comment fixes
  (an algorithm-name mismatch, a "Supercharker" typo).

## [2.3.0] - 2026-07-02 - "Full Catalogue"

Closes the cart bankswitch catalogue to 26/26 (CDF/CDFJ/CDFJ+, the last
unimplemented scheme) and lands three more follow-up items requested via
`/goal`: DPC+ music-mode audio, script overlay compositing, and a netplay
STUN client. Landed via three independent, parallel implementation
efforts, each independently gate-verified before merging.

### Added

- **CDF/CDFJ/CDFJ+** (`T-0401-006`, BestEffort tier) — `rusty2600-cart`'s
  `BankCdf`: one struct covering all four sub-versions
  (`CDF0`/`CDF1`/`CDFJ`/`CDFJ+`) via a `CdfVersion` const table, ported
  from Gopher2600's Go `cdf` package. Reuses `BankDpcPlus`'s synchronous
  CALLFN-to-`ProgramEnded` ARM entry shape, plus genuinely new mechanics
  DPC+ never needed:
  - **FastJMP** — redirects a `JMP absolute` instruction's own opcode
    fetch (not just an operand) through a data-fetcher stream, guarded by
    a countdown state machine ported to close a real, documented
    false-positive hazard in the reference.
  - **A real `ARMinterrupt` fault-servicing dispatch** — unlike DPC+'s
    no-op-stub equivalent, CDF's driver ROM makes genuine host-serviced
    calls via a `BX` to a fixed non-Thumb address. Caught via
    `rusty2600-thumb`'s existing `Fault::UnimplementedPeripheral` path —
    **no changes to that crate were needed**: `Arm7Tdmi::instruction_pc()`
    already reports the correct call-site address, and `set_register`'s
    existing PC-storage convention already produces the correct resume
    target. Verified with a real hand-assembled Thumb-1 program that
    plants an actual `BX` instruction at the documented call-site offset
    and asserts the targeted music fetcher's frequency field was set by
    the dispatch loop — not just that a fault was caught.
  - CDFJ+'s `fast_ldx`/`fast_ldy`/`datastream_offset` constants are
    derived via a runtime byte-pattern scan of the driver ROM (not a
    fixed table, per the reference). The reference's `NumBanks()` is
    hardcoded to `7` even for CDFJ+ (whose bankswitch table addresses 8
    slots) — ported exactly rather than "fixed" on a guess, since no real
    CDFJ+ ROM was available to verify against.
- **DPC+ music-mode audio** — `BankDpcPlus` gains a `Board::tick()`
  override advancing its 3 music fetchers' phase accumulators (`count +=
  freq` every 59th call, dividing the ~1.19 MHz CPU-cycle tick rate down
  to DPC+'s 20 kHz music-sample rate). Turned out to be a fully
  self-contained `rusty2600-cart` fix — the `Board::tick()` hook already
  existed and was already called at the right rate — contrary to
  `[2.2.0]`'s own speculation that this would be a `rusty2600-tia`
  follow-up. No cross-crate audio mixing needed: the DPC+ driver ROM's
  own 6507 code already reads the now-correctly-time-varying `$05`
  register and writes the result into real `AUDV0`/`AUDV1`.
- **Script overlay compositing** (`scripting` feature) — `app.rs` now
  calls `ScriptState::take_overlay()` each frame and draws the result via
  a new `draw_script_overlay()` function, piggybacked on the frontend's
  existing always-on egui pass (an unclipped foreground layer painter,
  not a new `wgpu::RenderPipeline`) — reusing egui's own font
  rasterization for `drawText` rather than building a glyph atlas from
  scratch. Closes the `[1.9.0]` overlay-compositing gap.
- **Netplay STUN client** (`netplay` feature) — a real RFC 5389 STUN
  client (`rusty2600-netplay::stun`, via `stun_codec`, sans-IO, fitting
  this codebase's 100%-synchronous convention) discovers this machine's
  public NAT-mapped address, plus a best-effort UDP hole-punch and a
  second "Connect via STUN" dialog button. **Live-verified**: a genuine
  round trip against a real public STUN server (`stun.l.google.com:19302`)
  — confirmed passing, independently re-run during release verification.
  WebRTC (browser or native) remains explicitly deferred — it needs
  either browser-only `web-sys` bindings or a native Rust WebRTC stack
  that would pull an async runtime into an otherwise 100% synchronous
  codebase.

### Notes

- **E7 documentation correction, no code change**: `docs/cart.md` carried
  a stale "Curated (not yet implemented — `T-0401-002`)" claim for E7 this
  entire session — `BankE7` has actually been implemented and wired into
  `detect()` since commit `94ca3a4` (2026-07-01), before the `v1.1.0`
  release line even began. This bug was silently copied forward into
  `[2.1.0]`'s and `[2.2.0]`'s own CHANGELOG/STATUS entries (both claimed
  E7 as a remaining unimplemented gap) without independently verifying it
  against the code. Both prior entries are left as published (this
  project never rewrites CHANGELOG history) — this note is the
  correction. **The cart catalogue is now 26 of 26 schemes implemented**
  (E7 was already done; CDF/CDFJ/CDFJ+ closes the actual last gap).
- **Honest verification boundary for netplay STUN**: the STUN client
  itself is genuinely live-tested. Real NAT traversal between two
  independently-NATed peers on different networks is NOT verified — a
  single-host sandbox cannot provide that (loopback never crosses a NAT
  boundary).
- Test count: 313 passing on default features (317 with
  `--features test-roms`), up from 295 at `[2.2.0]`. `scripting` and
  `netplay` features both verified individually and together.
- Additive-feature default-build invariant reconfirmed: `scripting`/
  `netplay` stay off by default and native-only; the `no_std`
  `rusty2600-core` gate is unaffected by any change in this release.

## [2.2.0] - 2026-07-02 - "Coprocessor Online"

Closes the final open item from `[2.1.0]`'s follow-up work: wires DPC+ —
the first of the Harmony/Melody ARM-coprocessor cart families — into
`rusty2600-cart::detect()`, using the `rusty2600-thumb` interpreter that
has existed unconsumed since `[1.6.0]`.

### Added

- **`BankDpcPlus`** (`T-0401-006`, BestEffort tier) — a full port of
  Gopher2600's Go `dpcplus` package (not Stella's C++, matching this
  project's established precedent for ARM-adjacent code). The complete
  `$00..=$7F` register window: RNG, 8 plain + 8 windowed + 8 fractional
  data fetchers, `FastFetch` `LDA #immediate` redirection, and the `$5A`
  CALLFUNCTION register.
  - `DpcPlusArmMemory` implements `rusty2600_thumb::ThumbMemory` over the
    board's driver/custom/data/freq ROM+RAM segments at Gopher2600's own
    Harmony-architecture addresses (Flash `0x0000_0000`, SRAM
    `0x4000_0000`).
  - The ARM entry point (`$5A` write of `254`/`255`) runs
    `Arm7Tdmi::step()` in a loop, **synchronously, from within
    `cpu_write`** — this needed no `Bus`/scheduler change at all, since
    DPC+'s CALLFUNCTION is call-and-run-to-completion, not a per-clock
    tick. A generous, documented step-count safety cap guards against a
    runaway/buggy ROM.
  - Detected via content signature (the ASCII string `"DPC+"` occurring
    twice, matching Stella's own `isProbablyDPCplus`), not size alone — a
    6-bank DPC+ image is the same 32 KiB several other schemes already
    use.
  - **Verified with a real hand-assembled Thumb-1 program** (`MOV`/`LDR`
    PC-relative/`STRB`/`BX LR`, opcodes hand-derived and cross-checked
    against `rusty2600-thumb`'s own format encoders) that actually
    executes via the interpreter and writes a byte into data RAM through
    a genuine `STRB` instruction — proving the ARM coprocessor actually
    runs, not just that registers decode.
  - 12 new tests (295 total on default features, up from 283; 299 with
    `--features test-roms`).

### Notes

- **Honestly deferred, not silently dropped**: DPC+'s music-mode
  continuous-time audio (the reference's `Step(clock)`-driven phase
  accumulator) is not implemented — register plumbing round-trips
  correctly, but waveform sampling always reads index 0, so DPC+
  music-mode audio is silent/incorrect on that one channel. This is a
  `rusty2600-tia` audio-timing follow-up, not a cart-catalogue one.
  Function-call service `2` ("copy value to fetcher, N times") is ported
  with Gopher2600's own address formula verbatim, including what looks
  like a copy-paste artifact (`Hi` is also advanced by the loop index,
  so it does not fill a contiguous block) — Stella can't cross-check this
  specific service (it runs the real ARM driver rather than
  short-circuiting it), so it's ported exactly rather than "corrected" on
  a guess.
- CDF/CDFJ/CDFJ+ (the other three Harmony/Melody families) remain their
  own future, separately-scoped follow-up.
- **Doc fix, found during this release's reconciliation**: `docs/cart.md`'s
  scheme-catalogue tally line said "15 BestEffort (25 schemes)," but the
  table itself has always had 16 BestEffort rows (26 total) — a stale
  count predating F0/3F/3E being split into three distinct rows from the
  source research report's combined "F0 / 3F-variants" draft entry.
  Corrected the tally, not the catalogue — no scheme was added or
  removed by this fix. **24 of 26 schemes** are now implemented and wired
  into `detect()`, leaving E7 (`T-0401-002`, a pre-existing, unrelated
  gap) and CDF/CDFJ/CDFJ+ (this release's own deliberately-deferred
  scope) as the two remaining entries.
- No `Board`/`Bus` architecture changes were needed for this release
  (unlike `[2.1.0]`'s AR/Supercharger, which added
  `Board::take_oob_pokes()`) — DPC+'s synchronous call-and-return
  CALLFUNCTION model fit the existing `cpu_write` hook directly.

## [2.1.0] - 2026-07-02 - "Follow-Through"

Closes four of the follow-up gaps `[2.0.0]`'s reconciliation pass carried
forward: AR/Supercharger, real TIA paddle timing, and frontend wiring for
both Lua scripting and rollback netplay. All landed via three independent,
parallel efforts (each independently verified against the full gate before
merging) since they touch non-overlapping crates.

### Added

- **AR/Supercharger** (`T-0402-015`, BestEffort tier) — `rusty2600-cart`'s
  `BankAr`: a full "fast-load" (ROM-image-only) port of Stella's
  `CartridgeAR`, including a byte-exact port of Stella's 294-byte dummy
  BIOS stub, the `$1FF8` bank-config hotspot, the 5-distinct-access
  delayed-write RAM protocol (reconstructed via `snoop_read`/`snoop_write`
  since this crate has no bus-wide access counter), and the `$1850`
  fast-load multi-load hotspot. Detected via images' distinctive size (one
  or more 8448-byte loads — never collides with any power-of-2-KiB
  scheme). A new `Board::take_oob_pokes()` hook (default empty) lets a
  cart stage direct RIOT-RAM writes bypassing normal bus routing —
  mirrors Stella's own `System::pokeOob`, a genuinely reusable primitive,
  not an AR-specific hack. Sound-load (tape-audio) mode is deliberately
  not ported. Closes the cart catalogue to 24 of 25 schemes.
- **Real TIA paddle timing** (`T-0501-010`) — a faithful port of Stella's
  `AnalogReadout` RC-circuit model (`crates/rusty2600-tia/src/paddle.rs`):
  the same R0/C/R_DUMP/U_SUPP/TRIPPOINT_LINES constants and exponential
  charge/discharge formulas, via `libm` under `no_std`. `INPT0..=INPT3`
  now compute live from four `AnalogPaddle` instances instead of a plain
  stored byte; a new monotonic `Tia::paddle_clock` counter provides the
  RC model's elapsed-time base. Wired end-to-end through every existing
  paddle-position consumer — the native frontend (`SharedInput` widened
  with a second atomic for paddle bytes) and `rusty2600-mobile::run_frame`
  (both the Android and iOS hosts) — making paddle games respond to
  paddle input for the first time on any Rusty2600 platform. Only NTSC
  timing is modeled; PAL/SECAM-specific behavior is honestly unverified.
- **Lua scripting frontend wiring** (`scripting` feature, off by default,
  native-only) — a real `ScriptBus` implementation (`FrontendScriptBus`)
  wired into the render loop, plus a `Tools -> Load/Unload Script` menu
  entry. `FrontendScriptBus` owns a private `System` clone synced from the
  live emulator each tick (a deliberate, documented indirection avoiding
  `unsafe` raw pointers, since `ScriptEngine<B>` needs an owned `B` but
  the emulator lives behind `Arc<Mutex<>>`); `setJoystick`/
  `setConsoleSwitch` record overrides the frontend ORs into the next
  frame's real input. Closes the `[1.9.0]` frontend-wiring gap.
- **Rollback netplay frontend wiring** (`netplay` feature, off by default,
  native-only) — `NetplaySession` wraps `RollbackSession` behind a
  `Tools -> Netplay...` Connect dialog (a single symmetric Connect action,
  matching `RollbackSession::new`'s API). While connected, the background
  emu-thread loop is suppressed and the render loop drives the session
  directly via a new additive `EmuCore::extract_frame()` method. Run-ahead
  is bypassed while netplay is active (documented scope cut — the two
  features have no obvious combined semantics). `WritesLocked` gained a
  real `netplay_active` field, locking script writes during an active
  session. Verified with a real two-local-peer integration test (two
  sessions on `127.0.0.1`, actually synchronizing via GGRS's UDP handshake
  and advancing real frames). Closes the `[1.10.0]` frontend-wiring gap;
  direct-IP/LAN only, STUN/NAT traversal and the WebRTC transport remain
  deferred (need real external infra to verify).

### Notes

- **Version note**: this was originally requested as "v2.0.1," but per
  this project's own SemVer convention (additive work is MINOR, PATCH is
  reserved for fixes — unbroken across all thirteen prior releases this
  line), it ships as `v2.1.0` instead.
- **Honest gaps carried forward, not glossed over**: overlay compositing
  for scripting's `drawText`/`drawRect`/`drawPixel` (pixels aren't
  composited into the render pipeline yet, though the API works); STUN/
  NAT traversal and the WebRTC transport for netplay; console
  switches/paddles still aren't modeled per-player in netplay; no paddle
  test ROM exists in `tests/roms/` to cross-check the new RC simulation
  against real game behavior (validated via unit tests of the RC math
  only); DPC+/CDF/CDFJ/CDFJ+ ARM-coprocessor cart wiring remains
  unattempted (deliberately not rushed — a half-correct ARM coprocessor
  board would be worse than deferring again; closing the catalogue to
  25/25 remains open follow-up work).
- Test count: 283 passing on default features (287 with
  `--features test-roms`), up from 268 at `[2.0.0]`; 86 passing with
  `--features scripting`, 82 with `--features netplay` (both off by
  default, verified individually and together).
- Additive-feature default-build invariant reconfirmed: `scripting`/
  `netplay` are native-only (wasm32-excluded) and off by default; the
  `no_std` `rusty2600-core` gate is unaffected.

## [2.0.0] - 2026-07-02 - "Parity"

The culmination release of the `v1.1.0 -> v2.0.0` RustyNES-parity line. No
code changes from `[1.12.0]` — this is a version-line milestone tag plus a
full documentation/status reconciliation pass confirming every claim across
all twelve prior releases matches what actually shipped, mirroring the
`[1.0.0]` ceremony's own rigor. All four of RustyNES's biggest-ticket
features chosen in-scope by the user — Lua scripting, HD texture packs,
rollback netplay, and mobile builds (Android + iOS) — landed, alongside the
shader stack and TAS movie tooling recommended in-scope by default.

### Status at 2.0.0

- **Persistence & timeline** (`[1.1.0]`/`[1.2.0]`) — a versioned binary
  save-state format (ADR 0007) reusing the core's own `serde` derives; a
  serialized-snapshot rewind ring (replacing a raw full-`System`-clone
  `VecDeque`); run-ahead (`0..=4` frames, off by default) built on the same
  snapshot primitives. A save-state round-trip test is a permanent
  regression gate.
- **Debugger depth** (`[1.3.0]`) — a watch/conditional-breakpoint expression
  engine, a live JSR/RTS call stack, a per-scanline TIA write-scatter
  viewer, a player/missile/ball position panel, and the RetroAchievements
  achievement-list/login/toast UI (`T-0802-005`, closing a `[1.0.0]`-era
  deferral).
- **Shader stack & sprite-pack data model** (`[1.4.0]`) — a composable
  post-process stack (`rusty2600-gfx-shaders`: CRT scanline darkening + an
  honestly-labeled composite-artifact approximation), always compiled in
  but **defaulting to an empty stack byte-identical to the direct blit**;
  the sprite-pack data model (`hd-pack` feature, off by default) — its live
  rendering splice remains a documented follow-up pending a TIA object-ID
  mask.
- **Cart catalog** (`[1.5.0]`) — `Bank4A50` closes 23 of 25 cataloged
  bankswitch schemes. AR/Supercharger and the ARM-driven DPC+/CDF/CDFJ/
  CDFJ+ family remain open, explicitly out of this line's gate (see
  "Explicit non-requirements" below).
- **ARM7TDMI Thumb interpreter** (`[1.6.0]`) — a real Thumb-1 interpreter
  (`rusty2600-thumb`, ported from Gopher2600's Go implementation, 27
  conformance tests) — the substrate the DPC+/CDF/CDFJ/CDFJ+ family will
  wire into via a separately-scoped follow-up.
- **TAS movies** (`[1.7.0]`) — a `.r26m` format (`rusty2600-core::movie`)
  and a TAStudio-lite piano-roll panel, built on the `[1.1.0]` snapshot
  substrate. Live per-frame auto-recording remains a documented follow-up.
- **Accuracy battery** (`[1.8.0]`) — `GoldenLogDiffer` bundles a genuine
  externally-oracled golden CPU trace (20,000 instructions vs. an
  independent Gopher2600 run, `first_divergence() == None`). `T-0602-006`
  (TIA/RIOT-timing test-ROM fixtures) stays a **permanent, honestly
  documented stub** — no freely-redistributable corpus exists; confirmed
  by research, not assumed.
- **Lua scripting** (`[1.9.0]`) — a real, tested `rusty2600-script` crate
  (`mlua` native backend, `scripting` feature off by default and not yet a
  frontend dependency at all) — engine only, frontend wiring and the
  `piccolo` wasm fallback remain open follow-ups.
- **Rollback netplay** (`[1.10.0]`) — a real, tested `rusty2600-netplay`
  crate wrapping `ggrs`, 2-player-only by deliberate scope cut, validated by
  a genuine rollback-desync test — session crate only, not yet a frontend
  dependency; frontend wiring, STUN/hole-punch NAT traversal, and the
  WebRTC transport remain open follow-ups.
- **Mobile builds** (`[1.11.0]`/`[1.12.0]`) — `rusty2600-mobile`, one UniFFI
  bridge crate driving both a real Android app (`android/`, **verified
  running on a real KVM-accelerated emulator** — booted, installed, launched
  crash-free, a ROM loaded via the real system file picker) and a real iOS
  app (`ios/`, genuinely tool-generated Swift bindings + real SwiftUI/Metal/
  AVFoundation source, **honestly unverified by compilation** — this
  development sandbox has no Xcode/`xcrun`/`swift` toolchain at all, so no
  Xcode build, Simulator run, or device run was possible). Neither mobile
  host exposes save/load-state UI yet; the iOS build additionally needs a
  real Mac to complete verification before it reaches the Android build's
  bar. A pre-existing, project-wide gap discovered along the way
  (`T-0501-010`): the TIA has no real analog dump-capacitor paddle-timing
  simulation on any platform yet.
- **Testing** — 268 tests passing workspace-wide (272 with
  `--features test-roms`), up from 151 (154) at `[1.0.0]`. Full CI matrix
  green — Linux/macOS/Windows + the `no_std` gate — on every tagged release
  since `[0.5.0]`, unbroken through `[2.0.0]`.
- **Release matrix** — three desktop platform archives (Linux/macOS/
  Windows) built and published for every tagged release; the wasm/GH Pages
  demo deploys on every push to `main` (vertical-crop and missing-audio bugs
  found and fixed this line); the Android build is verified on a real
  emulator; the iOS build is source-complete but not yet Xcode-verified
  (see above) — **the one release-matrix cell not fully green at `2.0.0`**,
  carried forward honestly rather than glossed over.
- **Additive-feature default-build invariant** — confirmed by inspection:
  `rusty2600-script`/`rusty2600-netplay`/`rusty2600-mobile` are not
  dependencies of `rusty2600-frontend` at all (unwired, not merely
  feature-gated); `hd-pack` and `retroachievements` are off-by-default
  Cargo features; the shader stack is always compiled but its zero-pass
  default is byte-identical to the direct blit. The plain default-feature
  desktop build and the `no_std` `rusty2600-core` gate both stay green
  through every release in this line.

### Explicit non-requirements (unchanged posture from `[1.0.0]`)

Per `to-dos/ROADMAP.md`'s "Explicit scope note": unlike RustyNES's own
literal "v2.0.0" (a fractional master-clock timebase rewrite closing hard
sub-scanline residuals — ADR 0002 already considered and rejected that
exact rewrite for the 2600, "likely never needed"), Rusty2600's `v2.0.0` is
the RustyNES-parity **culmination milestone**, not an accuracy-architecture
rewrite. Mobile store production launch (Play Store / App Store submission,
monetization) stays explicitly out of scope, deferred beyond `v2.0.0` —
matching RustyNES's own `v2.1.0` precedent exactly as planned. AR/
Supercharger, the DPC+/CDF/CDFJ/CDFJ+ wiring, frontend wiring for scripting/
netplay, real TIA paddle timing, and full Xcode-verified iOS build/run all
continue as their own separately-scoped follow-up work — none of these gate
`2.0.0` per the plan's own explicit gate criteria.

### Notes

- No code changes from `[1.12.0]` — version bump only (workspace + all 13
  crates), plus this changelog entry and a full `README.md`/`docs/STATUS.md`/
  `to-dos/ROADMAP.md` reconciliation pass confirming every number and claim
  matches the shipped `v2.0.0` tag.
- Twelve minor releases (`v1.1.0` through `v1.12.0`) shipped every
  big-ticket item the user chose in-scope, each as a real, tested,
  honestly-scoped release rather than a rushed or partial claim — the same
  "land a real core now, document what's deferred" discipline held from
  `v1.1.0`'s save-states through `v1.12.0`'s iOS build.

## [1.12.0] - 2026-07-02 - "Pocket"

A SwiftUI iOS app reusing `[1.11.0]`'s `rusty2600-mobile` bridge unchanged,
plus genuinely new virtual-analog-paddle UX design work. Honestly scoped to
what a Linux sandbox without an Xcode toolchain can actually verify.

### Added

- **`ios/`** (new tree, reusing `rusty2600-mobile` with zero Rust changes):
  - **`ios/RustyMobileFFI/`** — a local Swift Package wrapping the FFI
    bridge. `Sources/RustyMobileFFI/rusty2600_mobile.swift` (1,523 lines)
    is genuinely tool-generated — produced by an actual
    `cargo build -p rusty2600-mobile --release` followed by
    `uniffi-bindgen generate --language swift` run on this Linux box, not
    hand-written — matching the same trusted codegen path `[1.11.0]`'s
    Kotlin bindings used. `Package.swift` declares a `binaryTarget`
    pointing at `Rusty2600Mobile.xcframework`, which intentionally does
    not exist in this checkout yet (building it needs
    `aarch64-apple-ios`/`aarch64-apple-ios-sim` Rust cross-compilation on
    a real Mac with Xcode — impossible on Linux).
  - **`ios/Rusty2600/Sources/`** — real SwiftUI/Metal/AVFoundation app
    source: `EmulatorView` (a Metal `MTKView` blitting `FrameOutput.rgba`
    into an `MTLTexture` via a single full-screen-triangle blit, not a
    shader stack), `AudioEngine` (`AVAudioEngine` + `AVAudioPlayerNode`
    scheduling `FrameOutput.audioSamples`), `EmulatorViewModel` (the
    `ObservableObject` owning the emulator instance and ~60Hz run loop),
    `ContentView` (ROM loading via `.fileImporter`, on-screen controls),
    and **`PaddleControlView`** — the genuinely new UX work: a touch-drag
    rotary dial (not accelerometer/tilt) mapping deterministically to
    `MobilePaddle.position`, modeled as a clamped -150°...+150° arc
    matching a real paddle's fixed-sweep potentiometer. RustyNES's
    d-pad-only touch overlay never had to solve an analog input problem;
    this is the first one either Rusty2600 mobile host has needed.
  - `ios/regenerate-bindings.sh` documents the real end-to-end process
    (cross-compile both Apple targets, assemble the xcframework via
    `xcodebuild -create-xcframework`, regenerate bindings) for a real Mac
    to run — the xcframework-assembly step is explicitly commented as
    requiring Xcode, not silently presented as Linux-runnable.

### Notes

- **Explicit hard environment constraint, honestly documented rather than
  glossed over**: this development sandbox is Linux with no
  `xcodebuild`/`xcrun`/`swift` at all. Unlike `[1.11.0]`'s Android build
  (verified on a real KVM-accelerated emulator), **no Xcode build, no iOS
  Simulator run, and no device run were performed or are possible here**.
  The Swift bindings are genuinely tool-generated (see above); the
  SwiftUI/Metal/AVFoundation source was written directly against that
  generated API's real method/type signatures, but has never been
  compiled by `swiftc` — unverified by compilation. No `.xcodeproj` exists
  in this checkout either: a hand-authored `project.pbxproj` was
  deliberately avoided (fragile to write correctly by hand, and
  unverifiable without Xcode itself); `ios/RustyMobileFFI/` is instead a
  real, independently-valid Swift Package ready to be added as a local
  dependency to a fresh Xcode iOS App project. A `v1.12.x` follow-up on a
  real Mac needs to run `regenerate-bindings.sh` in full, create that
  Xcode project, and actually build + run on Simulator (and ideally a
  physical device) before this reaches the Android build's verification
  bar. See `docs/mobile.md`'s "iOS Verification" section for full detail.
- **A pre-existing, project-wide gap discovered and documented, not fixed
  here**: `MobileInput.paddle0..=paddle3` are wired end-to-end from the
  new `PaddleControlView` through every `run_frame` call, but
  `rusty2600-mobile`'s `run_frame` (unchanged this release) never
  forwards those fields into `system.bus.tia.inpt[0..=3]` — the TIA has
  no real analog dump-capacitor charge-timing simulation anywhere in the
  engine yet, on any platform (`rusty2600-frontend`'s `emu_thread.rs`
  documents the identical gap as `T-0501-010`). Implementing real
  capacitor-charge timing is a `rusty2600-tia` accuracy task, out of
  scope for a mobile-bridge release. Paddle games (Breakout, Warlords,
  Kaboom!) will not respond to the paddle control yet on any platform —
  inherited scope, not new breakage from this release.
- Save/load-state UI and HD-pack loading aren't exposed in the iOS app
  either — same as the Android app, the bridge crate already supports
  save-states.
- App Store submission stays explicitly out of scope, deferred beyond
  `v2.0.0` per RustyNES's own `v2.1.0` precedent — same posture as Play
  Store submission at `[1.11.0]`.
- Rust side untouched: `cargo build -p rusty2600-mobile --release`
  recompiles 0 crates versus `[1.11.0]`; the full workspace test count is
  unchanged at 268 passing (272 with `--features test-roms`).

## [1.11.0] - 2026-07-02 - "Handheld"

A new `rusty2600-mobile` crate and a real Android app, verified running on
a real emulator — a design improvement over the original plan eliminated
the need for a dedicated native-glue crate entirely.

### Added

- **`rusty2600-mobile`** (new crate): a `std`, host-testable,
  platform-agnostic [UniFFI](https://mozilla.github.io/uniffi-rs/) bridge
  over `rusty2600_core::System`, reusable from both Kotlin (Android, this
  release) and Swift (`v1.12.0` "Pocket", iOS) — one Rust implementation,
  two mobile hosts.
  - `MobileEmulator` — `new()`, `load_rom(bytes, rom_tag)`,
    `is_rom_loaded()`, `run_frame(input)`, `save_state()`,
    `load_state(bytes)`.
  - `MobileInput` — two joysticks, four paddles, console switches (named
    fields, since UniFFI's `Record` derive doesn't support fixed-size
    arrays).
  - `FrameOutput` — `rgba: Vec<u8>` (160x192 RGBA8, the same crop the
    native/wasm frontends already apply) + `audio_samples: Vec<f32>`
    (DC-blocked, normalized, at the TIA's native rate).
  - 6 tests: garbage-ROM rejection, run-frame-without-ROM error path,
    correct framebuffer/audio sizing, save/load-state round-trip.
- **A real Android app** (`android/`): AGP 8.6 / Kotlin 1.9 /
  `compileSdk 34` / `minSdk 26`. `EmulatorView` (a custom `View` blitting
  the RGBA8 framebuffer via `Bitmap.copyPixelsFromBuffer`) and
  `MainActivity` (loads a ROM via the system file picker, drives
  `run_frame` at ~60 Hz on a background thread, plays audio through an
  `AudioTrack` in `ENCODING_PCM_FLOAT` mode, and wires on-screen
  Up/Down/Left/Right/Fire/Select/Reset buttons). `cargo-ndk`
  cross-compiles the bridge for `arm64-v8a` (real hardware) and `x86_64`
  (the emulator ABI); UniFFI generates the Kotlin bindings (2,029 lines)
  from the compiled library.

### Notes

- **Real design improvement over the original plan, not a corner cut**:
  the plan called for a dedicated `rusty2600-android` crate doing JNI,
  `ANativeWindow`→wgpu rendering, and AAudio — "the only `unsafe`
  surface." The actual design instead has `run_frame` return plain owned
  data (`FrameOutput`) rather than a native rendering-surface handle, so
  Android's stock `Bitmap`/`AudioTrack` APIs consume it directly via
  UniFFI's generated JNA-based Kotlin bindings. Result: **zero
  hand-written JNI or `unsafe` on either side of the bridge** — fewer
  `unsafe` surfaces than the plan anticipated, not an isolated one. See
  `docs/mobile.md`'s "Design deviation" section for the full rationale.
- **Verified running on a real Android emulator** (`Pixel_8_API_34`,
  x86_64, KVM-accelerated) — not just "it compiles": booted, `adb install`
  succeeded, `am start` launched `MainActivity` with no crash (confirmed
  via `logcat` — no `FATAL EXCEPTION`/`AndroidRuntime` for the package), a
  synthetic 4K test ROM was pushed and loaded through the real system file
  picker (Storage Access Framework) with no crash and the background
  emulation thread visibly consuming CPU (direct evidence `run_frame` is
  executing continuously, not silently failing), and screenshots were
  captured confirming the UI rendered correctly.
- **Not verified this release**: visual output from a ROM that actually
  writes TIA color registers (the synthetic test ROM used for
  verification never writes `COLUBK`, so the black post-load screen is
  expected, not a bug); a physical device (only the emulator was
  available); Play Store packaging (explicitly out of scope, deferred
  beyond `v2.0.0` per RustyNES's own precedent). Save/load-state UI and
  paddle input aren't exposed in the Android app yet either — the bridge
  crate already supports them.
- Fixed a stale documentation-honesty issue in `README.md` found while
  investigating GH Pages bugs earlier this window: the `WebAssembly` rows
  claimed a `wasm-winit` (full winit+wgpu+egui) build mode exists
  separately from `wasm-canvas`; only the canvas-2D bootstrap actually
  exists (both feature names are currently identical placeholders, per
  `rusty2600-frontend`'s own `Cargo.toml` comment).
- 268 tests passing workspace-wide (+6 for the new `rusty2600-mobile`
  tests), up from 262 at `[1.10.0]`; 272 with `--features test-roms`.

## [1.10.0] - 2026-07-01 - "Rollback"

A new `rusty2600-netplay` crate: 2-player rollback netplay wrapping `ggrs`
(GGPO-style), not a from-scratch reimplementation. The session crate
lands in this release; frontend wiring is an explicitly-scoped follow-up.

### Added

- **`rusty2600-netplay`** (new crate): 2-player-only rollback netplay,
  a deliberate scope cut vs. RustyNES's own 2-4-player mesh — real 2600
  hardware rarely supported more than 2 controllers, so a fully-connected
  N-peer mesh would be pure RustyNES-parity cost with little real payoff.
  - `PortInput` — one player's joystick contribution (four directions +
    fire). Deliberately NOT `rusty2600_core::MovieFrame` as `ggrs::Config::Input`:
    `MovieFrame` packs BOTH joystick ports together (a whole-machine-state
    record), but GGRS's input type is fundamentally per-player; using
    `MovieFrame` as-is would let one player's "input" silently smuggle the
    other's port bits.
  - `RustyConfig` — the `ggrs::Config` binding (`Input = PortInput`,
    `State = Vec<u8>` (a `SaveState`-encoded blob), `Address = SocketAddr`).
  - `RollbackSession` — wraps `ggrs::P2PSession`, using GGRS's own
    built-in `UdpNonBlockingSocket` (no custom transport implementation
    needed). `DEFAULT_INPUT_DELAY = 2` / `DEFAULT_MAX_PREDICTION_WINDOW = 8`
    match GGPO convention exactly.
  - `resync()` reuses `[1.1.0]`'s `SaveState` capture/restore directly —
    no new determinism infrastructure needed, per ADR 0004's own citation
    of save-states as "the basis for rewind, run-ahead, and netplay
    rollback."
  - **A genuine rollback-desync test**, not a decorative one: uses
    `ggrs::SyncTestSession` (GGRS's own built-in determinism-testing
    session) driving a real `rusty2600_core::System` loaded with a
    synthetic 4K ROM whose state visibly depends on joystick input, across
    12 frames of varied two-player input. `SyncTestSession` internally
    saves, advances, rewinds, re-simulates, and panics on any checksum
    mismatch — reaching the end without a panic is the test's actual pass
    condition. Validated for real: an initial version using a bare
    `System::new()` with no cartridge passed vacuously; fixed by adding the
    input-reactive ROM and a real checksum, then re-confirmed by
    deliberately reintroducing the same bug and watching the test
    correctly fail.

### Notes

- **This release lands the session crate only.** `rusty2600-netplay` is
  not yet wired into `rusty2600-frontend`: no host/join-game menu, no live
  per-frame input capture feeding a running session. A `v1.10.x`
  follow-up does that wiring — the same pattern `rusty2600-thumb`
  (`[1.6.0]`) and `rusty2600-script` (`[1.9.0]`) already established. See
  `docs/netplay.md` for the full scope.
- **Direct-IP/LAN connection only this release** — STUN/hole-punch NAT
  traversal for real internet play is deferred to `v1.10.x`: implementing
  and verifying real NAT traversal needs a genuine external peer no
  sandbox can provide, and is substantial, separable work from the
  rollback session logic itself.
- **WebRTC browser transport is also deferred to `v1.10.x`** per the
  original plan — this crate is native-only.
- Console switches (Select/Reset/Color/Difficulty) and paddles are not
  modeled per-player this pass — there's no natural "which peer owns
  this" mapping for shared machine-level switches in a 2-player session.
  A session runs with switches idle and paddles centered until revisited.
- A real stack-overflow bug was found and fixed while building the
  desync test: `rusty2600_cart::Cartridge`'s enum is sized to its largest
  variant (`BankF4`'s inline 32 KiB ROM array), so several nested
  `System::clone()`s (inside GGRS's own rollback recursion) overflowed the
  default ~2 MiB test-thread stack. The test now runs on an explicit
  32 MiB-stack thread.
- 262 tests passing workspace-wide (+6 for the new `rusty2600-netplay`
  tests), up from 256 at `[1.9.0]`; 266 with `--features test-roms`.

## [1.9.0] - 2026-07-01 - "Scriptable"

A new `rusty2600-script` crate: a real, tested Lua scripting engine, off
by default. The engine lands in this release; frontend wiring is an
explicitly-scoped follow-up.

### Added

- **`rusty2600-script`** (new crate, `std`-only, `unsafe`-permitted — the
  one exception besides `rusty2600-cheevos`, both for C-FFI reasons):
  `mlua` native backend (vendors Lua 5.4, no system Lua install needed),
  exposing a deliberately-smaller-than-RustyNES `emu` table:
  - `emu.peek(addr)` / `emu.poke(addr, val)` — mirror
    `rusty2600_core::Bus::peek`/`cpu_write`.
  - `emu.cpu()` — a read-only 6507 register-file snapshot.
  - `emu.onFrame(fn)` — a per-frame callback.
  - `emu.setJoystick(port, direction, pressed)` /
    `emu.setConsoleSwitch(name, value)` — the gated input-override
    surface.
  - `emu.drawText`/`drawRect`/`drawPixel` — script-driven overlay
    primitives, accumulated per frame.
  - `emu.pause()` / `emu.saveState()` / `emu.loadState(bytes)` — wraps the
    existing `[1.1.0]` `SaveState` capture/restore.
  - All of the above sit behind `ScriptBus`, a host-agnostic trait — this
    crate never touches `rusty2600-core`/`rusty2600-frontend` types
    directly; a host implements the trait over whatever real state it
    owns.
  - `WritesLocked`: the determinism gate every script-driven WRITE is
    checked against. Folds exactly one real lock source today
    (RetroAchievements hardcore mode) — `.r26m` movie and rollback-netplay
    locks are documented, NOT stubbed, future additions, added when those
    subsystems get real lock semantics of their own.
  - 18 hand-authored tests: peek/poke round-trip, lock rejection for
    `poke`/`setJoystick` (verifying no side effect occurs, not just that
    an error is raised), joystick/console-switch recording and
    unknown-name rejection, CPU snapshot exposure, `onFrame` firing across
    multiple ticks, `pause`, save/load-state round-trip and bad-blob error
    surfacing, and draw-primitive accumulation.

### Notes

- **This release lands the engine only.** `rusty2600-script` is not yet
  wired into `rusty2600-frontend`: no `scripting` feature flag, no live
  `ScriptBus` implementation over the real `Bus`/`Cpu`, no overlay
  compositing in the render pipeline, no `onFrame` hook tied to
  `EmuCore::run_frame`. A `v1.9.x` follow-up does that wiring — the same
  pattern `rusty2600-thumb` (`[1.6.0]`, cart-board wiring deferred) and
  `.r26m` movies (`[1.7.0]`, live recording deferred) already established
  for this project. See `docs/scripting.md` for the full scope.
- **A `piccolo` pure-Rust wasm-fallback backend is also deferred.**
  `piccolo` is a materially less mature project than `mlua` (fewer
  standard-library facilities, less battle-tested) — landing a second,
  non-byte-parity scripting backend alongside a brand-new API surface in
  the same pass risked shipping either half-tested. The `emu` table and
  `ScriptBus` seam are designed backend-agnostically, so a `piccolo`
  backend remains a genuine, scoped follow-up rather than a rewrite.
- Explicitly out of scope (per the original plan): TAStudio-integrated
  scripting, host-IPC (`comm.*`), SQLite-backed `userdata`.
- 256 tests passing workspace-wide (+18 for the new `rusty2600-script`
  engine tests), up from 238 at `[1.8.0]`; 260 with `--features test-roms`.

## [1.8.0] - 2026-07-01 - "Oracle"

Accuracy battery depth, grown honestly: a genuine externally-oracled
golden CPU trace closes `T-0602-007`; `T-0602-006` stays a documented,
permanent scope boundary rather than a gap papered over.

### Added

- **A genuine externally-oracled golden CPU trace** (`T-0602-007`):
  `tests/golden/klaus_functional_test_gopher2600.trace` bundles the first
  20,000 retired instructions of the CC0 Klaus `6502_functional_test`,
  captured by running Gopher2600's `hardware/cpu` package directly against
  the identical ROM (the same technique its own `cpu_test.go` uses to
  unit-test the CPU in isolation, via the project's established
  differential-oracle workflow). `GoldenLogDiffer::bundled()` now reports
  `true`. A new integration test,
  `crates/rusty2600-test-harness/tests/golden_log_test.rs`, runs
  Rusty2600's own `Cpu` over the same instructions from an explicit common
  baseline and asserts `first_divergence() == None` — two
  independently-implemented 6502 cores agreeing register-for-register and
  cycle-for-cycle, real external validation distinct from Klaus's own
  internal pass/fail trap (which only proves the ROM's self-check passed,
  not that either emulator matches an independent reference).

### Notes

- **`T-0602-006` (a TIA-timing test-ROM corpus for Layer 3's
  `run_until_complete`) stays a permanent stub, not a gap to close later.**
  Researched again this release: no freely-redistributable 2600-specific
  TIA/RIOT test-ROM corpus exists. Gopher2600's own README makes the same
  admission (it has never obtained permission to redistribute its
  TIA/RIOT test ROMs either), and the Atari Diagnostic Test Cartridge 2.0
  is official service-center software, not freely redistributable.
  TIA/RIOT accuracy work continues via the differential-oracle method
  against specific known-hard commercial titles — the same technique that
  already found the real Frogger WSYNC-jitter and Pitfall II RIOT-timer
  bugs — not a canned test-ROM suite.
- The golden trace is deliberately bounded to 20,000 instructions, not the
  full ~30-million-instruction Klaus run: a full trace would be an
  impractically large committed fixture, and 20,000 instructions already
  is a real, meaningful confirmation over the test's early control flow.
- Both sides of the cross-check start from an explicit, documented common
  baseline (`A=X=Y=0, SP=0xFF, P=0x34, PC=0x0400`) rather than each
  emulator's own reset-vector-fetch ceremony, which is an
  implementation-specific detail neither side claims to model identically
  and isn't a real accuracy question the Klaus test exercises.
- The PAL 192-vs-228 visible-line-budget question (`docs/compatibility.md`)
  remains open, per ADR 0003's honesty principle — no new evidence
  surfaced this release, so it stays flagged rather than guessed.
- 238 tests passing workspace-wide (unchanged — the new golden-trace test
  is `--features test-roms`-gated); 242 with `--features test-roms`
  (+1 vs. `[1.7.0]`).

## [1.7.0] - 2026-07-01 - "Chronicle"

A `.r26m` TAS movie format plus a TAStudio-lite piano-roll debugger panel,
built on `[1.1.0]`'s save-state snapshot substrate.

### Added

- **`rusty2600-core::movie`** (new module, `no_std`-compatible): a `.r26m`
  TAS movie format mirroring `save_state.rs`'s header conventions (magic
  `"R26M"`, format version, `rom_tag`, `postcard`-encoded, typed
  `MovieError`) without depending on it beyond reusing `SaveState`'s
  encoding for embedded branch points.
  - `MovieStart::PowerOn { seed }` — a fresh power-on, exercising ADR
    0006's deterministic seeded RIOT RAM / CPU `A`/`X`/`Y`.
  - `MovieStart::FromSaveState(Vec<u8>)` — an embedded `SaveState`-encoded
    blob. A branch point is exactly this: `Movie::new_branch` captures the
    running `System` via `SaveState::capture`, no parallel snapshot system.
  - `MovieFrame` — one frame's worth of joystick directions/fire, four
    paddle positions + fire, and console switches (per-frame fields, since
    Select/Reset/Color/Difficulty can all change mid-run on real
    hardware, unlike a fixed NES controller). Packed to mirror the
    RIOT/TIA port-byte conventions `rusty2600-frontend::input` already
    established.
  - `MovieRegion` — a small, deliberate 3-variant duplication of
    `rusty2600-frontend::palette::Region` (the crate graph is
    one-directional; core cannot depend on the frontend crate).
- **`rusty2600-frontend::debugger::tastudio_panel`**: a piano-roll input
  grid (click-to-toggle editing), deliberately scoped below RustyNES's own
  ~609-line TAStudio panel. Jump-to-frame reuses the existing `[1.1.0]`
  rewind ring (`EmuCore::snapshots`/`rewind`) rather than a new "greenzone"
  structure; branch points save as separate `.r26m` files via
  `Movie::new_branch`.
- **Two riders**: `debugger::access_counter` (a per-address write-count
  heatmap sourced from the existing `[1.3.0]` `Bus::write_log`) and
  `debugger::memory_compare_panel` (a byte-diff between two memory
  snapshots) — both small, generic, address-space-agnostic tools landed
  alongside the TAS work since they're cheap.

### Notes

- **Live per-frame recording is not yet wired into `EmuCore::run_frame`'s
  hot path.** The format, the panel's state machine, and manual
  jump-to-frame/save-branch actions are real and tested, but nothing
  automatically appends a `MovieFrame` every frame the emulator runs yet —
  the same honest-partial-landing call this project made for `[1.4.0]`'s
  sprite-pack data model (shipped without its render splice, clearly
  documented as deferred rather than rushed or silently skipped). See
  `docs/movie.md` for the full scope.
- Explicitly out of scope: foreign movie-format import (no existing 2600
  movie format to import from) and branch-tree visualization (a flat list
  of saved branch files is enough for a first cut).
- `MovieFrame`'s `Default` is hand-implemented, not derived: `swcha`/
  `swchb` default to `0xFF` (idle, matching real hardware's active-low
  pull-ups) — a naive all-zero derive would instead mean "every direction
  and switch held down simultaneously," caught by the module's own tests.
- 238 tests passing workspace-wide (241 with `--features test-roms`; +21
  for the new `.r26m` movie/TAStudio/access-counter/memory-compare tests),
  up from 217/220 at `[1.6.0]`.

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
