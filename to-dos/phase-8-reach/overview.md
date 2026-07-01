# Phase 8 — Reach

**Goal:** the reach features that turn the accurate core into a modern emulator —
rollback netplay, RetroAchievements, TAS movies, a Lua scripting engine, and the
shader / video-filter ecosystem — every one **additive and off by default**, so
the shipped / native / `no_std` / wasm builds stay byte-identical with the
features off and the determinism contract (ADR 0004) is never perturbed.

References: `docs/frontend.md`; `docs/adr/0004` (determinism — netplay rollback +
TAS replay depend on it); `docs/performance.md`; `ref-docs/research-report.md` §9;
the RustyNES reach lineage (netplay / RA / Lua / shaders) as the structural
template.

## Scope

In, each behind its own default-off feature flag: GGPO-style rollback netplay (2-4
players, UDP native + WebRTC browser) built on save-state snapshot/restore;
RetroAchievements (opt-in, native-only, vendored rcheevos FFI); TAS movies
(record/play/branch + a deterministic movie format) and a TAStudio-style editor;
a Lua scripting engine (memory peek/poke, frame hooks, input injection — write
access gated); the composable shader stack (CRT / scanline / NTSC filter +
upscalers); audio depth (EQ / per-channel mix). The honesty + determinism gates
remain authoritative.

Out: any change to the core synthesis that introduces hidden non-determinism;
breaking the byte-identical "features-off" guarantee.

## Exit criteria (verifiable)

- With every reach feature OFF, the native / `no_std` / wasm builds are
  byte-identical to the Phase 7 cut (only the embedded version string differs)
  and the accuracy battery is unchanged.
- Netplay: a 2-player rollback session stays in sync across a forced-desync probe
  (the determinism contract holds under rollback).
- TAS: a recorded movie replays bit-identically (same seed + ROM + input).
- RetroAchievements: the native client authenticates and unlocks against a test
  game; the wasm build never links the FFI.
- Lua: the bundled example scripts run; write access is gated behind the same
  opt-in as RA / netplay.
- The shader stack composes (CRT + scanline + upscaler) and a filter-less build
  is pixel-identical to the direct blit.

## Sprints

- Sprint 1 — rollback netplay (UDP + WebRTC) on snapshot/restore →
  `sprint-1-netplay.md` (`T-0801-NNN`).
- Sprint 2 — RetroAchievements + TAS movies + the TAStudio editor →
  `sprint-2-ra-and-tas.md` (`T-0802-NNN`).
- Sprint 3 — Lua scripting + the shader / filter ecosystem + audio depth →
  `sprint-3-lua-shaders.md` (`T-0803-NNN`).

## Risks

- Any reach feature that touches core synthesis can leak non-determinism (wall
  clock, OS RNG, thread scheduling) and break netplay / TAS / save-states at once
  (ADR 0004) — keep rate control + rollback orchestration in the frontend.
- The "byte-identical with features off" guarantee is easy to regress when a
  flag's plumbing isn't fully `cfg`-gated — verify the off-build hash each PR.
- RA / Lua FFI carries `unsafe` and C deps; gate them native-only so the wasm
  size budget and the `forbid(unsafe_code)` chip stack are untouched.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
