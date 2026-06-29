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
//! - **Debugger panels.** 6507 (A/X/Y/SP/PC/P), TIA (object regs + beam position), RIOT (timer +
//!   I/O ports), memory — see [`shell`].
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
pub mod shell;

// The always-on egui App shell + the wgpu blit + the run loop. Native only — wasm routes through
// `wasm::start`.
#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod audio;
#[cfg(not(target_arch = "wasm32"))]
pub mod gfx;

// The dedicated emulation thread + the shared-state handles (native; the thread spawn itself is
// behind the `emu-thread` feature, but the handle types + the `EmuCore` are always present).
#[cfg(not(target_arch = "wasm32"))]
pub mod emu_thread;

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
#[cfg(not(target_arch = "wasm32"))]
pub use emu_thread::{EmuCore, EmuHandle, SharedInput};

/// The native NTSC frame rate (the wall-clock pacing target for the produce loop).
pub const FRAME_RATE_NTSC: f64 = 60.098_8;
/// The PAL / SECAM frame rate (region-switchable; the pacing matrix reads it from config).
pub const FRAME_RATE_PAL: f64 = 50.006_98;
