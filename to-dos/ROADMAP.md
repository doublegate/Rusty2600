# Rusty2600 — Roadmap

Entry point for planning. Each phase links its overview; phases contain sprints;
sprints contain tickets with stable IDs `T-PPSS-NNN` (`PP` = 2-digit phase,
`SS` = 2-digit sprint, `NNN` = 3-digit ticket sequence, all zero-padded),
e.g. `T-0001-003` = phase 0, sprint 1, ticket 3. Reference them in commit
messages. References: `ref-docs/research-report.md`; `docs/architecture.md`;
`docs/STATUS.md` (current-state source of truth).

**Current release: v2.1.0 "Follow-Through"** — a post-`v2.0.0` follow-up
release closing four of the gaps the `[2.0.0]` reconciliation pass
explicitly carried forward: AR/Supercharger (`T-0402-015`, closing the
cart catalogue to 24/25), real TIA paddle timing (`T-0501-010`, a faithful
port of Stella's `AnalogReadout` RC model, wired end-to-end through
native/Android/iOS), and frontend wiring for both Lua scripting
(`scripting` feature) and rollback netplay (`netplay` feature) — both off
by default, native-only, closing the `[1.9.0]`/`[1.10.0]` frontend-wiring
gaps. Landed via three independent, parallel implementation efforts, each
independently gate-verified before merging. Still open, deliberately not
rushed: DPC+/CDF/CDFJ/CDFJ+ ARM-coprocessor wiring (closing the catalogue
to 25/25), overlay compositing for scripting, STUN/NAT traversal + WebRTC
for netplay, and the `[1.12.0]`-carried-forward Xcode-verified iOS
build/run. See `CHANGELOG.md`'s `[2.1.0]` entry for full detail.

Full release-by-release detail (v1.1.0 through v2.0.0) lives in
`docs/STATUS.md`'s "Current release" section and `CHANGELOG.md` — not
duplicated here to avoid this file drifting out of sync with the
canonical status doc. Phase 0 (foundation)
through the full Curated-tier board set (Phase 4) are complete. Phase 7
(BestEffort breadth) has landed 15 of the ~15-scheme BestEffort long tail
cataloged in `docs/cart.md` (F0, E0, 3F, 3E, EF/EFSC, DF/DFSC, BF/BFSC, UA,
0840, FE, SB, X07, 4A50, AR/Supercharger — 24 of 25 total schemes in the
LOCAL catalogue), leaving only the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family
(`T-0401-006`) — its interpreter substrate (`rusty2600-thumb`) exists but
isn't yet wired into any `Board`, that wiring remaining its own
separately-scoped follow-up, one family at a time.
`Board::snoop_write`/`snoop_read` (added v0.4.0/v0.4.1) underpin all of
UA/0840/FE/SB/X07/4A50/AR. **Phase 5 (frontend) is fully complete** — the real
`debug-hooks` debugger (6507/TIA/RIOT/memory panels, breakpoints/step/
continue, a side-effect-free `Bus::peek`/`peek_range`, a standalone
disassembler) shipped in v0.5.0, and the four chip-crate Criterion benches
are populated with real measured baselines (`docs/performance.md`). The
RetroAchievements slice of Phase 8 is now real: `rusty2600-cheevos` vendors
the `rcheevos` C library and wires a safe `RaClient` into the frontend
(`retroachievements`, off by default) — per-frame achievement tracking,
hardcore mode, and a menu all work; a dedicated achievement-list/login/toast
UI is deferred (`T-0802-005`). **Phase 6's accuracy battery (Layer 4) is now
real** — `rusty2600-test-harness` goes from unused scaffolding to a real
battery: the shared `Sentinel`/`run_cpu_until_sentinel` Layer 2 runner (both
bundled Klaus oracles refactored onto it), a real `AccuracyScore`-gated
`tests/accuracy_battery.rs` (2/2, 100%, CI-enforced via the existing
`--features test-roms` step), and a tolerance-aware `SnapComparator`. Still
honestly deferred: a genuine externally-oracled golden CPU trace log for
`GoldenLogDiffer` and TIA-timing test-ROM fixtures for the Layer 3
`run_until_complete` runner (`T-0602-006`/`007`). RIOT timing, TIA
collision continuity, seeded power-on state, the full SingleStepTests
corpus, and Klaus's decimal test landed in v0.2.0. **`T-0601-008` is fixed
(v0.9.0)** — a rebuilt Gopher2600/Stella differential probe against Pitfall
II found that Rusty2600's RIOT timer never reverted its post-underflow
(divide-by-1) decrement rate back to the normal prescale after an `INTIM`
read (confirmed against Stella's `M6532::peek`/`updateEmulation`); the CPU
now correctly leaves the boot-time wait loop (previously stuck forever) —
see `docs/riot.md` and `CHANGELOG.md`. See `docs/STATUS.md` for the
authoritative per-suite/per-chip state.

## The phase line

- **Phase 0 — foundation:** workspace + crate skeletons compiling; CI green on
  stubs → `phase-0-foundation/overview.md`
- **Phase 1 — CPU golden log:** the 6507 to 0-diff against the Klaus / ProcessorTests
  golden log (documented + undocumented opcodes, decimal mode) →
  `phase-1-cpu-golden-log/overview.md`
- **Phase 2 — scheduler + video:** the color-clock lockstep scheduler + the TIA
  beam-racing renderer (RESPx, HMOVE comb, playfield, players/missiles/ball,
  collisions) producing a stable frame → `phase-2-scheduler-video/overview.md`
- **Phase 3 — audio:** the TIA two-channel poly-counter synthesis + the
  non-linear mixer → `phase-3-audio/overview.md`
- **Phase 4 — carts / mappers:** the cart model + board breadth, tier-gated under
  the honesty gate → `phase-4-carts-mappers/overview.md`
- **Phase 5 — frontend:** the egui shell + wasm + save-states / rewind / run-ahead
  → `phase-5-frontend/overview.md`
- **Phase 6 — accuracy to ~100:** drive the accuracy battery to target; defer hard
  residuals → `phase-6-accuracy-to-100/overview.md`
- **Phase 7 — breadth:** the BestEffort board families + region timing (NTSC/PAL/
  SECAM) as data → `phase-7-breadth/overview.md`
- **Phase 8 — reach:** netplay / RetroAchievements / TAS / Lua / shaders —
  additive, off-by-default → `phase-8-reach/overview.md`

## Release line

Iterative `v0.x.0` releases (each a real GitHub tag + release, `v0.x.y` for
post-`.0` fixes), extending the phases above with a debugger and
RetroAchievements pulled forward into the v1.0.0 gate, and cart breadth
pushed toward Stella-adjacent parity rather than stopping at the original
Core/Curated set:

| Version | Content |
|---|---|
| v0.1.1 / v0.1.2 | Truth-pass (docs/code reconciliation) + release-CI fixes |
| v0.2.0 "Cycle-Exact" | RIOT/TIA accuracy hardening, ADRs 0005/0006, full SingleStepTests + Klaus decimal in CI, CPU-crate cleanup |
| v0.3.0 "Curated" | Curated-tier cart schemes finished (CV/FA/Superchip/DPC/E7), all wired into `detect()` via Stella-ported hotspot heuristics (`T-0401-009`) |
| v0.4.0 "Breadth" | BestEffort cart breadth toward Stella-adjacent parity (staged patch train) — Batches 1-2 done (F0/E0/3F/3E/EF/DF/BF, 7 schemes) + `Board::snoop_write` |
| v0.4.1 | Continues the Batch 2 patch train — UA/0840 (2 more schemes) + `Board::snoop_read`; FE/SB/X07/4A50 and Batches 3-5 (DPC-family, ARM/peripheral, multicarts) target v0.4.2+ |
| v0.5.0 "Inspector" | Real `debug-hooks` debugger (6507/TIA/RIOT/memory panels, breakpoints/step/continue, `Bus::peek`/`peek_range`, a standalone disassembler); performance benches populated with real Criterion baselines |
| v0.6.0 "Catalog" | Closes 22 of the local 25-scheme catalogue (`docs/cart.md`): FE, SB, X07 land (`T-0402-006`/`011`, DONE) alongside the existing 19. 4A50 (`T-0402-014`), AR/Supercharger (`T-0402-015`), and the ARM-driven DPC+/CDF/CDFJ/CDFJ+ family (`T-0401-006`, needs a full ARM7TDMI Thumb interpreter) are substantially larger undertakings, deliberately deferred to a v0.6.x patch train rather than rushed |
| v0.7.0 "Cheevos" | RetroAchievements (`rusty2600-cheevos`, `T-0802-001..004`, DONE): vendors `rcheevos`, wires a safe `RaClient` into the frontend behind the off-by-default `retroachievements` feature — real per-frame achievement tracking, hardcore mode, a menu. A dedicated achievement-list/login/toast UI is deferred (`T-0802-005`) |
| v0.8.0 "Battery" | The accuracy battery stood up for real (`T-0602-001..005`, DONE): shared `Sentinel`/`run_cpu_until_sentinel` Layer 2 runner, a real `AccuracyScore`-gated `accuracy_battery.rs` (2/2, 100%, CI-enforced via the existing `test-roms` step — no new CI YAML needed), tolerance-aware `SnapComparator`. A genuine externally-oracled golden CPU trace log and TIA-timing test-ROM fixtures remain deferred (`T-0602-006`/`007`) |
| v0.9.0 "Hardening" | `T-0601-008` fixed (Pitfall II's boot-time RIOT-timer wait loop, found via a rebuilt Gopher2600/Stella differential probe): reading `INTIM` now reverts the post-underflow decrement rate to the normal prescale, confirmed against Stella's `M6532::peek`/`updateEmulation` (`docs/riot.md`). Commercial-ROM regression oracle expansion remains blocked by data availability (locally-supplied dumps only, none available); doc-sync pass done across `docs/architecture.md`/`compatibility.md` |
| **v1.0.0 "Foundation"** (current) | The first stable release. Every gate below is met: accuracy battery 2/2 (100%, ≥90% threshold), `debug-hooks` debugger + `retroachievements` both shipped, 22/25 cataloged cart schemes (Stella-adjacent breadth), green three-platform release matrix. No code changed from v0.9.0 — a version-line milestone plus a full doc/status reconciliation pass (`CHANGELOG.md`'s `[1.0.0]` entry). |

Explicit v1.0.0 non-requirements: netplay, TAS tooling, Lua scripting, HD
texture packs, shader stacks, mobile builds, and RA server-side allowlisting
(the integration working is the bar, not third-party approval) — all
Beyond-v1.0 (Phase 7 residual breadth / Phase 8 reach), plus the ADR 0002
fractional-timebase refactor **only if** a hard residual ever warrants it
(for the 2600, **likely never** — integer color-clock resolution is the
machine's native granularity).

## Version → Phase mapping (v1.1.0 → v2.0.0)

With v1.0.0 shipped, the project targets RustyNES-level (`../RustyNES`,
v1.9.9) documentation depth, feature breadth, performance rigor, and
accuracy methodology through an iterative `v1.x.0` line, culminating in a
Rusty2600 `v2.0.0`. Full context, rationale, and per-version technical
design lives in the plan this table summarizes; each `v1.x.0` gets `v1.x.y`
patch releases for anything found wrong after the fact, same convention as
the `v0.x.0` line.

| Version | Codename | Headline content |
|---|---|---|
| v1.1.0 "Persistence" | Save-states (`rusty2600-core::save_state`, ADR 0007) + a rewind rework reusing the same serialized format; three real frontend bugs fixed (gameplay/debugger flicker, window sizing, Settings persistence) |
| v1.2.0 "Foresight" | Run-ahead (`rusty2600-frontend::runahead`, off by default, `0..=4` frames), built on v1.1.0's snapshot/restore primitives; a `Tia::scanline` overflow panic fixed along the way |
| v1.3.0 "Scope" | Debugger depth: watch/conditional-breakpoint expression engine, callstack, a TIA per-scanline event/write-scatter viewer, a player/missile/ball position view — plus the long-deferred RetroAchievements achievement-list/login/toast UI (`T-0802-005`, DONE) |
| v1.4.0 "Signal" | A composable shader/filter stack (`rusty2600-gfx-shaders`: `CrtScanline` + an honestly-labeled `CompositeArtifact` approximation) + the data-model half of a right-sized sprite-replacement overlay (`sprite_pack`, `hd-pack` feature — live rendering splice deferred pending a TIA object-ID mask) |
| v1.5.0 "Full Catalog" | `Bank4A50` (`T-0402-014`, DONE): three independently relocatable ROM/RAM segments + a previous-access-gated hotspot state machine — closes 23 of 25 cataloged cart schemes. AR/Supercharger (`T-0402-015`) deliberately NOT attempted this release (its "fast-load" mode alone needs a bank-config decode, a 5-distinct-access delayed-write protocol, and a synthesized dummy BIOS stub — substantially larger than every other scheme here) |
| v1.6.0 → v1.6.x "Coprocessor" (patch train) | A new `rusty2600-thumb` crate: a real ARM7TDMI Thumb-1 interpreter (ported from Gopher2600's Go implementation, all 19 instruction-format classes, 27 conformance tests) for the DPC+/CDF/CDFJ/CDFJ+ family (`T-0401-006`). `v1.6.0` (DONE) lands the interpreter core only — no `Board`/`Cartridge` wiring yet (`docs/thumb.md`). `v1.6.1+` wires DPC+, then CDF, then CDFJ/CDFJ+ into `detect()` one family at a time — closing the catalogue to 24/25, leaving only AR/Supercharger (`T-0402-015`) as its own separately-scoped follow-up to reach 25/25 |
| v1.7.0 "Chronicle" | `.r26m` TAS movie format (`rusty2600-core::movie`, DONE): a start point (fresh seeded power-on or an embedded save-state blob — a branch point is exactly this) + a per-frame `MovieFrame` input log, built on v1.1.0's snapshot substrate. TAStudio-lite piano-roll panel (jump-to-frame via the existing rewind ring, branch points as separate `.r26m` files) + two riders (`access_counter`, `memory_compare_panel`). Live per-frame auto-recording into `EmuCore::run_frame` honestly deferred (`docs/movie.md`) |
| v1.8.0 "Oracle" | Accuracy battery depth, grown honestly rather than claiming an inflated pass rate. `T-0602-007` (DONE): a genuine externally-oracled golden CPU trace bundled for `GoldenLogDiffer` (20,000 instructions vs. an independent Gopher2600 CPU-package run, `first_divergence() == None`). `T-0602-006` (researched, stays a permanent stub): no freely-redistributable TIA/RIOT test-ROM corpus exists — confirmed again this release; TIA/RIOT accuracy work continues via the differential-oracle method against specific known-hard titles, not a canned corpus |
| v1.9.0 → v1.9.x "Scriptable" | Lua scripting (`rusty2600-script`: `mlua` native backend). `v1.9.0` (DONE) lands the engine only — a real, tested `emu` API + `ScriptBus` seam, `WritesLocked` determinism gate — not yet wired into `rusty2600-frontend` (`docs/scripting.md`). `v1.9.1+` wires it into the frontend (live `ScriptBus` impl, `scripting` feature flag, overlay compositing, `onFrame` hook) and, time permitting, adds the `piccolo` wasm fallback |
| v1.10.0 → v1.10.x "Rollback" | Rollback netplay (`rusty2600-netplay`), 2-player-only by deliberate scope cut vs. RustyNES's 2-4-player mesh. `v1.10.0` (DONE) lands the session crate only — `RollbackSession` wrapping `ggrs` over direct-IP/LAN UDP, a genuine rollback-desync test — not yet wired into `rusty2600-frontend` (`docs/netplay.md`). `v1.10.x` wires it into the frontend (host/join-game menu, live input capture), adds STUN/hole-punch NAT traversal, and adds the WebRTC browser transport |
| v1.11.0 → v1.11.x "Handheld" | Android build (`rusty2600-mobile` UniFFI bridge). `v1.11.0` (DONE): the bridge crate + a real Gradle/Kotlin app (`android/`), verified running on a real emulator (`Pixel_8_API_34`) — booted, installed, launched crash-free, a test ROM loaded via the real system file picker. No separate `rusty2600-android` glue crate needed (a design win, not a cut — see `docs/mobile.md`). Sideloadable, not store-submitted. `v1.11.x` (open): on-device save-state UI, paddle input, and real (non-emulator) hardware verification once physical test devices are available |
| v1.12.0 → v1.12.x "Pocket" | iOS build (`rusty2600-ios`/`ios/`, reusing the `rusty2600-mobile` bridge unchanged). `v1.12.0` (DONE): genuinely tool-generated Swift bindings (not hand-written) + real SwiftUI/Metal/AVFoundation app source, including `PaddleControlView` (a new touch-drag rotary-dial control, the first analog input either mobile host has solved). **This sandbox has no Xcode toolchain**, so — honestly documented, not glossed over — no Xcode build/Simulator/device run was possible; unverified by compilation. `v1.12.x` (open): on a real Mac, cross-compile the xcframework, create the Xcode project, and build+run on Simulator/device before this reaches the Android build's verification bar |
| **v2.0.0 "Parity"** (current) | Full doc/status reconciliation (DONE) confirming every gate above shipped, mirroring the `v1.0.0` ceremony. Release-matrix status: desktop ×3 + wasm/Pages + Android all green/verified; **iOS is the one cell not fully green** — source-complete, genuinely tool-generated bindings, but honestly unverified by compilation (no Xcode toolchain in this sandbox), carried forward as `v1.12.x` follow-up work rather than glossed over. Mobile store production launch stays explicitly out of scope (deferred beyond v2.0.0, matching RustyNES's own v2.1.0 precedent) |

**Explicit scope note**: unlike RustyNES's own literal "v2.0.0" (a
fractional master-clock timebase rewrite closing hard sub-scanline
residuals — ADR 0002 already considered and rejected that exact rewrite for
the 2600, "likely never needed"), Rusty2600's v2.0.0 is the RustyNES-parity
culmination milestone, not an accuracy-architecture rewrite. All four of
RustyNES's biggest-ticket features (Lua scripting, HD texture packs,
rollback netplay, mobile builds) landed — confirmed shipped, not just
planned. AR/Supercharger, the DPC+/CDF/CDFJ/CDFJ+ wiring, frontend wiring
for scripting/netplay, real TIA paddle timing, and full Xcode-verified iOS
build/run all continue beyond `v2.0.0` as their own separately-scoped
follow-up work, per the plan's own explicit gate criteria.

## Beyond v2.0.0

This roadmap's `v1.1.0 -> v2.0.0` arc is complete; `v2.1.0` "Follow-Through"
closed four of the carried-forward gaps (AR/Supercharger, real TIA paddle
timing, and frontend wiring for both `rusty2600-script`/`rusty2600-netplay`
— see `CHANGELOG.md`'s `[2.1.0]` entry). Open follow-up work remaining
(see each item's own release notes for full context): the DPC+/CDF/CDFJ/
CDFJ+ family (`T-0401-006`, substrate exists in `rusty2600-thumb` since
`v1.6.0`) to close the cart catalogue to 25/25; netplay's STUN/hole-punch
NAT traversal and WebRTC transport, plus per-player console
switches/paddles; scripting's overlay-compositing render step and the
`piccolo` wasm fallback; on-device save-state UI and physical-hardware
verification for the Android app; a real Xcode build, Simulator run, and
device verification for the iOS app on an actual Mac; a paddle test ROM to
cross-check the new RC-circuit simulation against real game behavior; and,
beyond either mobile store's submission (explicitly deferred past
`v2.0.0`), the HD-pack live rendering splice pending a TIA object-ID mask.
None of these need a new numbered plan yet — each remains a well-scoped,
independently shippable `v2.x.y`/`v2.x.0` release whenever picked up.

## How the phases map to the architecture

Phases 1–4 build the chips bottom-up along the one-directional crate graph
(`docs/architecture.md`): CPU first (no deps), then the scheduler + TIA video,
then TIA audio, then the cart boards. Phase 2 is where the lockstep substrate
(ADR 0001) and the determinism contract (ADR 0004) become load-bearing. The
honesty gate (ADR 0003) is live from Phase 0 and tightened every time a board
lands.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
