# Frontend — Rusty2600

References: `ref-docs/research-report.md` §13 (frontend dependencies), §9 (Javatari
as a portable/wasm reference); `docs/architecture.md` (fact 8); `docs/adr/0004`
(determinism). The `rusty2600-frontend` crate is owned by the frontend agent;
this doc describes the SHAPE it implements and the 2600-specific details.

## The always-on egui shell

`rusty2600-frontend` is `winit` + `wgpu` + `cpal` + `egui` — **not** a bare
window. egui runs **every frame**: a persistent menu bar (File / Emulation /
Tools / View / Debug / Help) + a status bar + a tabbed Settings window, with
toggleable CPU/TIA/RIOT/memory debugger panels layered on top.

The shell **never holds the emu lock inside the egui closure**: menu interactions
return a `MenuAction` that the app dispatches **after** the egui pass, and the
render branch copies the scanline buffer under a brief lock, drops it, then
presents. On native the emulator runs on a **dedicated thread** communicating via
an `Arc<Mutex<…>>` handle + lock-free shared input; the winit thread only does
UI + present. This is the only `std`/`unsafe`-carrying crate.

## 2600-specific display

- **Output geometry.** The TIA emits **160 visible pixels** per line over **192
  visible scanlines** (NTSC) — a roughly 160×192 active raster. Pixels are wide
  (the common presentation doubles them horizontally to ~320 for a ~4:3 look);
  the frontend owns the aspect correction. The beam-raced core hands up a composed
  scanline buffer, not a chip framebuffer (`docs/tia.md`).
- **Region palette.** The TIA's 7-bit colour value maps to RGB differently per
  region — **NTSC / PAL / SECAM** are distinct palettes (region data, not a build
  fork). The same value is yellowish on NTSC, gray on PAL, aqua on SECAM. See
  `docs/compatibility.md`.
- **Frame line budget.** NTSC 262 lines / PAL 312 lines drives the present cadence
  (≈60 Hz NTSC, ≈50 Hz PAL).

## Shader stack (`v1.4.0`, expanded `v2.10.0` "Prism")

`rusty2600-gfx-shaders` (`no_std`) carries the WGSL source for every
built-in post-process pass; `crate::shader_pass::ShaderStack` runs them.
Settings -> Video's "Shader stack" checkboxes build a `Vec<PassKind>`
(`crate::config::VideoConfig::shader_passes`) in click order; **empty is
the default**, and an empty stack skips `ShaderStack` entirely —
`Gfx::present` falls straight through to the unchanged direct nearest-blit,
so the byte-identical-default invariant holds by construction (the same
guarantee `[1.1.0]`'s `uv_scale` landed with).

### The five built-in passes

| Pass | What it is | Position constraint |
|---|---|---|
| `CompositeArtifact` | A horizontal chroma-bleed blur in already-decoded RGB space — a stylistic approximation, honestly labeled as such (not a real signal decode). | None — any position. |
| `CrtScanline` | Darkens every other output row. | None. |
| `NtscComposite` (`v2.10.0`) | A genuine YIQ-domain composite decode — see below. | **Must be first.** |
| `HqNx` (`v2.10.0`) | Edge-directed pixel-art smoothing (an independent WGSL adaptation of the published hqx technique). | None. |
| `Xbrz` (`v2.10.0`) | Edge-directed pixel-art smoothing with the xBR diagonal-dominance rule (an independent WGSL adaptation, characteristically rounder corners than `HqNx`). | None. |

### Arbitrary-length ping-pong (corrected from the original fixed 2-slot design)

Through `v2.9.0` `ShaderStack` was hard-limited to exactly 2 chained passes
— its own doc comment argued a third pass would need a third intermediate
texture. That turned out to be wrong: a linear chain of single-input/
single-output passes only ever needs TWO intermediate textures regardless
of length (pass `i` reads whichever texture pass `i-1` wrote and writes the
other one, or the swapchain target if it's the last pass). `v2.10.0`
generalized `ShaderStack::render` to loop over any number of passes with
the same two textures the 2-pass design already allocated — see
`shader_pass.rs`'s module doc for the exact alternation rule.

### The `NtscComposite` pass: a real YIQ decode, not a bigger blur

The TIA's colour value is 4 bits of hue + 3 bits of luma (`docs/tia.md`),
looked up through a baked, region-specific RGB table
(`crate::palette`) — there is no live signal math on the CPU side. The
raw palette-index byte (`colour7 = colu >> 1`) is threaded through
alongside the RGBA framebuffer, purely additively:
`rusty2600_core`/every chip crate is completely untouched; only
`rusty2600-frontend`'s OWN presentation buffers gained a parallel `index`
field (`present_buffer::Frame::index`, `EmuCore::index_buffer`), populated
at the exact same call site the existing RGB conversion already runs, and
a small `R8Uint` texture (`ShaderStack`'s `index_texture`, `Gfx::upload_index`)
that nothing samples unless `NtscComposite` is actually in the active
stack.

**The technique** (adapted from the Bisqwit/Mesen NES composite decoder,
NOT ported — the TIA's colour generation is architecturally different from
the NES PPU's; see `rusty2600_gfx_shaders::NTSC_COMPOSITE_WGSL`'s own doc
comment for the full derivation): the TIA's dot clock runs at exactly the
NTSC colour-subcarrier frequency, so — unlike the NES — there's no
progressive per-dot phase drift to synthesize; a given `(hue, luma)` value
decodes, in isolation, to one fixed YIQ point, which is exactly what the
existing measured NTSC RGB table already captures. The genuinely
TIA-specific work is modelling a real NTSC decoder's **chroma/luma
bandwidth differential** (chroma ~0.6-1.5 MHz vs. luma ~4.2 MHz in a
broadcast decoder) — the actual physical origin of 2600 "artifact colors."
The pass converts a 5-tap horizontal neighbourhood's raw `(hue, luma)`
entries to YIQ, passes the CENTRE tap's Y through unblended (sharp luma),
takes a weighted 5-tap average of I/Q (blurred chroma), and converts back
to RGB.

**Verification**: the RGB<->YIQ matrix pair is a true inverse (up to f32
rounding), so a uniform neighbourhood (no colour transition) decodes back
to the exact same RGB a direct palette lookup would give — verified in
pure Rust (`shader_pass.rs`'s `ntsc_yiq_round_trip_matches_palette_for_every_entry`
test) against every one of the 128 real NTSC palette entries, independent
of WGSL/naga/wgpu execution. A second test
(`ntsc_composite_wgsl_table_matches_frontend_palette`) keeps the WGSL's
baked copy of the palette table honest against `palette::NTSC`'s own
values. **Honesty**: this is NOT a transistor-level composite-signal
simulation (no colorburst timing, no sub-pixel oversampling) and it is
NTSC-only — PAL's subcarrier uses a different phase-alternating modulation
this pass doesn't model, and SECAM has no chroma at all, so the Settings
checkbox disables itself outside the NTSC region. It IS a genuine
signal-domain (YIQ, not RGB) decode modelling a real, named NTSC
phenomenon the pre-existing `CompositeArtifact` RGB blur does not attempt.

Because `NtscComposite` samples the raw index texture (not the RGBA
ping-pong texture), it can only usefully be the stack's first pass — the
Settings UI enforces this by always re-sorting it to position 0 when
enabled (`shell.rs`); `ShaderStack::render` also defensively skips it if it
somehow ends up elsewhere (a wgpu bind-group-layout mismatch would
otherwise panic).

### Constrained `.slangp`/`.cgp` preset importer (`v2.10.0`)

Settings -> Video -> "Import shader preset..." (native-only — needs
`rfd`'s file dialog) opens a `.slangp`/`.cgp` RetroArch shader preset and
maps its `shaderN = path` entries onto this project's five built-in passes
by recognizing well-known filename stems (`crate::slang_preset`) —
matching RustyNES's own ADR 0013 design philosophy: this is **not** a
GLSL/Slang -> WGSL transpiler and never will be. An unrecognized filename
is reported to the user as unsupported, never silently dropped; a pure
pass-through stem (`stock`/`passthrough`/`pixellate`) is silently skipped
(it isn't a missing capability). An `ntsc`/`composite` stem always maps to
the position-flexible `CompositeArtifact`, never the position-constrained
`NtscComposite`, so an imported preset can never produce an invalid stack
regardless of where the preset places that shader.

## The debugger (`debug-hooks`, v0.5.0)

Real as of v0.5.0 — `crates/rusty2600-frontend/src/debugger/` (default-on
feature, same precedent as `emu-thread`: turn it off with `--no-default-
features --features wasm-winit,help-tui,emu-thread` for a debugger-free
build). Structure mirrors the shell's non-negotiable rule: nothing under
`debugger/` ever touches the emu lock. `app.rs` builds a `DebugSnapshot`
(registers, TIA/RIOT state, disassembly, a memory-window byte slice) once
per frame **only while the overlay is open and a ROM is loaded**, under the
same brief lock the present path already takes; the panel-render functions
in `debugger/mod.rs` are pure functions over that snapshot.

**Wasm32 (`[v2.9.0]`)**: `debug-hooks` is wasm32-safe for the `wasm-winit`
build (`--no-default-features --features wasm-winit,debug-hooks`) — every
panel below works identically in-browser, toggled by the same
`Debug -> Debugger overlay` checkbox, EXCEPT TAStudio's "Save branch"
button (needs `rfd`'s native-only save-file dialog; hidden on wasm32 in
favor of a "native-only" label). See the wasm section below for the full
wasm32 story.

- **6507 panel** — A/X/Y/S/PC/P register grid, Step (`step_instruction()`
  once) and Continue (run to a breakpoint or a 1,000,000-instruction safety
  cap) buttons, a breakpoint add/remove list, and a scrolling disassembly
  window starting at PC (`>` marks the current PC, `*` marks a breakpoint).
- **TIA panel** — beam position (scanline/color clock), P0/P1/M0/M1/BL
  positions + colors, the playfield/background colors, and the 15 pairwise
  collision latches.
- **RIOT panel** — `INTIM` + its prescale divisor, `SWCHA`/`SWCHB` pin state
  + DDRs.
- **Memory panel** — a 256-byte hex+ASCII viewer with quick-jump buttons for
  RIOT RAM (`$0080`) and the cart window (`$1000`), backed by
  `Bus::peek_range` (see below).
- **Disassembler** (`debugger/disasm.rs`) — a standalone, display-only 6502
  mnemonic table, independent of the CPU crate's private opcode dispatch.
  Undocumented opcodes render as `.byte $xx` rather than inventing a
  mnemonic.

**Side-effect-free reads.** A real `Bus::cpu_read` can trigger bankswitch
hotspots, RIOT's INTIM read-clears-underflow behavior, and cart
`snoop_read` side effects — none of which a debugger peek should ever
cause. `Bus::peek`/`Bus::peek_range` (`rusty2600-core::bus`) solve this by
reading from a full clone of the bus rather than the live one; `peek_range`
clones once and reads every requested byte from that single clone (not once
per byte), which is the difference between one clone per frame and one
clone per displayed byte for a 256-byte memory panel or a multi-instruction
disassembly window.

## RetroAchievements (`retroachievements`, v0.7.0)

`rusty2600-cheevos` vendors the RetroAchievements `rcheevos` C library (MIT)
and wraps it in a safe `RaClient`, native-only and off by default. Owned by
`crate::cheevos::CheevosState` on the winit/main thread — deliberately NOT
inside `EmuCore`: `RaClient` is `!Send`/`!Sync`, incompatible with
`EmuCore`'s `Send` requirement (needed by the default-on `emu-thread`
feature). Pumped once per frame under the same brief emu lock the present
path and the debug snapshot already take, peeking the bus via
`|addr| bus.peek(addr)` rather than holding the lock any longer.

- **Memory map.** The 2600's only mutable game-state RAM is the RIOT's 128
  bytes — RA's flat address space maps directly onto it
  (`rusty2600_cheevos::memory::ra_addr_to_riot`: RA `0x00..=0x7F` -> CPU bus
  `$0080..=$00FF`), far simpler than a typical console's split RAM/WRAM map.
- **Game identification.** ROM bytes hash via rcheevos' own generic
  whole-buffer MD5 path (`RC_CONSOLE_ATARI_2600`; no console-specific hash
  case exists for the 2600 in the vendored source, since it needs none of
  the header/region disambiguation some consoles do).
- **What's wired today:** client construction, ROM load/close ->
  `begin_load_game`/`unload_game`, per-frame `do_frame`/`idle` pumping,
  hardcore mode (Emulation -> RetroAchievements menu), and achievement-
  unlock/server events surfaced as status-bar text.
- **What's deferred:** a dedicated achievement-list panel, a login dialog,
  and a rich-presence/unlock-toast HUD — the backend fires real events
  today; these are the dedicated UI surfaces for them
  (`to-dos/phase-8-reach/sprint-2-ra-and-tas.md`, `T-0802-005`).

## 2600-specific input

- **Joystick** — the standard CX40: 4 directions + 1 fire, fed into RIOT `SWCHA`
  (P0/P1) and the TIA `INPT4`/`INPT5` trigger latches.
- **Paddles** — the CX30 analog paddles, read via the TIA `INPT0..3` dump-capacitor
  inputs (a per-paddle charge timer); two paddles per port.
- **Console switches** — Reset, Select, Color/B&W (`TV TYPE`), and the two
  difficulty switches, all on RIOT `SWCHB`. The frontend surfaces these as UI
  toggles (a real-panel affordance), since many games read them at boot.

### Keyboard controller / Trak-Ball — researched, not modeled (`v2.5.0`)

A RustyNES gap-analysis question: are the Atari Keyboard (Keypad) Controller
and the CX-22/CX-80 Trak-Ball worth modeling? Checked against Stella's own
implementation and properties database (`ref-proj/stella/`) rather than
relying on memory:

- **Keyboard/Keypad Controller** (`Controller::Type::Keyboard`,
  `ref-proj/stella/src/emucore/Keyboard.hxx`) — a 12-key numeric keypad (0-9,
  `*`, `#`), wired through the same `AnalogReadout` machinery this crate's
  own `AnalogPaddle` (`v2.1.0`) already ports. Stella's `stella.pro`
  properties database tags **40 ROM entries** with this controller,
  including real official Atari releases (*Brain Games*, 1978) alongside
  prototypes and homebrew (*Big Bird's Egg Catch*, *Oscar's Trash Race*) —
  a genuinely real, if minor, peripheral.
- **Trak-Ball** (`Controller::Type::TrakBall`, a `PointingDevice` subclass
  using 2-bit quadrature-encoded relative motion, not an absolute-position
  or RC-circuit signal) — Stella's properties database tags only **4
  entries (2 distinct ROMs)**, both homebrew hacks (*Missile Command
  (Trakball)* by Thomas Jentzsch, 2002) — **no official Atari 2600
  cartridge ever shipped with Trak-Ball support**; the CX-22/CX-80
  Trak-Ball's real historical home was the 5200/8-bit computer line and
  arcade cabinets, not the VCS itself.

**Decision: neither is modeled in this arc.** The Trak-Ball has no
official-release justification at all. The Keyboard controller has some
real (if minor) historical footing, but at 40 ROMs out of a catalogue of
several thousand known 2600 titles it remains a genuinely niche peripheral,
and — unlike the paddle, which nearly every 2600 owner had and which
directly gated real gameplay accuracy for a meaningful fraction of the
library (`v2.1.0`) — no title requiring it is load-bearing for this
project's own accuracy or compatibility goals. Revisit only if a specific,
concrete user need for a Keyboard-controller title surfaces; this is not a
permanent "never," just a deliberately deprioritized "not yet."

## Save-states, rewind, run-ahead

All snapshot/restore and rate control live **in the frontend**, never in the core
synthesis (ADR 0004). Because the core is deterministic (seeded power-on phase,
no OS RNG), a snapshot is a full serialization of `System` state and restores
bit-identically — the basis for rewind, run-ahead, and (later) netplay rollback.

### Manual save-state slots (`v2.4.0`)

`File -> Save State` / `Load State` expose 8 numbered slots per ROM — the
ordinary "save my game" menu feature, built entirely on top of the rewind
ring's already-real `SaveState` format (`rusty2600-core`, ADR 0007); no new
wire format was needed.

- **ROM identity**: `EmuCore::load_rom` computes a 64-bit FNV-1a hash over
  the raw ROM bytes (`rom_tag`), stored for the session's lifetime. This is
  the same opaque `rom_tag: u64` `SaveState::capture`/`restore` already
  required — the frontend just had no caller supplying one until now.
- **On-disk layout**: `<platform-data-dir>/Rusty2600/saves/<rom_tag as
  16-digit lowercase hex>/slot_<N>.r26s` (`crate::config::save_slot_dir`/
  `save_slot_path`). Keying by `rom_tag` means two different ROMs' slots can
  never collide, and one game's whole save history is a single deletable/
  relocatable directory. `.r26s` matches the project's existing `.r26m`
  TAS-movie extension convention. Native-only — the wasm build has no
  filesystem save path yet. Settings persistence (a plain string) landed via
  `localStorage` in `v2.8.0` (see the `wasm` section below); save-state SLOT
  persistence is deliberately deferred past `v2.8.0` — it needs per-slot
  `localStorage` keys, an existence/timestamp substitute for
  `SaveSlotInfo::probe`'s filesystem `stat` calls (`localStorage` has no
  mtime), and real size budgeting (`localStorage` is typically ~5-10 MB per
  origin; each slot is small on its own, but 8 slots is a real multiple), a
  distinctly bigger lift than the single-key Settings value above. Landing
  it well needs its own release rather than being squeezed in alongside
  `v2.8.0`'s touch-control/Settings-usability headline.
- **Menu status**: each slot's existence + last-modified timestamp is
  probed fresh every frame via plain filesystem `stat` calls
  (`SaveSlotInfo::probe`), deliberately AFTER the brief emu lock is
  dropped — the probe only needs the `rom_tag` copied out under the lock,
  never the emulator itself, so the File menu never adds emu-lock
  contention to show real per-slot info.
- **Load safety**: `SaveState::restore`'s existing `rom_tag` mismatch check
  means a slot file can never be silently loaded against the wrong
  cartridge; a missing, corrupt, or mismatched slot file surfaces a clear
  status-bar message instead of panicking or silently doing nothing.

## wasm

Two independent wasm32 entry points, selected by feature flag (`src/wasm.rs`;
see its module doc and `Cargo.toml`'s `[features]` doc comment):

- **`wasm-winit`** (landed v2.5.0, NOT yet the deployed build — see
  `wasm-canvas` below for what `web/index.html` actually builds today) —
  the real `app::App`, the SAME winit+wgpu+egui shell the native build
  uses, compiled for `wasm32-unknown-unknown`. `emu-thread`/`debug-hooks`/
  `retroachievements`/`scripting`/`netplay` are NOT wasm-safe and must
  stay off (see `Cargo.toml`'s feature doc comments). Available to build
  via `data-cargo-no-default-features` + `data-cargo-features="wasm-winit"`
  once real-browser rendering is confirmed (see the honest-status note
  below for why that switch hasn't happened yet). ROM loading routes
  through a hidden `<input type=file>`
  (`App::trigger_wasm_rom_picker`) rather than `rfd` (native-only, not even
  a wasm32 dependency).

  **`v2.8.0` "Touchpoint" additions** (web parity wave 1 — all shared code
  with the native build, verified via `cargo check`/`cargo clippy --target
  wasm32-unknown-unknown` + native unit tests; NOT live-browser-verified for
  the same reason rendering itself isn't, see the honest-status note below):
  - **On-screen touch controls**: a D-pad + fire button + a console-switch
    row (Select/Reset/Color/Difficulty), all plain egui `Button` widgets
    (`ShellState::render_touch_overlay`, wasm32-only), feeding the exact same
    `crate::input::InputState` the keyboard path populates via the new
    `crate::input::TouchButton`/`TouchOverlayState` (pure press/hold/release +
    latch-edge model, target-agnostic and unit-tested under the ordinary
    native `cargo test --workspace` run). Defaults to visible (`View ->
    Touch overlay` toggles it) since a touch-only device has no other way to
    drive the emulator. KNOWN LIMITATION: egui-winit's default touch handling
    tracks one synthesized pointer at a time, so simultaneous multi-finger
    combos (holding a direction while also tapping Fire) are not guaranteed
    to register both — a real egui/egui-winit constraint, unverifiable
    either way in this project's sandbox (no real touchscreen), not
    something this release introduces or works around (see
    `render_touch_overlay`'s doc comment for the full reasoning).
  - **Console-switch buttons**: already available "for free" via the shared
    `Emulation -> Console switches` menu (native and wasm32 both dispatch
    `MenuAction::ConsoleSwitch`) — no gap needed closing there. The NEW work
    is the touch-friendly on-screen row above, since a touch user can't
    easily reach a dropdown menu the way a mouse user can.
  - **Settings panel**: reviewed tab-by-tab for wasm32-safety — the Video/
    Audio/Input/System tabs are plain egui widgets mutating `Config` in
    place with no native-only API in the path (no `rfd` dialogs, no blocking
    file I/O), so the panel already worked correctly on wasm32 structurally;
    this release's real work was making `MenuAction::SaveConfig`'s
    `Config::save()` call actually persist there (see below) instead of
    silently discarding every change on tab close.
  - **`Config::save()`/`Config::load()`**: real `localStorage` persistence
    (`LOCAL_STORAGE_KEY = "rusty2600.config"`), replacing the previous
    no-op `save()` stub and previously-nonexistent `load()` on this target.
    Shares the exact same TOML (de)serialization helpers
    (`Config::to_toml_string`/`from_toml_str`) the native `config.toml` file
    path uses, so both backends are covered by one set of native unit tests.
    Falls back to defaults on any missing/corrupt/foreign stored value,
    never blocking launch. Save-STATE-SLOT persistence (`v2.4.0`'s manual
    save slots) is deliberately deferred — see the save-states section
    above for why.

  **`v2.9.0` "Full Circle" additions** (web parity wave 2 — again all shared code with the
  native build, verified via `cargo check`/`cargo clippy --target wasm32-unknown-unknown` +
  native unit tests; NOT live-browser-verified for the same reason rendering itself isn't, see
  the honest-status note below):
  - **`?settings=` share-link** (`crate::share_link`) — encodes the CURRENT `Config` (region,
    video, audio, both players' key bindings) to a compact URL-safe base64 blob and reads it back
    from `window.location().search()` at boot, overriding the `localStorage`-persisted config so
    opening a shared link always reflects the sender's settings. Unlike RustyNES's own
    `ShareSettings` (a curated subset — that project's config also carries machine-local paths
    and a login token), Rusty2600's `Config` has no such fields, so the WHOLE config round-trips
    through the exact same `Config::to_toml_string`/`from_toml_str` helpers `[v2.8.0]`'s
    `localStorage` persistence already uses (now `pub(crate)`), rather than a parallel DTO.
    Scope is settings ONLY — there is no canonical URL for a loaded ROM on this build (both the
    native `rfd` dialog and the wasm `<input type=file>` picker read a user-local file with no
    URL), so a share link never references "this exact ROM". The base64url codec itself is
    hand-rolled pure Rust (no `web_sys`/`atob`/`btoa`, no new dependency), which is what makes the
    encode/decode round-trip real, native-tested coverage rather than only a wasm32-only claim.
    Settings -> System gained a "Generate share link" button (wasm32-only) that displays the
    current URL in a selectable text field to copy.
  - **Wasm debugger overlay** — `debug-hooks` is now confirmed wasm32-safe for `wasm-winit`
    (`cargo check`/`clippy --target wasm32-unknown-unknown --no-default-features --features
    wasm-winit,debug-hooks`, zero warnings), exposing the CPU/TIA/RIOT/Memory panels (plus
    Watch/Callstack/Events/P-M-B/Access-counter/Compare) in-browser, toggled the same
    `Debug -> Debugger overlay` checkbox the native build uses (shared `shell.rs` code, no
    separate wasm path needed). The one exception: `TastudioSaveBranch` (TAStudio's "Save branch"
    button) needs `rfd`'s native-only save-file dialog, so that ONE action/variant/button is
    `not(target_arch = "wasm32")`-gated specifically (`MenuAction::TastudioSaveBranch`,
    `debugger::tastudio_panel::render_tastudio_panel`'s wasm32 branch shows a "native-only" label
    instead of a dead button) — every other debugger panel, including TAStudio's own
    jump-to-frame, is fully functional on both targets. This was the sole actual wasm32-safety gap
    in `debug-hooks`; nothing else in `crates/rusty2600-frontend/src/debugger/` touches a
    native-only API.
  - **PWA install** (`web/manifest.json` + `web/sw.js` + placeholder icons) — makes whichever
    build is actually deployed here installable ("Add to Home Screen" / a desktop browser's
    install prompt) and usable offline after a first visit. `sw.js` runtime-caches same-origin
    GETs (cache-first, background-revalidate) rather than a fixed precache list, since Trunk
    hashes the `.wasm`/`.js` glue filenames per build; navigation requests are cache-keyed by
    pathname only (query stripped) so a `?settings=` share link still hits the cached shell
    instead of missing it and failing offline. Measured bundle sizes (this sandbox, `trunk build
    --release`): the ACTUALLY DEPLOYED `wasm-canvas` build is tiny (~324 KiB total: ~249 KiB wasm
    + ~24 KiB JS glue + HTML/manifest/icons/SW) — comfortably under any reasonable budget. The
    NOT-yet-deployed `wasm-winit` build is meaningfully larger — ~7.1 MiB wasm (no `wasm-opt`
    pass configured in `web/Trunk.toml` yet) + ~137 KiB JS glue — because `winit`+`wgpu`+`egui`
    dominate wasm bundle size regardless of how simple the 2600 core itself is; a follow-up
    release that actually deploys `wasm-winit` should add a `wasm-opt` pass (`[tools] wasm_opt =
    "…"` in `Trunk.toml`) to shrink this before making it the live build. RustyNES's own cited
    "~5 MiB budget" precedent does not directly transfer for the same reason: frontend-stack
    weight, not console complexity, dominates. Icons are a placeholder (`web/icon.svg`, a simple
    joystick glyph in the page's own dark/amber palette, rasterized via `rsvg-convert` to 192px/
    512px PNGs) — this project has no dedicated logo/brand asset yet.

  **`v2.10.0` "Prism" shader-stack expansion** — verified via `cargo check`/
  `cargo clippy --target wasm32-unknown-unknown --no-default-features
  --features wasm-winit` (and again with `,debug-hooks`), zero warnings, same
  posture as every other wasm-winit addition above. This release's new
  `ShaderStack::upload_index`/`NtscComposite` path adds an `R8Uint`
  (`texture_2d<u32>`, `textureLoad`-only) texture binding, which is a NEWER
  and less commonly exercised wgpu/WebGL2 code path than the plain
  `Rgba8UnormSrgb` + filtering-sampler bindings every other pass uses —
  type-checks and validates via naga on this target, but (like the rest of
  `wasm-winit`'s rendering story, see the honest-status note below) has NOT
  been confirmed to actually render correctly against a real browser's
  WebGL2/WebGPU backend in this sandbox. `HqNx`/`Xbrz` reuse the exact same
  bind-group shape `CompositeArtifact`/`CrtScanline` already had, so they
  carry no additional wasm32 risk beyond what was already unverified before
  this release. The `.slangp`/`.cgp` preset importer (`crate::slang_preset`)
  is pure parsing logic with no GPU or file-picker dependency of its own, but
  its own UI entry point (Settings -> Video -> "Import shader preset...")
  is native-only (`rfd`), matching the save-state-slot actions' own gate.

  **Honest status (v2.5.0 landing)**: compiles cleanly (`cargo check
  --target wasm32-unknown-unknown --no-default-features --features
  wasm-winit`, zero warnings) and a real `trunk build` produces a working
  `dist/` bundle. Loaded in a real browser (headless Chromium), `run_winit()`
  executes and logs correctly — but the `Gfx::new_async` adapter-request step
  could NOT be verified rendering an actual frame in this sandbox: the
  available headless-Chromium-with-SwiftShader environment has no
  `navigator.gpu` (no real WebGPU) and, in this specific setup, wgpu 29's
  adapter request also failed to fall back to the compiled-in GL/WebGL2
  backend (`wgpu-hal`'s `gles` feature, confirmed present via `cargo tree -e
  features`) — see `Cargo.toml`'s wasm32 `wgpu` dependency comment for the
  full investigation (a likely wgpu-internal `webgpu`-vs-`wgpu_core`
  build-script exclusivity for this target, not something fixable from this
  project's own `Cargo.toml` alone, since `egui-wgpu`'s own unconditional
  `wgpu` dependency reintroduces the `webgpu` feature via Cargo's per-crate
  feature unification regardless of this crate's own `default-features =
  false`). **Rendering + keyboard input are therefore UNVERIFIED end-to-end**
  — real desktop browsers with genuine GPU access (real WebGPU, or a real
  hardware-accelerated GL driver instead of SwiftShader) may well work where
  this sandboxed headless environment could not; that is a real, open
  question for the next release/session with browser access to confirm, not
  a claimed-working capability. Audio is an explicitly deferred stretch goal
  (the native `cpal`-based `audio.rs`/`AudioProducer` stays native-only;
  wasm-winit audio would need to reuse `wasm-canvas`'s proven Web Audio
  `AudioSink` pattern instead, not attempted yet).

- **`wasm-canvas`** (the older, simpler fallback — **this is what
  `web/index.html` actually builds and what the live GH-Pages demo
  deploys, as of `v2.5.0`**) — a bare canvas-2D `requestAnimationFrame`
  bootstrap with real, proven-working keyboard input, ROM loading
  (including `.zip` archives), and Web Audio output. This is genuinely
  complete and live-verified (it was the sole demo through `v2.4.0`, and
  stays the deployed one through `v2.5.0` too, pending `wasm-winit`'s
  real-browser rendering confirmation above).

Both build for `wasm32-unknown-unknown`. The Trunk project lives at
`crates/rusty2600-frontend/web/` (not a repo-root `web/`) — `trunk build
--release` from that directory produces `dist/`, which `.github/workflows/
pages.yml` deploys to GitHub Pages (demo at `/`, rustdoc at `/api/`) on every
push to `main`. `web/Trunk.toml` pins `public_url = "/Rusty2600/"` to match
where Pages actually serves this repo, and pins the `wasm_bindgen` CLI
version to match Cargo.lock's resolved library version (a mismatch fails the
build). The core's `no_std` + `alloc` chip stack cross-compiles independent
of any of this. `web/manifest.json` + `web/sw.js` + `web/icon-192.png`/
`icon-512.png` (`[v2.9.0]`, see above) are copied into `dist/` by
`data-trunk rel="copy-file"` entries in `index.html` regardless of which
wasm feature is selected — the PWA install/offline layer applies to
whichever build is actually deployed.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
