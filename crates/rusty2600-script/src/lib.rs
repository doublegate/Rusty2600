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
//!
//! ## `v2.9.0` "Full Circle" update — `piccolo` actually investigated, still deferred
//!
//! This release's plan called for finally landing the `piccolo`-backed
//! `wasm32` fallback flagged above. It was investigated for real this time
//! (docs.rs, the upstream README, the upstream `stdlib/` source tree, the
//! `Executor` API) rather than re-deferred on the same reasoning as before
//! — and the native `mlua`-on-`wasm32` wall was independently reconfirmed
//! by direct build attempt (`cargo check --target wasm32-unknown-unknown
//! -p rusty2600-script` fails in `lua-src`'s build script with "don't know
//! how to build Lua for wasm32-unknown-unknown" — a hard wall, not a bug
//! to work around). The new finding is that `piccolo` itself is not yet a
//! workable substrate either, for two compounding reasons:
//!
//! 1. **Stdlib gap.** `piccolo`'s only crates.io-published release
//!    (`0.3.3`, June 2024) implements almost none of Lua's `string`/
//!    `table` libraries — per its own README, "the `io`, `file`, `os`,
//!    `package`, `string`, `table`, and `utf8` libs are either missing or
//!    very sparsely implemented." That rules out exactly the kind of
//!    ordinary script this crate's own `emu` API invites — `string.format`
//!    to build an `emu.drawText` debug label, or `table.insert` to track
//!    per-frame state across `onFrame` calls. Upstream `master` has since
//!    grown partial `string.rs`/`table.rs` implementations (active 2025
//!    commit history), but nothing past `0.3.3` has been published to
//!    crates.io, and this workspace has no git dependency anywhere else —
//!    pinning one just for this feature would be a first, and a risky one
//!    against a crate whose own README says "expect *frequent* pre-1.0 API
//!    breakage, this crate is still very experimental."
//! 2. **Architecture mismatch, not a drop-in swap.** `piccolo` is a
//!    `gc-arena`-based "stackless" VM: an `Executor<'gc>` is driven by
//!    repeated `Executor::step(ctx, fuel) -> bool` calls inside an arena
//!    `mutate`/`finish` scope, and the `Executor` itself is neither `Send`
//!    nor storable outside that arena's lifetime. `engine.rs`'s current
//!    design — a plain owned `Lua` VM, `Rc<RefCell<B>>`-captured host
//!    closures, an `mlua::RegistryKey` held across frames for `onFrame` —
//!    has no equivalent "keep the VM around, call into it once per real
//!    frame" pattern in `piccolo` without first building real
//!    `gc-arena`/`Rootable!` plumbing of its own. That is a genuine rewrite
//!    of the engine's whole embedding shape, not a second
//!    `engine_piccolo.rs` sibling reusing `engine.rs`'s shape.
//!
//! **Decision: defer again, honestly, on a narrower and more concrete
//! basis than before.** This is not a scope-discipline judgment call this
//! time so much as a confirmed external blocker: the only realistic
//! fallback candidate's published version cannot run realistic scripts,
//! and its unpublished branch is explicitly labeled pre-1.0-unstable by
//! its own maintainer. Forcing an implementation against either horn of
//! that trade-off would risk shipping either a stdlib-crippled engine or a
//! git-pinned dependency on volatile, breakage-prone code — both worse
//! outcomes than shipping nothing this release. In-browser Lua scripting
//! remains unsupported on `wasm32`; the native `mlua` backend is completely
//! unchanged (same crate, same version pin, same feature gating). Revisit
//! once `piccolo` publishes a crates.io release with real `string`/`table`
//! coverage — that is an upstream milestone, not one this project
//! controls or can schedule against.

mod bus;
mod engine;
mod lock;
/// Captured `print()`/error output — see `log.rs`'s module doc.
pub mod log;
mod overlay;

pub use bus::{CpuSnapshot, JoyDirection, ScriptBus};
pub use engine::ScriptEngine;
pub use lock::WritesLocked;
pub use log::{LogLine, ScriptLog};
pub use overlay::{Overlay, PixelPrimitive, RectPrimitive, TextPrimitive};
