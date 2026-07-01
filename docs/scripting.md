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
  calls into a per-frame buffer a host can consume. Compositing this onto
  the presented frame is part of the deferred frontend integration above.
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

## What's next

Per `to-dos/ROADMAP.md`: a `v1.9.x` follow-up wires `ScriptEngine` into
`rusty2600-frontend` — a real `ScriptBus` implementation over the live
`Bus`/`Cpu`, a `scripting` feature flag (off by default, matching every
other additive feature), an actual overlay-compositing step in the render
pipeline, and a live `onFrame` hook tied to `EmuCore::run_frame`. Only
once that lands does Lua scripting become something a user can actually
attach to a running game.
