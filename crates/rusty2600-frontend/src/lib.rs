//! `rusty2600_frontend` — the Rusty2600 frontend library: an always-on
//! `winit + wgpu + cpal + egui` shell that drives the `rusty2600_core::System`.
//!
//! ## What this crate is (and the non-negotiable rules)
//!
//! The frontend is the only `std` + `unsafe`-permitted crate (besides the cheevos FFI). It is an
//! **always-on egui shell**, not a bare window: every frame draws a menu bar (File / Emulation /
//! Tools / View / Debug / Help) + a status bar + a tabbed Settings window, with the toggleable
//! debugger panels layered on top.
//!
//! The load-bearing rules, lifted from RustyNES `docs/frontend.md` (see
//! `references/frontend_reuse.md`):
//!
//! 1. **egui runs every frame.** Always-on shell.
//! 2. **Never hold the emu lock inside the egui closure.** Menu interactions return a
//!    [`shell::MenuAction`] dispatched *after* the egui pass; the present path copies the display
//!    buffer under a brief lock, drops the lock, then renders / presents.
//! 3. **On native the emulator runs on a dedicated emu-thread** (see [`emu_thread`]); the winit
//!    thread only does UI + present. (The thread spawn is behind the off-by-default `emu-thread`
//!    feature until the core's `Board` trait gains a `Send` bound; the default is the synchronous
//!    in-`render` drive.)
//! 4. **The frontend owns rate control + run-ahead** — the determinism contract. These live HERE
//!    (a resampler stage / snapshot-restore orchestration), NEVER in the core synthesis. See
//!    [`audio_ring`].
//!
//! ## The 2600 specifics this shell swaps in (vs. the RustyNES NES shell)
//!
//! - **Framebuffer.** The TIA has no framebuffer; it races the beam. The frontend accumulates the
//!   beam-raced pixels into a display buffer ([`present_buffer`]): 160 visible color clocks wide x
//!   192 (NTSC) / 228 (PAL/SECAM) visible lines. 2600 pixels are tall — the classic ~1.8:1
//!   horizontal stretch to a 4:3 display (noted, not perfected, at v0.1).
//! - **Palette.** The 2600 NTSC / PAL / SECAM colour tables ([`palette`]).
//! - **Input.** Joystick (SWCHA + INPT4/5), paddles (INPT0-3, analog), and the console switches on
//!   SWCHB (Select / Reset / Color-B&W / Left & Right Difficulty) — see [`input`]. This differs
//!   from the NES d-pad.
//! - **Debugger panels.** 6507 (A/X/Y/SP/PC/P, breakpoints, disassembly), TIA (object regs + beam
//!   position + collisions), RIOT (timer + I/O ports), memory (peek-based hex viewer) — see
//!   [`debugger`] (behind the `debug-hooks` feature, default-on) and [`shell`] for the overlay
//!   window that hosts them.
//! - **RetroAchievements.** The RIOT's 128 B of RAM is the 2600's only mutable game-state RAM, and
//!   so the entirety of what RA conditions against — see `cheevos` (behind the off-by-default
//!   `retroachievements` feature; a plain code span here, not an intra-doc link, since the module
//!   doesn't exist in the default doc build).
//!
//! ## v-next: extract a shared `rusty-frontend-core`
//!
//! Most of this crate is console-agnostic (the App + winit loop, the emu-thread handle, the
//! present path, the audio ring + rate control, the egui shell scaffold, the CLI). It wants to
//! become a shared `rusty-frontend-core` crate parameterized over a `Console` trait, so RustyNES /
//! RustySNES / Rusty2600 / future Rusty\* cores share one frontend. **Do NOT block v0.1 on
//! extracting it** — lift-and-adapt first, extract once a second consumer exists.

// The frontend is the std + unsafe-permitted crate (workspace lints set `unsafe_code = "warn"`);
// the lock-free audio ring needs it, and every `unsafe` block carries a `// SAFETY:` comment.
#![allow(unsafe_code)]
// v0.1 scaffold allowances. These relax a handful of pedantic/nursery lints that fight a TODO-heavy
// scaffold; they are LOCAL to this crate and come off as the modules grow real bodies:
//   * `doc_markdown` — the docs quote many bare hardware tokens (SWCHA, INPT4, RustyNES, etc.);
//     backticking every one in prose adds noise without value at v0.1.
//   * `struct_excessive_bools` — the panel-visibility / console-switch structs are plain UI flag
//     bags (become egui state later).
//   * `similar_names` — `swcha`/`swchb`, `inpt4`/`inpt5` are the hardware names.
#![allow(
    clippy::doc_markdown,
    clippy::struct_excessive_bools,
    clippy::similar_names
)]

pub mod audio_ring;
pub mod config;
pub mod input;
pub mod palette;
pub mod present_buffer;
pub mod resampler;
/// Extracting a ROM image out of a `.zip` archive.
///
/// Shared by the native `OpenRom` dialog (`app`) and the wasm demo's file
/// loader (`wasm`). Plain code spans, not intra-doc links: `app`/`wasm` are
/// mutually `cfg`-exclusive, so only one exists in any given doc build.
pub mod rom_archive;
/// `?settings=` share-link encode/decode (`[v2.9.0]`).
///
/// Pure encode/decode/query-parsing (tested natively); only the `window.location` read/build
/// helpers are wasm32-gated. See the module doc for the full design (why the whole [`config::Config`]
/// is shareable here, unlike RustyNES's curated subset).
pub mod share_link;
pub mod shell;
/// A constrained RetroArch `.slangp`/`.cgp` shader-preset importer (`v2.10.0`).
///
/// Pure parsing/mapping logic (no GPU, no I/O beyond the caller handing
/// over the file text) — see the module doc for the honesty-gated design
/// (maps known shader-name stems to Rusty2600's own built-in
/// [`rusty2600_gfx_shaders::PassKind`] set; anything unrecognized is
/// reported, never silently dropped).
pub mod slang_preset;

/// The debugger's persistent state, structured chip snapshots, and panel
/// renderers (6507/TIA/RIOT/memory + a standalone disassembler).
///
/// Gated behind `debug-hooks` (default-on, like `emu-thread`) so a minimal
/// build can still opt it out entirely.
#[cfg(feature = "debug-hooks")]
pub mod debugger;

// The always-on egui App shell + the wgpu blit + the run loop. Native, OR wasm32 with the real
// `wasm-winit` build (v2.5.0) — `wasm-canvas`'s bootstrap (`wasm::run_canvas`) is the fallback
// when `wasm-winit` is off. `app.rs`'s own doc comment covers exactly which of its code paths are
// wasm-safe (native-only crates like `rfd`/`gilrs`/`directories` stay behind their own
// `#[cfg(not(target_arch = "wasm32"))]` splits within the file).
#[cfg(any(not(target_arch = "wasm32"), feature = "wasm-winit"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod audio;
#[cfg(any(not(target_arch = "wasm32"), feature = "wasm-winit"))]
pub mod gfx;
/// The composable post-process shader stack `gfx` presents through.
#[cfg(any(not(target_arch = "wasm32"), feature = "wasm-winit"))]
pub mod shader_pass;

/// The player/missile/ball sprite-replacement data model + loader
/// (`hd-pack` feature) — the 2600-appropriate HD-pack analog.
#[cfg(all(not(target_arch = "wasm32"), feature = "hd-pack"))]
pub mod sprite_pack;

// The emulator core handle + shared-state types `app.rs` drives (native, or wasm32 with
// `wasm-winit` — see `app` above). The OS-thread spawn a `emu-thread` build performs stays
// feature-gated INSIDE `app.rs`, not here — `wasm-winit` builds must not enable `emu-thread` (see
// `Cargo.toml`'s doc comment).
#[cfg(any(not(target_arch = "wasm32"), feature = "wasm-winit"))]
pub mod emu_thread;

/// Run-ahead: hides a game's internal input lag.
///
/// Speculatively simulates a few frames ahead of the canonical timeline and
/// displays that, built on [`emu_thread::EmuCore`]'s save-state/rewind
/// snapshot primitives.
#[cfg(not(target_arch = "wasm32"))]
pub mod runahead;

/// RetroAchievements integration.
///
/// Owns the [`rusty2600_cheevos::RaClient`] on the main thread (never inside
/// [`emu_thread::EmuCore`] — the client is deliberately `!Send`). Native-only,
/// behind the off-by-default `retroachievements` feature.
#[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
pub mod cheevos;

/// Lua scripting frontend wiring (`rusty2600-script`, `[1.9.0]`).
///
/// A real [`rusty2600_script::ScriptBus`] implementation over
/// [`emu_thread::EmuCore`] plus a live `onFrame` hook, native-only, behind
/// the off-by-default `scripting` feature. See `docs/scripting.md`.
#[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
pub mod scripting;

/// Rollback netplay frontend wiring (`rusty2600-netplay`, `[1.10.0]`).
///
/// A real host/join-game session driving [`emu_thread::EmuCore`], native-only,
/// behind the off-by-default `netplay` feature. See `docs/netplay.md`.
#[cfg(all(not(target_arch = "wasm32"), feature = "netplay"))]
pub mod netplay_session;

// Native CLI (clap 4) + the structured help-topic registry + the ratatui help TUI. Native-only: a
// browser tab has no terminal.
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(all(not(target_arch = "wasm32"), feature = "help-tui"))]
pub mod help_tui;

// The wasm32 entry point (`#[wasm_bindgen(start)]`). Gated to wasm so it's absent from native
// rustdoc; named here as a code span rather than an intra-doc link.
#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use app::App;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::Cli;
pub use config::Config;
#[cfg(not(target_arch = "wasm32"))]
pub use emu_thread::{EmuCore, EmuHandle, SharedInput};

/// The native NTSC frame rate (the wall-clock pacing target for the produce loop).
pub const FRAME_RATE_NTSC: f64 = 60.098_8;
/// The PAL / SECAM frame rate (region-switchable; the pacing matrix reads it from config).
pub const FRAME_RATE_PAL: f64 = 50.006_98;
