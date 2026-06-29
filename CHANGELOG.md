# Changelog

## [Unreleased]
### Added
- Phase and Sprint `.md` files populated under `to-dos/` tracing the roadmap from CPU Golden Log to full reach.
- Commercial test ROMs extracted and copied to `tests/roms/external/` for robust mapper and coprocessor verification.
- Klaus2m5 and Tom Harte ProcessorTests test suites downloaded to `tests/roms/Klaus2m5/` and `tests/golden/ProcessorTests/`.
- Stella oracle test ROMs mapped for TIA behavior and timing testing.
- Markdown documentation and .gitignore updated to reflect the new test infrastructure.



All notable changes to Rusty2600 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Initial workspace scaffold: the eight-crate cycle-accurate architecture
  (`rusty2600-cpu` / `-tia` / `-riot` / `-cart` / `-core` / `-frontend` /
  `-cheevos` / `-test-harness`) ported from the RustyNES design.
- The master-clock lockstep scheduler (TIA color clock timebase, 6507 on every
  third clock, `WSYNC`/`RDY` beam-stall) and the seeded determinism contract.
- The Bus (owns the TIA + RIOT + cart board + open-bus latch; no separate WRAM)
  and the Core-tier cart boards (2K / 4K / F8) behind the accuracy-tiering
  honesty gate (ADR 0003).
- The `winit + wgpu + cpal + egui` frontend shell: the always-on egui pass, the
  wgpu framebuffer-blit present path, the 2600 input map (joystick / paddle /
  console switches), the NTSC/PAL/SECAM palette + region model, and the
  6507/TIA/RIOT/memory debugger-panel scaffold. Opens a window and presents a
  cleared frame (the chip cores are skeletons).
- The full documentation set (`docs/` spec + four ADRs), the immutable research
  corpus (`ref-docs/research-report.md`), and the Phase 0-8 roadmap.

### Notes

- This is a v0.1.0 compiling skeleton: the workspace, scheduler, frontend shell,
  and docs are in place; the per-chip engines (6507 / TIA video+audio / RIOT /
  the cart long-tail) are `// TODO`. See `docs/STATUS.md` for the authoritative
  per-suite / per-chip state.
