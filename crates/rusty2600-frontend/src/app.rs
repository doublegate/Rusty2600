//! The winit [`ApplicationHandler`] that drives the always-on egui shell, the framebuffer present
//! path, the emulator, and audio.
//!
//! Native, or wasm32 with the real `wasm-winit` build (v2.5.0; the fallback bootstrap lives in
//! `wasm.rs`'s `run_canvas`). The structure is the RustyNES `app.rs`, distilled to the load-bearing
//! flow:
//!
//! 1. `resumed()` (the winit 0.30 idiom) creates the window + [`Gfx`] + the egui integration and
//!    powers on the [`EmuCore`]. On native this is fully synchronous; on wasm32, `Gfx::new_async`'s
//!    adapter/device acquisition can't block the browser's single JS thread (`pollster::block_on`
//!    would hang), so it runs inside a `wasm_bindgen_futures::spawn_local` task that writes the
//!    finished `Active` into `App::active` once ready — see `resumed`'s wasm branch. Every
//!    OTHER `ApplicationHandler` method treats a still-`None` `active` as "not ready yet" and
//!    no-ops, exactly like the native "not yet resumed" case already had to handle.
//! 2. `window_event()` feeds input to egui, late-latches the host input into the
//!    [`crate::input::InputState`], and on `RedrawRequested` runs one render:
//!    - step one frame + copy the framebuffer out under a BRIEF emu lock, then DROP the lock;
//!    - blit it via wgpu;
//!    - run the egui shell pass (which NEVER touches the emu lock) and collect [`MenuAction`]s;
//!    - present;
//!    - dispatch the collected actions AFTER the egui pass.
//!
//! The frontend owns pacing + run-ahead; the core never sees wall-clock time (determinism).
//!
//! ## wasm32 (`wasm-winit`) status — see `docs/frontend.md` for the authoritative version
//!
//! `emu-thread`/`debug-hooks`/`retroachievements`/`scripting`/`netplay` are NOT wasm-safe and must
//! stay off for this build (see `Cargo.toml`'s doc comments) — this file's `#[cfg(feature = ...)]`
//! blocks for those simply don't compile in. `MenuAction::OpenRom` and `SaveConfig`/`SetRegion`'s
//! `Config::save()` call are handled with wasm-specific paths (see their dispatch arms and
//! `config.rs`'s wasm `save()` stub) since `rfd`/`directories` aren't available on this target.

use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::rc::Rc;
#[cfg(feature = "emu-thread")]
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[cfg(not(target_arch = "wasm32"))]
use crate::cli::Cli;
use crate::config::Config;
use crate::emu_thread::EmuCore;
use crate::gfx::Gfx;
use crate::input::InputState;
use crate::shell::{ConsoleSwitchAction, MenuAction, ShellInfo, ShellState};

// wasm32: bytes staged by the hidden `<input type=file>`'s `onchange` handler
// (`App::trigger_wasm_rom_picker`), drained once per frame by `App::render` — see both for why
// this can't just write directly into a live `Active`/`EmuCore` (the callback fires from a
// detached JS event, with no `&mut Active` in scope).
#[cfg(target_arch = "wasm32")]
thread_local! {
    static PENDING_ROM: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    static ROM_INPUT: RefCell<Option<web_sys::HtmlInputElement>> = const { RefCell::new(None) };
}

/// The live application state, constructed in `resumed()` (the winit 0.30 idiom — a window cannot
/// be created before the event loop resumes).
struct Active {
    window: Arc<Window>,
    gfx: Gfx,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    /// The emulator, shared with the present path (and the optional emu-thread).
    core: Arc<Mutex<EmuCore>>,
    /// The current host-input accumulation (late-latched into the core each frame).
    host_input: InputState,
    /// The egui shell's persistent UI state.
    shell: ShellState,
    /// The live frontend config (the Settings window edits it in place).
    config: Config,
    /// Whether the window is currently fullscreen (toggled from the View menu).
    fullscreen: bool,
    /// The running audio output stream (keeps the cpal device alive). Native-only — see
    /// `emu_thread.rs`'s `audio_tx` doc comment for why wasm-winit has no audio yet.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    audio_out: Option<crate::audio::AudioOutput>,
    /// Wall-clock deadline for the next emulated frame. `about_to_wait` requests a
    /// redraw every event-loop iteration with no inherent rate limit (the display's
    /// present-mode vsync isn't a reliable substitute — it caps to the MONITOR's
    /// refresh rate, not the console's ~60.0988 Hz NTSC rate, and doesn't always
    /// block the way Fifo nominally promises across platforms). Without this gate
    /// `run_frame()` gets called once per `RedrawRequested`, which can fire far
    /// faster than real NTSC time — the emulator visibly runs "fast." This is the
    /// pacing the module doc comment promises ("the frontend owns pacing").
    ///
    /// Only meaningful on the synchronous (`not(emu-thread)`) path: when the emu
    /// thread owns stepping, ITS run loop paces via the audio ring buffer's fill
    /// ratio instead (see `emu_thread.rs`'s spawned closure).
    #[cfg(not(feature = "emu-thread"))]
    next_step_at: web_time::Instant,

    #[cfg(feature = "emu-thread")]
    shared_input: Arc<crate::emu_thread::SharedInput>,
    #[cfg(feature = "emu-thread")]
    frame_rx: crate::present_buffer::Consumer,
    /// The live run-ahead frame count (`crate::runahead`).
    ///
    /// Shared with the emu-thread so a Settings-window change takes effect
    /// without a restart. `0` = off (the default) — a plain
    /// `EmuCore::step_frame` call with no speculative work.
    #[cfg(feature = "emu-thread")]
    runahead_frames: Arc<AtomicU32>,
    /// The most recently PUBLISHED frame's pixels, reused when `frame_rx.take()`
    /// finds nothing new this render pass. The emu-thread only publishes at the
    /// region's frame rate (~60 Hz NTSC), while `RedrawRequested` fires with no
    /// rate limit of its own (see `about_to_wait`'s doc comment) — every render
    /// pass in between MUST redisplay the last real frame, not a black one:
    /// `EmuCore::framebuffer` is only ever written by the non-`emu-thread`
    /// (`run_frame`) path and stays permanently black under this feature, so
    /// falling back to it here used to flash black on most render passes (the
    /// "entire display flickers" bug).
    #[cfg(feature = "emu-thread")]
    last_frame: Vec<u8>,

    /// Timestamp of the previous render pass, for the smoothed FPS estimate.
    last_render_at: web_time::Instant,
    /// An exponential-moving-average FPS estimate shown in the status bar
    /// (`ShellInfo::fps`) — measures actual render-pass cadence, not the
    /// emulator's internal frame-production rate, so it reflects what the
    /// user is actually seeing (display refresh rate under `emu-thread`).
    fps_smoothed: f32,

    /// The RetroAchievements client. Lives here (main thread), never inside
    /// `EmuCore` — `RaClient` is deliberately `!Send`, and `EmuCore` must stay
    /// `Send` for the default-on `emu-thread` feature.
    #[cfg(feature = "retroachievements")]
    cheevos: crate::cheevos::CheevosState,

    /// The currently-loaded Lua script, if any (`File -> Load Script...`).
    /// Lives here (main thread) for the same reason `cheevos` does: `mlua`'s
    /// `Lua` VM is `!Send`, and `EmuCore` must stay `Send` for `emu-thread`.
    #[cfg(feature = "scripting")]
    script: Option<crate::scripting::ScriptState>,

    /// The active rollback netplay session, if any (`Tools -> Netplay...`).
    /// See `netplay_session.rs`'s module doc for why this bypasses the
    /// `emu-thread` background loop while active.
    #[cfg(feature = "netplay")]
    netplay: Option<crate::netplay_session::NetplaySession>,
}

/// The app: holds the config + the deferred ROM path until `resumed()` builds `Active`.
///
/// `active` is a shared cell (not a plain `Option`) so the wasm32 build can populate it from a
/// detached `wasm_bindgen_futures::spawn_local` task — see this module's wasm32 doc section above.
/// `Rc<RefCell<_>>` (not `Arc<Mutex<_>>`) is deliberate: both targets drive every
/// `ApplicationHandler` callback on one thread (native: the winit/main thread; wasm32: the single
/// browser JS thread), so there is never real concurrent access, and `Rc`/`RefCell` are cheaper.
pub struct App {
    config: Config,
    #[cfg(not(target_arch = "wasm32"))]
    pending_rom: Option<PathBuf>,
    seed: u64,
    active: Rc<RefCell<Option<Active>>>,
}

impl App {
    /// Create the app from parsed CLI options + a loaded config. The window + emulator are built
    /// when the event loop resumes. Native-only (`Cli` doesn't exist on wasm32 — use
    /// [`Self::with_config`] there).
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn new(cli: &Cli) -> Self {
        Self {
            config: Config::load(),
            pending_rom: cli.rom.clone(),
            seed: 0,
            active: Rc::new(RefCell::new(None)),
        }
    }

    /// Create the app with an explicit config + optional ROM (used by `main.rs` natively; the
    /// wasm32 `wasm-winit` entry point (`wasm.rs::run_winit`) calls this with `Config::default()`
    /// and `rom: None` — the wasm32 target has no `pending_rom` field at all, see below).
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn with_config(config: Config, rom: Option<PathBuf>) -> Self {
        Self {
            config,
            pending_rom: rom,
            seed: 0,
            active: Rc::new(RefCell::new(None)),
        }
    }

    /// wasm32's `with_config` — no `pending_rom`/`PathBuf` on this target (ROM loading is
    /// browser-file-input-driven, wired in `resumed()`'s wasm branch, not a startup CLI path).
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn with_config(config: Config) -> Self {
        Self {
            config,
            seed: 0,
            active: Rc::new(RefCell::new(None)),
        }
    }

    /// Run the native event loop to completion (blocking).
    ///
    /// # Errors
    /// Returns any [`winit::error::EventLoopError`] from creating or running the loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(mut self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = EventLoop::new()?;
        event_loop.run_app(&mut self)
    }

    /// Run the wasm32 event loop. Unlike [`Self::run`], this returns immediately — the browser's
    /// single JS thread can't block on a `run_app`-style loop, so winit drives events via its own
    /// internal `requestAnimationFrame`/task-scheduler loop instead (`EventLoopExtWebSys::
    /// spawn_app`).
    ///
    /// # Errors
    /// Returns any [`winit::error::EventLoopError`] from creating the event loop.
    #[cfg(target_arch = "wasm32")]
    pub fn run(self) -> Result<(), winit::error::EventLoopError> {
        use winit::platform::web::EventLoopExtWebSys;
        let event_loop = EventLoop::new()?;
        event_loop.spawn_app(self);
        Ok(())
    }
}

impl ApplicationHandler for App {
    // Intentionally cohesive: this is the egui/winit window+device bring-up sequence; splitting
    // it apart would scatter tightly-sequenced init steps with no real reuse benefit.
    #[allow(clippy::too_many_lines)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.borrow().is_some() {
            return; // already initialized (e.g. resumed after suspend)
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("Rusty2600")
            .with_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));
        // wasm32: let winit create its own `<canvas>` and auto-insert it into the page (simpler
        // and less error-prone than pre-declaring one in `index.html` and wiring it up by ID).
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_append(true);
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                Self::log_error(&format!("failed to create window: {e}"));
                event_loop.exit();
                return;
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let gfx = match Gfx::new(Arc::clone(&window), &self.config.video.present_mode) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("rusty2600: wgpu init failed: {e}");
                    event_loop.exit();
                    return;
                }
            };
            let (egui_ctx, egui_state, egui_renderer) = Self::build_egui(&gfx, &window);

            // Power on the emulator at the configured region.
            let mut emu = EmuCore::new(self.seed);
            emu.region = self.config.region;
            #[cfg(feature = "retroachievements")]
            let mut cheevos = crate::cheevos::CheevosState::default();
            if let Some(path) = self.pending_rom.take() {
                match std::fs::read(&path) {
                    Ok(raw_bytes) => {
                        let filename = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        let extracted = if crate::rom_archive::looks_like_zip(&filename) {
                            crate::rom_archive::extract_first_rom(&raw_bytes)
                        } else {
                            Ok((raw_bytes, filename))
                        };
                        match extracted {
                            Ok((bytes, _entry_name)) => {
                                if let Err(e) = emu.load_rom(&bytes) {
                                    eprintln!("rusty2600: failed to load {}: {e}", path.display());
                                } else {
                                    #[cfg(feature = "retroachievements")]
                                    cheevos.load_rom(&bytes);
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "rusty2600: failed to extract a ROM from {}: {e}",
                                    path.display()
                                );
                            }
                        }
                    }
                    Err(e) => eprintln!("rusty2600: cannot read {}: {e}", path.display()),
                }
            }

            let (audio_out, audio_tx) = match crate::audio::AudioOutput::new() {
                Ok(pair) => (Some(pair.0), Some(pair.1)),
                Err(e) => {
                    eprintln!("rusty2600: failed to start audio: {e}");
                    (None, None)
                }
            };
            emu.audio_tx = audio_tx;

            // `Arc<Mutex<EmuCore>>` is the right shape for the dedicated `emu-thread` + the present
            // path. `EmuCore` is `Send` (via `Bus::board`'s concrete `Cartridge` enum, not
            // `Box<dyn Board>`) — the stale blocker this comment used to describe is resolved
            // (`T-0503-001`, DONE); see `Cargo.toml`'s `emu-thread` feature comment for the history.
            #[allow(clippy::arc_with_non_send_sync)]
            let core = Arc::new(Mutex::new(emu));

            #[cfg(feature = "emu-thread")]
            let (shared_input, frame_rx, runahead_frames) = {
                let shared_input = Arc::new(crate::emu_thread::SharedInput::new());
                let (frame_tx, frame_rx) = crate::present_buffer::channel();
                let runahead_frames = Arc::new(AtomicU32::new(self.config.video.runahead_frames));

                let thread_core = Arc::clone(&core);
                let thread_input = Arc::clone(&shared_input);
                let thread_runahead = Arc::clone(&runahead_frames);
                std::thread::Builder::new()
                    .name("emu-thread".into())
                    .spawn(move || {
                        loop {
                            let mut lock = thread_core.lock().unwrap();
                            let input = thread_input.load();
                            let paused = lock.paused || !lock.rom_loaded;

                            // Pace the emulator using the audio ring buffer fill ratio.
                            // If it's over 60% full, sleep to let the audio consumer drain it.
                            let fill = if paused {
                                0.0
                            } else {
                                lock.audio_tx
                                    .as_ref()
                                    .map_or(0.0, crate::audio::AudioProducer::fill_ratio)
                            };

                            if !paused && fill < 0.6 {
                                let ra = thread_runahead.load(Ordering::Relaxed);
                                crate::runahead::step_frame(&mut lock, &frame_tx, Some(input), ra);
                                drop(lock);
                            } else {
                                drop(lock);
                                std::thread::sleep(std::time::Duration::from_millis(1));
                            }
                        }
                    })
                    .expect("failed to spawn emu thread");

                (shared_input, frame_rx, runahead_frames)
            };

            *self.active.borrow_mut() = Some(Active {
                window,
                gfx,
                egui_ctx,
                egui_state,
                egui_renderer,
                core,
                host_input: InputState::default(),
                shell: ShellState::default(),
                config: self.config.clone(),
                fullscreen: false,
                audio_out,
                #[cfg(not(feature = "emu-thread"))]
                next_step_at: web_time::Instant::now(),
                #[cfg(feature = "emu-thread")]
                shared_input,
                #[cfg(feature = "emu-thread")]
                frame_rx,
                #[cfg(feature = "emu-thread")]
                runahead_frames,
                #[cfg(feature = "emu-thread")]
                last_frame: vec![0u8; (crate::gfx::VCS_W * crate::gfx::VCS_H_NTSC * 4) as usize],
                last_render_at: web_time::Instant::now(),
                fps_smoothed: 0.0,
                #[cfg(feature = "retroachievements")]
                cheevos,
                #[cfg(feature = "scripting")]
                script: None,
                #[cfg(feature = "netplay")]
                netplay: None,
            });
        }

        // wasm32: `Gfx::new_async`'s adapter/device acquisition can't run on `pollster::block_on`
        // (no blocking primitives on the browser's single JS thread — see this module's doc
        // comment). Kick off the async bring-up as a detached task that writes the finished
        // `Active` into the shared cell once ready; every other `ApplicationHandler` method
        // already treats "not ready yet" (a `None` `active`) as a no-op, so this is safe.
        #[cfg(target_arch = "wasm32")]
        {
            let active_cell = Rc::clone(&self.active);
            let config = self.config.clone();
            let seed = self.seed;
            wasm_bindgen_futures::spawn_local(async move {
                let gfx =
                    match Gfx::new_async(Arc::clone(&window), &config.video.present_mode).await {
                        Ok(g) => g,
                        Err(e) => {
                            Self::log_error(&format!("wgpu init failed: {e}"));
                            return;
                        }
                    };
                let (egui_ctx, egui_state, egui_renderer) = Self::build_egui(&gfx, &window);

                let mut emu = EmuCore::new(seed);
                emu.region = config.region;
                // No `pending_rom`/audio on wasm32 startup — ROM loading is wired via a hidden
                // `<input type=file>` in `wasm.rs::run_winit` (routed through `MenuAction::
                // OpenRom`'s wasm arm below), and audio is explicitly this release's stretch
                // goal, not yet wired (see `docs/frontend.md`'s wasm-winit status section).
                #[allow(clippy::arc_with_non_send_sync)]
                let core = Arc::new(Mutex::new(emu));

                *active_cell.borrow_mut() = Some(Active {
                    window,
                    gfx,
                    egui_ctx,
                    egui_state,
                    egui_renderer,
                    core,
                    host_input: InputState::default(),
                    shell: ShellState::default(),
                    config,
                    fullscreen: false,
                    next_step_at: web_time::Instant::now(),
                    last_render_at: web_time::Instant::now(),
                    fps_smoothed: 0.0,
                });
                web_sys::console::log_1(
                    &"Rusty2600 wasm-winit — armed. File > Open ROM to begin.".into(),
                );
            });
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let mut guard = self.active.borrow_mut();
        let Some(active) = guard.as_mut() else {
            return;
        };
        // Feed egui first; if it consumes the event we still latch keys for the emulator below.
        let _ = active.egui_state.on_window_event(&active.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                active.gfx.resize(size.width, size.height);
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                Self::latch_key(active, &key_event);
            }
            WindowEvent::RedrawRequested => {
                let actions = Self::render(active);
                Self::dispatch_actions(active, event_loop, actions);
                active.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(active) = self.active.borrow().as_ref() {
            active.window.request_redraw();
        }
    }
}

impl App {
    /// Build the egui integration for `window`/`gfx` — shared by both the native (synchronous)
    /// and wasm32 (post-`Gfx::new_async`) `resumed()` bring-up paths.
    fn build_egui(
        gfx: &Gfx,
        window: &Arc<Window>,
    ) -> (egui::Context, egui_winit::State, egui_wgpu::Renderer) {
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        (egui_ctx, egui_state, egui_renderer)
    }

    /// Log an error to stderr (native) or the browser devtools console (wasm32).
    fn log_error(msg: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!("rusty2600: {msg}");
        #[cfg(target_arch = "wasm32")]
        web_sys::console::error_1(&format!("rusty2600: {msg}").into());
    }

    /// wasm32's `MenuAction::OpenRom` — `rfd`'s native file dialog isn't available on this
    /// target (it's not even a wasm32 dependency, see `Cargo.toml`'s
    /// `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` block), so this drives a
    /// hidden `<input type=file>` instead, reusing the exact same FileReader +
    /// `rom_archive::extract_first_rom` pattern `wasm.rs`'s canvas-2D bootstrap already proved.
    /// The element is created once (cached in `ROM_INPUT`) and `.click()`-ed on every call — a
    /// synthetic click on a hidden file input is an established, browser-accepted way to open
    /// the native file picker from a real user gesture (the menu click that got us here).
    #[cfg(target_arch = "wasm32")]
    fn trigger_wasm_rom_picker() {
        use wasm_bindgen::JsCast as _;
        use wasm_bindgen::prelude::*;

        let input = ROM_INPUT.with(|cell| {
            let mut cell = cell.borrow_mut();
            if cell.is_none() {
                let Some(window) = web_sys::window() else {
                    return None;
                };
                let Some(document) = window.document() else {
                    return None;
                };
                let Ok(el) = document.create_element("input") else {
                    return None;
                };
                let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() else {
                    return None;
                };
                input.set_type("file");
                input.set_accept(".a26,.bin,.rom,.zip");
                let style = input.style();
                let _ = style.set_property("display", "none");
                if let Some(body) = document.body() {
                    let _ = body.append_child(&input);
                }

                let on_change =
                    Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
                        let Some(target_input) = ev
                            .target()
                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                        else {
                            return;
                        };
                        let Some(files) = target_input.files() else {
                            return;
                        };
                        let Some(file) = files.get(0) else { return };
                        let filename = file.name();

                        let Ok(reader) = web_sys::FileReader::new() else {
                            return;
                        };
                        let reader_clone = reader.clone();
                        let on_load = Closure::<dyn FnMut()>::new(move || {
                            let Ok(buffer) = reader_clone.result() else {
                                return;
                            };
                            let array = js_sys::Uint8Array::new(&buffer);
                            let raw_bytes = array.to_vec();
                            let bytes = if crate::rom_archive::looks_like_zip(&filename) {
                                match crate::rom_archive::extract_first_rom(&raw_bytes) {
                                    Ok((extracted, _entry_name)) => extracted,
                                    Err(e) => {
                                        Self::log_error(&format!("zip extraction failed: {e}"));
                                        return;
                                    }
                                }
                            } else {
                                raw_bytes
                            };
                            PENDING_ROM.with(|p| *p.borrow_mut() = Some(bytes));
                        });
                        reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                        on_load.forget();
                        let _ = reader.read_as_array_buffer(&file);
                    });
                input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
                on_change.forget();

                *cell = Some(input);
            }
            cell.clone()
        });
        if let Some(input) = input {
            input.click();
        }
    }

    /// Build a side-effect-free [`crate::debugger::DebugSnapshot`] from the live
    /// system state, using [`rusty2600_core::Bus::peek_range`] (never the mutating
    /// `cpu_read`) for the disassembly + memory views so opening the debugger can
    /// never itself perturb bankswitch state or RIOT timer/read latches.
    #[cfg(feature = "debug-hooks")]
    fn build_debug_snapshot(emu: &mut EmuCore, memory_base: u16) -> crate::debugger::DebugSnapshot {
        use crate::debugger::{CpuSnapshot, RiotSnapshot, TiaSnapshot};

        let cpu = CpuSnapshot {
            a: emu.system.cpu.a,
            x: emu.system.cpu.x,
            y: emu.system.cpu.y,
            s: emu.system.cpu.s,
            pc: emu.system.cpu.pc,
            p: format!("{:?}", emu.system.cpu.p),
        };

        let objects = &emu.system.bus.tia.objects;
        let c = &emu.system.bus.tia.collisions;
        let tia = TiaSnapshot {
            scanline: emu.system.bus.tia.scanline,
            color_clock: emu.system.bus.tia.color_clock,
            pos: objects.pos,
            colu: objects.colu,
            collisions: format!(
                "CXM0P:{:02X} CXM1P:{:02X} CXP0FB:{:02X} CXP1FB:{:02X} CXM0FB:{:02X} CXM1FB:{:02X} CXBLPF:{:02X} CXPPMM:{:02X}",
                c.cxm0p, c.cxm1p, c.cxp0fb, c.cxp1fb, c.cxm0fb, c.cxm1fb, c.cxblpf, c.cxppmm
            ),
            nusiz: objects.nusiz,
            hm: objects.hm,
            refp: objects.refp,
        };

        let riot_chip = &emu.system.bus.riot;
        let riot = RiotSnapshot {
            timer_value: riot_chip.timer.value,
            timer_prescale: format!("{:?}", riot_chip.timer.prescale),
            ports: riot_chip.ports,
            ddr: riot_chip.ddr,
        };

        // One bulk peek_range covers a generous worst case (16 instructions x 3
        // bytes) so the disassembly loop never needs a peek per instruction.
        let disasm_base = emu.system.cpu.pc;
        let disasm_bytes = emu.system.bus.peek_range(disasm_base, 48);
        let mut disassembly_at_pc = Vec::new();
        let mut offset: u16 = 0;
        for _ in 0..16 {
            if usize::from(offset) >= disasm_bytes.len() {
                break;
            }
            let addr = disasm_base.wrapping_add(offset);
            let inst = crate::debugger::disasm::disassemble_one(
                |a| {
                    let idx = usize::from(a.wrapping_sub(disasm_base));
                    disasm_bytes.get(idx).copied().unwrap_or(0)
                },
                addr,
            );
            offset = offset.wrapping_add(inst.len);
            disassembly_at_pc.push((addr, inst.text));
        }

        let memory_view = emu
            .system
            .bus
            .peek_range(memory_base, crate::debugger::MEMORY_VIEW_LEN);

        let riot_ram = emu.system.bus.peek_range(0x0080, 128);

        // Recording is enabled here (not just gated to the Events panel
        // specifically) so switching to that panel immediately shows this
        // frame's writes rather than waiting a frame for the flag to catch
        // up; the cost is one extra `bool` check + capped `Vec` push per
        // write, only while the debugger overlay is open at all.
        emu.system.bus.write_log.enabled = true;
        let tia_writes = emu.system.bus.write_log.events().to_vec();
        emu.system.bus.write_log.clear();

        crate::debugger::DebugSnapshot {
            cpu,
            tia,
            riot,
            disassembly_at_pc,
            memory_view,
            riot_ram,
            frame: emu.frame_count,
            tia_writes,
        }
    }

    /// Late-latch a key event into the host-input accumulation (per the config binding table).
    fn latch_key(active: &mut Active, key: &winit::event::KeyEvent) {
        let pressed = key.state.is_pressed();
        // winit logical key is mapped via the physical KeyCode debug name (the binding scheme in
        // `input::KeyBindings`).
        if let winit::keyboard::PhysicalKey::Code(code) = key.physical_key {
            let name = format!("{code:?}");
            if let Some(action) = active.config.p1.action_for(&name) {
                active.host_input.apply_action(action, pressed);
            }
        }
    }

    /// One render: step a frame + copy the framebuffer under a brief lock, blit, run the egui
    /// shell, present. Returns the menu actions to dispatch AFTER this pass (never dispatched
    /// mid-egui).
    // Intentionally cohesive: this is the immediate-mode per-frame render pass (lock, step,
    // blit, run the egui shell, present); egui UI functions are conventionally long.
    #[allow(clippy::too_many_lines)]
    fn render(active: &mut Active) -> Vec<MenuAction> {
        // wasm32: a ROM picked via the hidden `<input type=file>` (see
        // `trigger_wasm_rom_picker`) lands here asynchronously — its `onchange` handler can't
        // reach `active` directly (it fires from a detached JS callback, not from inside this
        // render pass), so it stages the bytes in `PENDING_ROM` and this drains them once per
        // frame, the same "check a shared cell at a known point" pattern `resumed()`'s wasm
        // branch uses for `Gfx` bring-up.
        #[cfg(target_arch = "wasm32")]
        if let Some(bytes) = PENDING_ROM.with(|p| p.borrow_mut().take()) {
            let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
            match emu.load_rom(&bytes) {
                Ok(()) => active.shell.status = format!("Loaded ({} bytes)", bytes.len()),
                Err(e) => active.shell.status = format!("load failed: {e}"),
            }
        }

        #[cfg(feature = "emu-thread")]
        active.shared_input.store(active.host_input);

        // Smoothed (EMA) render-pass FPS for the status bar — measures the actual
        // presented cadence, not the emulator's internal frame-production rate, so
        // it reflects what's really on screen (which under `emu-thread` tracks the
        // display's refresh rate, not necessarily the console's ~60 Hz).
        let now = web_time::Instant::now();
        let dt = now.duration_since(active.last_render_at).as_secs_f32();
        active.last_render_at = now;
        if dt > 0.0 {
            let instantaneous = 1.0 / dt;
            active.fps_smoothed = if active.fps_smoothed <= 0.0 {
                instantaneous
            } else {
                active.fps_smoothed.mul_add(0.9, instantaneous * 0.1)
            };
        }

        // --- (1) Step one frame + copy the framebuffer + read-only info under a BRIEF lock. ---
        let (fb, fb_dims, info) = {
            let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);

            // Rollback netplay bypasses BOTH the emu-thread background loop
            // (suppressed via `emu.paused`, set when the session started —
            // see `MenuAction::NetplayConnect`'s dispatch handler) and the
            // synchronous non-`emu-thread` stepping branch below (a no-op
            // anyway while paused): `RollbackSession` owns and steps its own
            // internal `System`, so this branch drives it directly and
            // copies the result into `emu.system` for the existing
            // video/audio post-processing (`extract_frame`) to consume. See
            // `netplay_session.rs`'s module doc for the full rationale.
            #[cfg(feature = "netplay")]
            let netplay_active = if let Some(session) = active.netplay.as_mut() {
                session.poll();
                match session.advance_frame(&active.host_input) {
                    Ok(true) => {
                        emu.system = session.system().clone();
                        emu.extract_frame();
                    }
                    Ok(false) => {}
                    Err(e) => {
                        active.shell.status = format!("Netplay error: {e}");
                        active.netplay = None;
                        emu.paused = false;
                    }
                }
                true
            } else {
                false
            };
            #[cfg(not(feature = "netplay"))]
            let netplay_active = false;

            #[cfg(not(feature = "emu-thread"))]
            if !netplay_active {
                let now = web_time::Instant::now();
                if now >= active.next_step_at {
                    emu.run_frame(Some(active.host_input));
                    let period = std::time::Duration::from_secs_f64(1.0 / emu.region.frame_rate());
                    active.next_step_at += period;
                    // If we fell far behind (window was minimized, the machine was
                    // suspended, a debugger paused us, ...), don't burst-catch-up by
                    // running a pile of frames back-to-back next time — that would
                    // look like fast-forward. Re-anchor to "one period from now."
                    if active.next_step_at + period < now {
                        active.next_step_at = now + period;
                    }
                }
                // Otherwise: skip stepping the emulator this pass. `framebuffer()`
                // below still returns the last-produced frame, so the present path
                // keeps rendering something (UI stays responsive) without the
                // emulator racing ahead of NTSC real time.
            }

            let fb = if netplay_active {
                emu.framebuffer().to_vec()
            } else {
                #[cfg(feature = "emu-thread")]
                {
                    match active.frame_rx.take() {
                        Some(f) => {
                            active.last_frame = f.pixels;
                            active.last_frame.clone()
                        }
                        None => active.last_frame.clone(),
                    }
                }
                #[cfg(not(feature = "emu-thread"))]
                {
                    emu.framebuffer().to_vec()
                }
            };

            let dims = emu.fb_dims();
            #[cfg(feature = "debug-hooks")]
            let debug = if active.shell.debugger_visible && emu.rom_loaded {
                Some(Self::build_debug_snapshot(
                    &mut emu,
                    active.shell.debugger.memory_base,
                ))
            } else {
                // No point paying the write-log recording cost while the
                // debugger overlay (and specifically its Events panel) isn't
                // even open.
                emu.system.bus.write_log.enabled = false;
                None
            };
            #[cfg(feature = "retroachievements")]
            if emu.rom_loaded {
                let events = active.cheevos.pump(&mut |addr| emu.system.bus.peek(addr));
                if let Some(msg) = events.into_iter().next_back() {
                    active.shell.status = msg;
                }
            }
            #[cfg(feature = "scripting")]
            let script_overlay = if emu.rom_loaded
                && let Some(script) = active.script.as_mut()
            {
                let locked = rusty2600_script::WritesLocked {
                    #[cfg(feature = "retroachievements")]
                    ra_hardcore: active.cheevos.hardcore_enabled(),
                    #[cfg(not(feature = "retroachievements"))]
                    ra_hardcore: false,
                    #[cfg(feature = "netplay")]
                    netplay_active: active.netplay.is_some(),
                    #[cfg(not(feature = "netplay"))]
                    netplay_active: false,
                };
                script.tick(&mut emu, locked, &mut active.host_input);
                script.take_overlay()
            } else {
                rusty2600_script::Overlay::default()
            };
            #[cfg(not(target_arch = "wasm32"))]
            let rom_tag = emu.rom_tag();
            #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
            let mut info = ShellInfo {
                board_tier: emu.board_tier().map(str::to_string),
                region: emu.region,
                fps: active.fps_smoothed,
                rom_loaded: emu.rom_loaded,
                #[cfg(feature = "debug-hooks")]
                debug,
                #[cfg(feature = "retroachievements")]
                cheevos_hardcore: active.cheevos.hardcore_enabled(),
                #[cfg(feature = "retroachievements")]
                cheevos_game_loaded: active.cheevos.game_loaded(),
                #[cfg(feature = "scripting")]
                script_loaded: active.script.is_some(),
                #[cfg(feature = "scripting")]
                overlay: script_overlay,
                #[cfg(feature = "netplay")]
                netplay_active: active.netplay.is_some(),
                #[cfg(not(target_arch = "wasm32"))]
                save_slots: Vec::new(),
            };
            drop(emu); // release the brief lock BEFORE the wgpu upload + egui pass
            // Probing save-slot status is 8 filesystem `stat` calls — too
            // expensive to redo every frame at 60+ FPS, so `active.shell`
            // caches the result and this only re-probes when the loaded ROM
            // changed or a save/load-slot action marked the cache dirty (see
            // `ShellState::save_slots_cache`'s doc comment). Deliberately
            // done AFTER dropping the emu lock either way.
            #[cfg(not(target_arch = "wasm32"))]
            {
                if rom_tag != active.shell.save_slots_cache_rom_tag || active.shell.save_slots_dirty
                {
                    active.shell.save_slots_cache = rom_tag.map_or_else(Vec::new, |tag| {
                        (0..crate::config::SAVE_SLOT_COUNT)
                            .map(|slot| crate::shell::SaveSlotInfo::probe(tag, slot))
                            .collect()
                    });
                    active.shell.save_slots_cache_rom_tag = rom_tag;
                    active.shell.save_slots_dirty = false;
                }
                info.save_slots.clone_from(&active.shell.save_slots_cache);
            }
            (fb, dims, info)
        };
        active.gfx.upload(&fb, fb_dims.0, fb_dims.1);

        // --- (2) Acquire the surface. ---
        let Some(frame) = active.gfx.acquire() else {
            return Vec::new();
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            active
                .gfx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rusty2600-frame"),
                });

        // --- (3) Present the framebuffer (base blit + any active shader-stack passes). ---
        active
            .gfx
            .present(&mut encoder, &view, &active.config.video.shader_passes);

        // --- (4) Run the always-on egui shell pass. The shell NEVER touches the emu lock. ---
        let raw_input = active.egui_state.take_egui_input(&active.window);
        let mut actions = Vec::new();
        // Split disjoint borrows so the egui closure can take `&mut shell` + `&mut config` while
        // `run_ui` borrows the (cloned) context. Read out BEFORE the splits below (a plain field
        // read, not a `&mut active` method call, so it doesn't conflict with them).
        #[cfg(feature = "scripting")]
        let screen_size = (active.gfx.config.width, active.gfx.config.height);
        let ctx = active.egui_ctx.clone();
        let shell = &mut active.shell;
        let config = &mut active.config;
        #[cfg(feature = "retroachievements")]
        let cheevos = &mut active.cheevos;
        #[cfg(feature = "scripting")]
        let active_script = &active.script;
        let full_output = ctx.run_ui(raw_input, |ui| {
            actions = shell.render(
                ui,
                &info,
                config,
                #[cfg(feature = "retroachievements")]
                cheevos,
                #[cfg(feature = "scripting")]
                active_script.as_ref(),
            );
            // The script overlay is a distinct concern from the menu/status-bar
            // chrome `ShellState::render` owns, so it's composited here rather
            // than folded into that function. Uses an unclipped foreground
            // layer painter (not `ui`'s own clip rect, which panels above may
            // have narrowed) so the overlay always covers the full framebuffer
            // area regardless of which debugger/menu panels are open.
            #[cfg(feature = "scripting")]
            draw_script_overlay(ui.ctx(), &info.overlay, fb_dims, screen_size);
        });
        active
            .egui_state
            .handle_platform_output(&active.window, full_output.platform_output);
        let tris = active
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [active.gfx.config.width, active.gfx.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };
        for (id, delta) in &full_output.textures_delta.set {
            active
                .egui_renderer
                .update_texture(&active.gfx.device, &active.gfx.queue, *id, delta);
        }
        active.egui_renderer.update_buffers(
            &active.gfx.device,
            &active.gfx.queue,
            &mut encoder,
            &tris,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rusty2600-egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load, // keep the framebuffer blit underneath
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            active.egui_renderer.render(&mut pass, &tris, &screen);
        }
        for id in &full_output.textures_delta.free {
            active.egui_renderer.free_texture(id);
        }

        // --- (5) Submit + present. ---
        active.gfx.queue.submit(Some(encoder.finish()));
        frame.present();
        actions
    }

    /// Dispatch the menu actions collected during the egui pass (AFTER the pass, so the emu lock is
    /// never taken inside the egui closure).
    // `significant_drop_tightening`: the `DebugStep`/`DebugContinue` arms genuinely need the emu
    // lock held for their entire body (read-then-step-then-update-callstack, or the whole
    // stepping loop) — there's no meaningful narrower scope to tighten to.
    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
    fn dispatch_actions(
        active: &mut Active,
        event_loop: &ActiveEventLoop,
        actions: Vec<MenuAction>,
    ) {
        for action in actions {
            match action {
                #[cfg(target_arch = "wasm32")]
                MenuAction::OpenRom => Self::trigger_wasm_rom_picker(),
                #[cfg(not(target_arch = "wasm32"))]
                MenuAction::OpenRom => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Atari 2600 ROM", &["a26", "bin", "rom", "zip"])
                        .pick_file()
                    {
                        match std::fs::read(&path) {
                            Ok(raw_bytes) => {
                                let filename = path.file_name().map_or_else(
                                    || path.display().to_string(),
                                    |n| n.to_string_lossy().into_owned(),
                                );
                                let extracted = if crate::rom_archive::looks_like_zip(&filename) {
                                    crate::rom_archive::extract_first_rom(&raw_bytes)
                                        .map(|(bytes, entry_name)| (bytes, Some(entry_name)))
                                } else {
                                    Ok((raw_bytes, None))
                                };
                                match extracted {
                                    Ok((bytes, entry_name)) => {
                                        let mut emu = active
                                            .core
                                            .lock()
                                            .unwrap_or_else(PoisonError::into_inner);
                                        let load_result = emu.load_rom(&bytes);
                                        if let Err(e) = &load_result {
                                            active.shell.status = format!("load failed: {e}");
                                        } else {
                                            let rom_label = entry_name
                                                .as_ref()
                                                .map_or_else(|| filename.clone(), Clone::clone);
                                            active.shell.status = entry_name.as_ref().map_or_else(
                                                || format!("Loaded {rom_label}"),
                                                |entry| format!("Loaded {entry} (from {filename})"),
                                            );
                                            active
                                                .window
                                                .set_title(&format!("Rusty2600 - {rom_label}"));
                                        }
                                        drop(emu);
                                        // Only spend the (potentially slow —
                                        // MD5 + RetroAchievements lookup)
                                        // cheevos load on a ROM that actually
                                        // loaded; an invalid/unsupported
                                        // image has nothing to identify.
                                        if load_result.is_ok() {
                                            #[cfg(feature = "retroachievements")]
                                            active.cheevos.load_rom(&bytes);
                                        }
                                    }
                                    Err(e) => {
                                        active.shell.status = format!("zip extraction failed: {e}");
                                    }
                                }
                            }
                            Err(e) => active.shell.status = format!("read failed: {e}"),
                        }
                    }
                }
                MenuAction::CloseRom => {
                    active
                        .core
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .close_rom();
                    active.window.set_title("Rusty2600");
                    // The emu-thread stops publishing once `rom_loaded` is false
                    // (see its `paused` gate), so without this the cached
                    // `last_frame` fallback would keep showing the last game
                    // frame forever instead of going blank.
                    #[cfg(feature = "emu-thread")]
                    active.last_frame.iter_mut().for_each(|b| *b = 0);
                    #[cfg(feature = "retroachievements")]
                    active.cheevos.close_rom();
                    active.shell.status = "ROM closed".into();
                }
                #[cfg(not(target_arch = "wasm32"))]
                MenuAction::SaveStateSlot(slot) => {
                    let emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    let Some(rom_tag) = emu.rom_tag() else {
                        drop(emu);
                        active.shell.status = "no ROM loaded".into();
                        continue;
                    };
                    let bytes = rusty2600_core::SaveState::capture(&emu.system, rom_tag).encode();
                    drop(emu);
                    let Some(path) = crate::config::save_slot_path(rom_tag, slot) else {
                        active.shell.status = "no writable save directory".into();
                        continue;
                    };
                    let result = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(&path, &bytes));
                    active.shell.status = match result {
                        Ok(()) => format!("Saved to slot {slot}"),
                        Err(e) => format!("save failed: {e}"),
                    };
                    // The slot's on-disk status (existence/timestamp) may
                    // have just changed — force the next frame's menu probe
                    // rather than waiting for the loaded ROM to change too.
                    active.shell.save_slots_dirty = true;
                }
                #[cfg(not(target_arch = "wasm32"))]
                MenuAction::LoadStateSlot(slot) => {
                    let rom_tag = active
                        .core
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .rom_tag();
                    let Some(rom_tag) = rom_tag else {
                        active.shell.status = "no ROM loaded".into();
                        continue;
                    };
                    let Some(path) = crate::config::save_slot_path(rom_tag, slot) else {
                        active.shell.status = "no writable save directory".into();
                        continue;
                    };
                    active.shell.status = match std::fs::read(&path) {
                        Ok(bytes) => match rusty2600_core::SaveState::restore(&bytes, rom_tag) {
                            Ok(system) => {
                                let mut emu =
                                    active.core.lock().unwrap_or_else(PoisonError::into_inner);
                                emu.system = system;
                                // The rewind ring holds snapshots from
                                // BEFORE this load — without clearing it,
                                // pressing Rewind right after a slot load
                                // would jump back to the pre-load timeline,
                                // which is a discontinuous, confusing jump
                                // relative to what the player just did.
                                emu.snapshots.clear();
                                drop(emu);
                                format!("Loaded slot {slot}")
                            }
                            Err(e) => format!("slot {slot} load failed: {e}"),
                        },
                        Err(e) => format!("slot {slot} read failed: {e}"),
                    };
                }
                #[cfg(feature = "retroachievements")]
                MenuAction::ToggleHardcore => {
                    let enabled = !active.cheevos.hardcore_enabled();
                    active.cheevos.set_hardcore_enabled(enabled);
                    active.shell.status =
                        format!("Hardcore mode: {}", if enabled { "on" } else { "off" });
                }
                MenuAction::SetRegion(region) => {
                    active.config.region = region;
                    active
                        .core
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .region = region;
                    let _ = active.config.save();
                    active.shell.status = format!("Region: {}", region.label());
                }
                MenuAction::ConsoleSwitch(sw) => Self::apply_console_switch(active, sw),
                MenuAction::TogglePause => {
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    emu.paused = !emu.paused;
                    active.shell.paused = emu.paused;
                }
                MenuAction::ToggleDebugger => {
                    active.shell.debugger_visible = !active.shell.debugger_visible;
                }
                #[cfg(feature = "debug-hooks")]
                MenuAction::DebugStep => {
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    emu.paused = true;
                    active.shell.paused = true;
                    // A single side-effect-free peek is trivially cheap here
                    // (unlike `MenuAction::DebugContinue`'s tight loop, where
                    // `Bus::peek`'s full-clone cost would multiply badly) —
                    // safe to keep the call stack accurate on every Step.
                    let pc_before = emu.system.cpu.pc;
                    let opcode = emu.system.bus.peek(pc_before);
                    emu.system.step_instruction();
                    crate::debugger::callstack::track_instruction(
                        &mut active.shell.debugger.call_stack,
                        opcode,
                        pc_before,
                    );
                }
                #[cfg(feature = "debug-hooks")]
                MenuAction::DebugContinue => {
                    // Safety cap: never spin forever if no breakpoint is ever hit.
                    const MAX_STEPS: u32 = 1_000_000;
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    emu.paused = true;
                    active.shell.paused = true;
                    for _ in 0..MAX_STEPS {
                        // NOTE: the call stack is deliberately NOT tracked here —
                        // `Bus::peek`'s full-clone cost (TIA's video/audio
                        // buffers included) would multiply across up to
                        // MAX_STEPS iterations. Only `DebugStep` (one call per
                        // click, never a loop) keeps it accurate.
                        emu.system.step_instruction();
                        if active
                            .shell
                            .debugger
                            .breakpoints
                            .contains(&emu.system.cpu.pc)
                        {
                            break;
                        }
                        // Watch-armed conditional breakpoints: read RIOT RAM
                        // directly (a slice reference, not `Bus::peek`'s
                        // clone) so this check stays cheap even at MAX_STEPS.
                        let ctx = crate::debugger::expr::EvalContext {
                            a: emu.system.cpu.a,
                            x: emu.system.cpu.x,
                            y: emu.system.cpu.y,
                            s: emu.system.cpu.s,
                            pc: emu.system.cpu.pc,
                            scanline: emu.system.bus.tia.scanline,
                            color_clock: emu.system.bus.tia.color_clock,
                            frame: emu.frame_count,
                            mem: &emu.system.bus.riot.ram,
                            mem_base: 0x0080,
                        };
                        if active.shell.debugger.any_breakpoint_watch_triggered(&ctx) {
                            break;
                        }
                    }
                }
                #[cfg(feature = "debug-hooks")]
                MenuAction::TastudioJumpToFrame(target) => {
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    // `EmuCore::rewind()` restores `system` from the snapshot
                    // ring but doesn't track frame numbering itself (that's
                    // this frontend's own `frame_count` bookkeeping) — so
                    // walk it back one frame per rewind, bounded by however
                    // much history the 600-entry ring actually has.
                    while emu.frame_count > target as u64 && !emu.snapshots.is_empty() {
                        emu.rewind();
                        emu.frame_count = emu.frame_count.saturating_sub(1);
                    }
                }
                #[cfg(feature = "debug-hooks")]
                MenuAction::TastudioSaveBranch => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Rusty2600 TAS movie", &["r26m"])
                        .save_file()
                    {
                        let emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                        let region = match emu.region {
                            crate::palette::Region::Ntsc => rusty2600_core::MovieRegion::Ntsc,
                            crate::palette::Region::Pal => rusty2600_core::MovieRegion::Pal,
                            crate::palette::Region::Secam => rusty2600_core::MovieRegion::Secam,
                        };
                        let bytes =
                            active
                                .shell
                                .debugger
                                .tastudio
                                .save_branch(0, region, &emu.system);
                        drop(emu);
                        match std::fs::write(&path, bytes) {
                            Ok(()) => {
                                active.shell.status = format!("Branch saved to {}", path.display());
                            }
                            Err(e) => active.shell.status = format!("branch save failed: {e}"),
                        }
                    }
                }
                MenuAction::ToggleFullscreen => {
                    active.fullscreen = !active.fullscreen;
                    let mode = active
                        .fullscreen
                        .then_some(winit::window::Fullscreen::Borderless(None));
                    active.window.set_fullscreen(mode);
                }
                MenuAction::OpenSettings => active.shell.settings_open = true,
                MenuAction::SaveConfig => {
                    // Sync the live run-ahead atomic the emu-thread reads, so a
                    // Settings-window change takes effect without a restart.
                    #[cfg(feature = "emu-thread")]
                    active
                        .runahead_frames
                        .store(active.config.video.runahead_frames, Ordering::Relaxed);
                    let _ = active.config.save();
                }
                MenuAction::Quit => event_loop.exit(),
                #[cfg(feature = "scripting")]
                MenuAction::LoadScript => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Lua script", &["lua"])
                        .pick_file()
                    {
                        match crate::scripting::ScriptState::load(&path, 0) {
                            Ok(state) => {
                                active.script = Some(state);
                                active.shell.status = format!("Loaded script {}", path.display());
                            }
                            Err(e) => active.shell.status = format!("script load failed: {e}"),
                        }
                    }
                }
                #[cfg(feature = "scripting")]
                MenuAction::UnloadScript => {
                    active.script = None;
                    active.shell.status = "Script unloaded".into();
                }
                #[cfg(feature = "netplay")]
                MenuAction::NetplayConnect {
                    local_port,
                    remote_addr,
                } => {
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    if emu.rom_loaded {
                        match crate::netplay_session::NetplaySession::connect(
                            local_port,
                            remote_addr,
                            emu.system.clone(),
                            0,
                        ) {
                            Ok(session) => {
                                // The background emu-thread (if compiled in) must
                                // never concurrently step `emu.system` while the
                                // synchronous netplay branch in `render()` owns
                                // it — see `netplay_session.rs`'s module doc.
                                emu.paused = true;
                                active.netplay = Some(session);
                                active.shell.status =
                                    format!("Netplay: connecting to {remote_addr}...");
                            }
                            Err(e) => {
                                active.shell.status = format!("Netplay connect failed: {e}");
                            }
                        }
                    } else {
                        active.shell.status = "Netplay: load a ROM first".into();
                    }
                }
                #[cfg(feature = "netplay")]
                MenuAction::NetplayConnectStun {
                    local_port,
                    remote_public_addr,
                } => {
                    let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
                    if emu.rom_loaded {
                        active.shell.status =
                            "Netplay: discovering public address via STUN...".into();
                        match crate::netplay_session::NetplaySession::connect_via_stun(
                            local_port,
                            remote_public_addr,
                            emu.system.clone(),
                            0,
                        ) {
                            Ok((session, public_addr)) => {
                                // See `MenuAction::NetplayConnect`'s identical
                                // rationale for pausing the background emu-thread.
                                emu.paused = true;
                                active.netplay = Some(session);
                                active.shell.status = format!(
                                    "Netplay: your address is {public_addr}; \
                                     connecting to {remote_public_addr}..."
                                );
                            }
                            Err(e) => {
                                active.shell.status = format!("Netplay STUN connect failed: {e}");
                            }
                        }
                    } else {
                        active.shell.status = "Netplay: load a ROM first".into();
                    }
                }
                #[cfg(feature = "netplay")]
                MenuAction::NetplayDisconnect => {
                    active.netplay = None;
                    active
                        .core
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .paused = false;
                    active.shell.status = "Netplay disconnected".into();
                }
                // TODO(impl-phase): Reset / PowerCycle / OpenDocs wire to the core / Docs pane.
                MenuAction::Reset | MenuAction::PowerCycle | MenuAction::OpenDocs => {
                    active.shell.status = format!("{action:?}: TODO");
                }
            }
        }
    }

    /// Apply a console-switch toggle to the host-input state. The core latches it on the next
    /// frame (the momentary switches track the menu click; the latches flip once).
    const fn apply_console_switch(active: &mut Active, sw: ConsoleSwitchAction) {
        let switches = &mut active.host_input.switches;
        match sw {
            ConsoleSwitchAction::ToggleColor => switches.color = !switches.color,
            ConsoleSwitchAction::ToggleLeftDifficulty => {
                switches.left_difficulty = switches.left_difficulty.toggled();
            }
            ConsoleSwitchAction::ToggleRightDifficulty => {
                switches.right_difficulty = switches.right_difficulty.toggled();
            }
        }
    }
}

/// Composite a script's accumulated `emu.drawText`/`drawRect`/`drawPixel`
/// output over the presented frame, via an unclipped `egui` foreground
/// layer painter (not a new `wgpu::RenderPipeline` — see `docs/scripting.md`
/// for the "why piggyback on the existing egui pass" rationale).
///
/// `overlay`'s coordinates are declared in emulated-frame pixels
/// (`0..=fb_dims.0`/`0..=fb_dims.1`). `Gfx::blit`'s base framebuffer draw is
/// a fullscreen-triangle stretch with no aspect-preserving letterbox (it
/// samples the active sub-rect across the WHOLE surface, not a centered
/// sub-region) — so a plain linear `screen / fb` scale, not a letterboxed
/// one, is what actually keeps the overlay pixel-aligned with the displayed
/// framebuffer at any window size.
// Every cast here is `u32`/`i32` -> `f32` screen-space math (well within
// `f32`'s 23-bit mantissa for any real window/framebuffer size) or an
// intentional 32-bit-to-8-bit color-channel unpack in `color32_from_packed`
// below — see `gfx.rs`'s identical `#![allow(...)]` for this crate's
// established rationale on this exact lint pair.
#[cfg(feature = "scripting")]
#[allow(clippy::cast_precision_loss)]
fn draw_script_overlay(
    ctx: &egui::Context,
    overlay: &rusty2600_script::Overlay,
    fb_dims: (u32, u32),
    screen_size: (u32, u32),
) {
    if overlay.is_empty() {
        return;
    }
    // `.max(1)`: never divide by a zero framebuffer dimension (e.g. before
    // the first ROM load ever populates `fb_dims`).
    let scale_x = screen_size.0 as f32 / fb_dims.0.max(1) as f32;
    let scale_y = screen_size.1 as f32 / fb_dims.1.max(1) as f32;

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("rusty2600_script_overlay"),
    ));

    for r in &overlay.rects {
        let min = egui::pos2(r.x as f32 * scale_x, r.y as f32 * scale_y);
        let size = egui::vec2(
            (r.w as f32 * scale_x).max(1.0),
            (r.h as f32 * scale_y).max(1.0),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(min, size),
            0,
            color32_from_packed(r.color),
        );
    }
    for p in &overlay.pixels {
        // Drawn as a scaled filled rect, not a single device pixel, so it
        // stays visible (matches one emulated pixel's on-screen footprint)
        // at any window size — a literal 1x1 dot would vanish at high scale.
        let min = egui::pos2(p.x as f32 * scale_x, p.y as f32 * scale_y);
        let size = egui::vec2(scale_x.max(1.0), scale_y.max(1.0));
        painter.rect_filled(
            egui::Rect::from_min_size(min, size),
            0,
            color32_from_packed(p.color),
        );
    }
    for t in &overlay.texts {
        let pos = egui::pos2(t.x as f32 * scale_x, t.y as f32 * scale_y);
        // `emu.drawText(x, y, text)` has no color parameter (see
        // `rusty2600-script`'s `TextPrimitive`) and no font-size parameter
        // either — a fixed white color and a size scaled with the
        // framebuffer-to-screen ratio (so text stays roughly one
        // emulated-scanline tall relative to the display) are both
        // reasonable defaults for an API that doesn't specify either, not a
        // silent guess at unstated real behavior.
        let font = egui::FontId::monospace((8.0 * scale_y).max(6.0));
        painter.text(
            pos,
            egui::Align2::LEFT_TOP,
            &t.text,
            font,
            egui::Color32::WHITE,
        );
    }
}

/// Unpack a script primitive's `0xRRGGBB`-packed color into `egui::Color32`.
// Each shifted-and-masked byte below always fits in a `u8` by construction
// (an `& 0xFF` per channel would be redundant given the `as u8` truncation
// already discards everything above bit 7) — same convention this project's
// other RGB-unpack sites already use (e.g. `rusty2600-mobile`'s `run_frame`).
#[cfg(feature = "scripting")]
#[allow(clippy::cast_possible_truncation)]
const fn color32_from_packed(rgb: u32) -> egui::Color32 {
    egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Region;

    #[test]
    fn console_switch_toggles_color() {
        let mut input = InputState::default();
        assert!(input.switches.color);
        // Emulate the dispatch path's toggle (the active state isn't constructible without a
        // window, so exercise the same field flip the handler performs).
        input.switches.color = !input.switches.color;
        assert!(!input.switches.color);
    }

    #[test]
    fn region_label_round_trips() {
        assert_eq!(Region::Pal.label(), "PAL");
        assert_eq!(Region::Secam.label(), "SECAM");
    }

    #[cfg(feature = "scripting")]
    mod script_overlay {
        use super::*;

        #[test]
        fn color32_from_packed_unpacks_each_channel() {
            let c = color32_from_packed(0x11_22_33);
            assert_eq!((c.r(), c.g(), c.b()), (0x11, 0x22, 0x33));
        }

        /// The real, GPU-free proof this feature works: build a bare `egui::Context`,
        /// run one frame calling `draw_script_overlay`, and inspect the resulting
        /// `FullOutput::shapes` — egui's shape list is a plain data structure,
        /// constructible and inspectable with no wgpu surface/device involved.
        #[test]
        fn overlay_primitives_reach_the_egui_paint_list() {
            let overlay = rusty2600_script::Overlay {
                texts: vec![rusty2600_script::TextPrimitive {
                    x: 4,
                    y: 8,
                    text: "hi".to_string(),
                }],
                rects: vec![rusty2600_script::RectPrimitive {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                    color: 0x00_FF_00,
                }],
                pixels: vec![rusty2600_script::PixelPrimitive {
                    x: 1,
                    y: 1,
                    color: 0xFF_00_00,
                }],
            };

            let ctx = egui::Context::default();
            let full_output = ctx.run_ui(egui::RawInput::default(), |ui| {
                draw_script_overlay(ui.ctx(), &overlay, (160, 192), (320, 384));
            });

            // One rect shape for the `RectPrimitive`, one for the `PixelPrimitive`
            // (drawn as a scaled filled rect, see `draw_script_overlay`'s doc), and
            // at least one text-bearing shape for the `TextPrimitive` — confirms all
            // three primitive kinds actually produced paintable output, not just that
            // the function ran without panicking.
            let rect_count = full_output
                .shapes
                .iter()
                .filter(|cs| matches!(cs.shape, egui::Shape::Rect(_)))
                .count();
            assert_eq!(
                rect_count, 2,
                "expected one rect shape + one pixel-as-rect shape"
            );

            let has_text = full_output
                .shapes
                .iter()
                .any(|cs| matches!(cs.shape, egui::Shape::Text(_)));
            assert!(
                has_text,
                "expected the drawText primitive to produce a text shape"
            );
        }

        #[test]
        fn empty_overlay_produces_no_shapes() {
            let overlay = rusty2600_script::Overlay::default();
            let ctx = egui::Context::default();
            let full_output = ctx.run_ui(egui::RawInput::default(), |ui| {
                draw_script_overlay(ui.ctx(), &overlay, (160, 192), (320, 384));
            });
            assert!(
                full_output.shapes.is_empty(),
                "an empty Overlay must not add any shapes to the paint list"
            );
        }
    }
}
