//! `rusty2600-script` — Lua scripting for Rusty2600, off by default.
//!
//! Native backend only in this release (`mlua`, vendoring Lua 5.4 via a C
//! compiler, matching how `rusty2600-cheevos` already vendors `rcheevos`).
//! A pure-Rust `piccolo`-backed wasm fallback (per the plan's own staging)
//! is deliberately NOT attempted here: `piccolo` is a materially less
//! mature project than `mlua` (fewer standard-library facilities, less
//! battle-tested), and landing a second, non-byte-parity scripting backend
//! alongside a brand-new API surface in the same release would risk
//! shipping either half-tested. This crate's `emu` table + [`ScriptBus`]
//! seam are designed to be backend-agnostic (nothing here depends on
//! `mlua`-specific types outside `engine.rs`), so a `piccolo` backend is a
//! genuine, scoped follow-up rather than a rewrite — the same
//! honest-partial-landing call this project has made repeatedly (the
//! sprite-pack render splice, live movie-recording wiring, the ARM
//! interpreter's cart-board wiring).
//!
//! The API surface is deliberately smaller than `RustyNES`'s own PPU/APU-heavy
//! `emu` table: the 2600's entire mutable game-state is 128 B of RIOT RAM
//! plus a handful of memory-mapped TIA/RIOT registers, so there's much less
//! to expose. See [`ScriptBus`] for the exact surface and [`WritesLocked`]
//! for the determinism gate every script-driven WRITE is checked against.
//!
//! This crate never touches `rusty2600-core`/`rusty2600-frontend` types
//! directly — a host (the frontend, or a test) implements [`ScriptBus`]
//! over whatever real state it owns. Overlay-drawing primitives
//! (`emu.drawText`/`drawRect`/`drawPixel`) accumulate into an [`Overlay`]
//! a host can consume every frame; see `overlay.rs` for why compositing
//! that into the presented frame isn't wired in this release.

mod bus;
mod engine;
mod lock;
mod overlay;

pub use bus::{CpuSnapshot, JoyDirection, ScriptBus};
pub use engine::ScriptEngine;
pub use lock::WritesLocked;
pub use overlay::{Overlay, PixelPrimitive, RectPrimitive, TextPrimitive};
