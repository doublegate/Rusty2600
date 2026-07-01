# Changelog

All notable changes to Rusty2600 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
