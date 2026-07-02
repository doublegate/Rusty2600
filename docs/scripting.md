# Lua scripting (`rusty2600-script`) — Rusty2600

References: `to-dos/ROADMAP.md` (v1.9.0 "Scriptable"); `docs/adr/0004`
(the determinism contract); `crates/rusty2600-script/src/`. This doc is
the SPEC, not history — update it in the same PR as the code.

## What this crate is

`rusty2600-script` adds Lua scripting to Rusty2600, off by default. It is
a `std`-only, `unsafe`-permitted crate (the one exception to the chip
crates' `no_std + #![forbid(unsafe_code)]` house style — `mlua`'s C FFI
requires it, matching how `rusty2600-cheevos` already vendors `rcheevos`
for the same reason). It never touches `rusty2600-core`/
`rusty2600-frontend` types directly: a **host** implements the
[`ScriptBus`] trait over whatever real emulator state it owns, and this
crate stays completely host-agnostic.

**v1.9.0 lands the engine only** — a complete, tested `emu` Lua API and
the `ScriptBus` seam it's built on. **It is not yet wired into
`rusty2600-frontend`**: there is no `scripting` feature flag on the
frontend, no `ScriptBus` implementation over the live `Bus`/`Cpu`, no
overlay-compositing step in the render pipeline, and no live `onFrame`
hook tied to `EmuCore::run_frame`. This is the same honest-partial-landing
call this project made for the ARM interpreter (`rusty2600-thumb`, landed
in `[1.6.0]` with cart-board wiring deferred to the `v1.6.x` patch train)
and for `.r26m` movies (`[1.7.0]`, format landed with live recording
deferred) — a real, substantial engine now, with the equally substantial
live-integration pass as its own explicitly-scoped follow-up, not rushed
or silently skipped.

## Why Lua, why `mlua`, why not `piccolo` yet

Per the project's own scope-cut precedent (2-player-only netplay, a
smaller `emu` API than RustyNES's own PPU/APU-heavy table), the plan
called for `mlua` as the native backend plus a pure-Rust `piccolo`-backed
wasm fallback. **Only `mlua` is implemented in v1.9.0.** `piccolo` is a
materially less mature project than `mlua` (fewer standard-library
facilities, less battle-tested) — landing a second, non-byte-parity
scripting backend alongside a brand-new API surface in the same pass
risked shipping either half-tested. The `emu` table and `ScriptBus` seam
are designed backend-agnostically (nothing outside `engine.rs` depends on
`mlua`-specific types), so a `piccolo` backend remains a genuine, scoped
follow-up rather than a rewrite.

### `v2.9.0` status: `piccolo` investigated for real, still deferred — not a rewrite candidate yet

`v2.9.0` "Full Circle" was scoped to finally land the `piccolo` wasm
fallback flagged above. This pass actually did the investigation (docs.rs,
the upstream README, the upstream `stdlib/` source tree, the `Executor`
API), rather than re-deferring on the same reasoning as before. Two
independent findings came out of it:

- **`mlua` on `wasm32` is confirmed, again, to be a hard wall.**
  `cargo check --target wasm32-unknown-unknown -p rusty2600-script` fails
  inside `lua-src`'s build script ("don't know how to build Lua for
  wasm32-unknown-unknown") — the vendored C build genuinely cannot target
  that toolchain. This is not a bug to fix; native's `mlua` backend is
  unchanged.
- **`piccolo` is not yet a workable substrate either**, for two
  compounding reasons:
  1. *Stdlib gap.* Its only crates.io-published release (`0.3.3`, June
     2024) implements almost none of Lua's `string`/`table` libraries —
     per its own README, those (plus `io`/`file`/`os`/`package`/`utf8`)
     are "either missing or very sparsely implemented." That rules out
     exactly the kind of script this crate's own `emu` API invites —
     `string.format` for an `emu.drawText` debug label, `table.insert` for
     per-frame state tracking. Upstream `master` has grown partial
     `string.rs`/`table.rs` implementations since (active 2025 commit
     history), but nothing past `0.3.3` is on crates.io, and this
     workspace has no git dependency anywhere else today — pinning one
     here would be a first, against a crate whose own README warns
     "expect *frequent* pre-1.0 API breakage, this crate is still very
     experimental."
  2. *Architecture mismatch.* `piccolo` is a `gc-arena`-based "stackless"
     VM: an `Executor<'gc>` is driven by repeated
     `Executor::step(ctx, fuel) -> bool` calls inside an arena
     `mutate`/`finish` scope, and is itself neither `Send` nor storable
     outside that arena's lifetime. `engine.rs`'s current design (a plain
     owned `Lua` VM, `Rc<RefCell<B>>`-captured host closures, an
     `mlua::RegistryKey` held across frames for `onFrame`) has no
     equivalent "keep the VM around, call into it once per real frame"
     pattern in `piccolo` without first building real `gc-arena`/
     `Rootable!` plumbing of its own — a genuine rewrite of the engine's
     embedding shape, not a second `engine_piccolo.rs` sibling reusing
     `engine.rs`'s shape.

**Decision: defer again, on a narrower and more concrete basis than
before.** This is a confirmed external blocker, not a scope-discipline
judgment call: the only realistic fallback candidate's published version
cannot run realistic scripts, and its unpublished branch is explicitly
pre-1.0-unstable by its own maintainer's admission. Forcing an
implementation against either horn of that trade-off (a stdlib-crippled
engine, or a git-pinned dependency on volatile code) would be a worse
outcome than shipping nothing this release. In-browser Lua scripting
remains unsupported on `wasm32`; nothing about the native `mlua` path
changed. Revisit once `piccolo` publishes a crates.io release with real
`string`/`table` coverage — an upstream milestone, not one this project
controls or can schedule against.

## Architecture (matches the crate)

- **`bus.rs`** — `ScriptBus`, the host seam. `CpuSnapshot` (a read-only
  6507 register-file view for `emu.cpu()`) and `JoyDirection` (the five
  values `emu.setJoystick` accepts: `up`/`down`/`left`/`right`/`fire`) are
  the plain data types the trait's methods use.
- **`lock.rs`** — `WritesLocked`, the determinism gate every script-driven
  WRITE (`emu.poke`, `emu.setJoystick`, `emu.setConsoleSwitch`) is checked
  against. Folds exactly one real source today: `ra_hardcore`
  (RetroAchievements hardcore mode). Deliberately does NOT carry
  `movie_locked`/`netplay_locked` stub fields — those subsystems (`.r26m`
  movies, rollback netplay) don't have their own lock concept yet, and a
  fake always-`false` field would be dead weight pretending to be a
  feature. The right time to add them is the same change that gives those
  subsystems a real lock to fold in.
- **`overlay.rs`** — `Overlay`/`TextPrimitive`/`RectPrimitive`/
  `PixelPrimitive`: accumulates `emu.drawText`/`drawRect`/`drawPixel`
  calls into a per-frame buffer a host can consume. `[2.3.0]` wires this
  into the frontend's render pipeline — see "Overlay compositing" below.
- **`engine.rs`** — `ScriptEngine<B: ScriptBus>`: owns the Lua VM and
  installs the full `emu` table via `Rc<RefCell<_>>`-shared closures over
  the host's `ScriptBus` implementation.

## The `emu` API

| Function | Gated? | Backs onto |
|---|---|---|
| `emu.peek(addr)` | No — always allowed | `ScriptBus::peek`, mirrors `rusty2600_core::Bus::peek` |
| `emu.poke(addr, val)` | Yes (`WritesLocked`) | `ScriptBus::poke`, mirrors `Bus::cpu_write` |
| `emu.cpu()` | No | `ScriptBus::cpu` → `CpuSnapshot` |
| `emu.onFrame(fn)` | No | registers a callback the host invokes once per frame |
| `emu.setJoystick(port, direction, pressed)` | Yes | `ScriptBus::set_joystick` |
| `emu.setConsoleSwitch(name, value)` | Yes | `ScriptBus::set_console_switch` (unrecognized `name` → Lua error, not a silent no-op) |
| `emu.drawText(x, y, text)` / `drawRect(x, y, w, h, color)` / `drawPixel(x, y, color)` | No | `Overlay` primitives |
| `emu.pause()` | No | `ScriptBus::pause` |
| `emu.saveState()` / `emu.loadState(bytes)` | No | `ScriptBus::save_state`/`load_state`, wrapping the existing `[1.1.0]` `SaveState` |

A locked `poke`/`setJoystick`/`setConsoleSwitch` surfaces as a Lua runtime
error to the script — verified by tests that the underlying write never
reaches the bus, not just that an error is thrown.

## Testing

18 hand-authored tests against a mock `ScriptBus`: peek/poke round-trip,
lock rejection for `poke`/`setJoystick` (with an explicit "and no side
effect occurred" assertion, not just an error check), joystick/
console-switch recording and unknown-name rejection, CPU snapshot
exposure, `onFrame` firing across multiple ticks and being a no-op when
unregistered, `pause`, save/load-state round-trip and bad-blob error
surfacing, and draw-primitive accumulation + clear-on-take.

## Frontend wiring (`scripting` feature, `rusty2600-frontend/src/scripting.rs`)

A real `ScriptBus` implementation, a `scripting` feature flag (off by
default, pulling in `rusty2600-script` + a direct `mlua` dependency for
`mlua::Error`), a live `onFrame` hook, and a `Tools -> Load Script...`
menu entry now exist. Load a `.lua` file via the file picker; `onFrame`
fires once per real emulated frame thereafter.

**Design: a per-frame-synced `System` clone, not a live pointer.**
`ScriptEngine<B>` owns its bound bus behind an `Rc<RefCell<B>>` with
`B: 'static`, fixed at construction — but the real `EmuCore` lives behind
`Arc<Mutex<EmuCore>>`, reachable only as a short-lived `MutexGuard`
re-acquired every render pass, with no `'static` reference to hand the
engine without `unsafe` raw pointers or a much larger architectural
change. Since `System` is cheaply `Clone` and this crate's own doc already
notes "the 2600's entire mutable game-state is 128 B of RIOT RAM plus a
handful of registers," `FrontendScriptBus` instead owns a **private
`System` clone**, synced from the real `EmuCore.system` at the start of
each tick and copied back after: `peek`/`poke`/`cpu()` and
`saveState`/`loadState` are exact, real operations against that copy (a
script's captured blob is a genuine live encode, not an approximation);
`setJoystick`/`setConsoleSwitch` can't apply directly this way (the next
`run_frame` call unconditionally overwrites RIOT/TIA ports from real host
input), so they're recorded into override fields the frontend ORs into the
next frame's real `InputState` instead. An honest, deliberate indirection
layer — not a corner cut — documented in `scripting.rs`'s module doc.

**Overlay compositing landed in `[2.3.0]`.** `app.rs`'s render pass now
calls `ScriptState::take_overlay()` right after `script.tick(...)` (inside
the same brief emu lock) and draws the result via an unclipped `egui`
foreground layer painter (`app.rs`'s `draw_script_overlay`), piggybacking
on the always-on egui shell pass rather than adding a new
`wgpu::RenderPipeline` — `gfx.rs`/`shader_pass.rs` are untouched. This
gets `drawText` working via egui's own font rasterization (no glyph atlas
to build) at essentially no extra render-pipeline cost.

Coordinates are declared in emulated-frame pixels; the render pass scales
them onto the actual window with a plain linear `screen / framebuffer`
ratio — matching `Gfx::blit`'s own behavior (a fullscreen-triangle stretch
with no aspect-preserving letterbox), so the overlay stays pixel-aligned
with the displayed framebuffer at any window size. `drawText` has no
color or font-size parameter in the `emu` API (see the table above), so
the render pass uses a fixed white color and a size scaled with the
framebuffer-to-screen ratio — a defensible default for an unspecified
API surface, not a guess at unstated real behavior.

Verified without a real GPU: a test constructs a bare `egui::Context`,
runs one frame calling `draw_script_overlay` with a populated `Overlay`,
and inspects the resulting `FullOutput::shapes` (egui's shape list is a
plain, GPU-free data structure) to confirm each primitive kind actually
produces paintable shapes, not just that the function runs without
panicking.

**`WritesLocked` gained a real second field this same pass**:
`netplay_active` (see `docs/netplay.md`) — a connected rollback netplay
session now locks script writes too, for the same reason RA hardcore mode
does: an unreplicated local write would silently desync the two peers'
otherwise bit-identical timelines.

## Debugger Lua console panel (`[2.5.0]`)

Lua's default `print` writes to the real process stdout — invisible in a
GUI app, and previously the only way a script author could see their own
`print` debugging or a runtime error. `ScriptEngine::new` now overrides
the `print` global with a function that stringifies each argument via
Lua's own `tostring` (so a table with a `__tostring` metamethod still
formats correctly) and joins them with tabs, matching real Lua `print`
semantics exactly, then pushes the result into a capped `ScriptLog` ring
buffer (`rusty2600_script::log`, 500 lines, oldest dropped first — a
persistent history, unlike `Overlay`'s per-frame drain-and-clear).
`ScriptEngine::tick_frame` also pushes any `onFrame` runtime error into
the same log (as a distinctly-marked `LogLine::Error`) before returning
it, so the error reaches the console even if the host only logs the
returned `Err` elsewhere.

`crate::debugger::lua_console_panel` (frontend, `scripting` feature)
renders the captured log — a new `Debug -> Lua Console` panel, oldest-
first, errors in red, with a Clear button. Output-only: this is NOT an
interactive Lua REPL. Executing arbitrary ad-hoc Lua from the debugger
would need its own `WritesLocked` determinism-gate integration (matching
what the normal `onFrame` tick already enforces) — real additional
design work, deliberately out of scope for this panel.
