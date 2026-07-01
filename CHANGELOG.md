# Changelog

All notable changes to Rusty2600 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
  release); resolved as part of this pass.

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
