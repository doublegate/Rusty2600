//! The winit [`ApplicationHandler`] that drives the always-on egui shell, the framebuffer present
//! path, the emulator, and audio.
//!
//! Native only (the wasm path lives in `wasm.rs`). The structure is the RustyNES `app.rs`,
//! distilled to the load-bearing flow:
//!
//! 1. `resumed()` (the winit 0.30 idiom) creates the window + [`Gfx`] + the egui integration and
//!    powers on the [`EmuCore`].
//! 2. `window_event()` feeds input to egui, late-latches the host input into the
//!    [`crate::input::InputState`], and on `RedrawRequested` runs one render:
//!    - step one frame + copy the framebuffer out under a BRIEF emu lock, then DROP the lock;
//!    - blit it via wgpu;
//!    - run the egui shell pass (which NEVER touches the emu lock) and collect [`MenuAction`]s;
//!    - present;
//!    - dispatch the collected actions AFTER the egui pass.
//!
//! The frontend owns pacing + run-ahead; the core never sees wall-clock time (determinism).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::cli::Cli;
use crate::config::Config;
use crate::emu_thread::EmuCore;
use crate::gfx::Gfx;
use crate::input::InputState;
use crate::shell::{ConsoleSwitchAction, MenuAction, ShellInfo, ShellState};

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
    /// The running audio output stream (keeps the cpal device alive).
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
    next_step_at: std::time::Instant,

    #[cfg(feature = "emu-thread")]
    shared_input: Arc<crate::emu_thread::SharedInput>,
    #[cfg(feature = "emu-thread")]
    frame_rx: crate::present_buffer::Consumer,
}

/// The app: holds the config + the deferred ROM path until `resumed()` builds `Active`.
pub struct App {
    config: Config,
    pending_rom: Option<PathBuf>,
    seed: u64,
    active: Option<Active>,
}

impl App {
    /// Create the app from parsed CLI options + a loaded config. The window + emulator are built
    /// when the event loop resumes.
    #[must_use]
    pub fn new(cli: &Cli) -> Self {
        Self {
            config: Config::load(),
            pending_rom: cli.rom.clone(),
            seed: 0,
            active: None,
        }
    }

    /// Create the app with an explicit config + optional ROM (used by `main.rs`).
    #[must_use]
    pub const fn with_config(config: Config, rom: Option<PathBuf>) -> Self {
        Self {
            config,
            pending_rom: rom,
            seed: 0,
            active: None,
        }
    }

    /// Run the native event loop to completion.
    ///
    /// # Errors
    /// Returns any [`winit::error::EventLoopError`] from creating or running the loop.
    pub fn run(mut self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = EventLoop::new()?;
        event_loop.run_app(&mut self)
    }
}

impl ApplicationHandler for App {
    // Intentionally cohesive: this is the egui/winit window+device bring-up sequence; splitting
    // it apart would scatter tightly-sequenced init steps with no real reuse benefit.
    #[allow(clippy::too_many_lines)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.is_some() {
            return; // already initialized (e.g. resumed after suspend)
        }
        let attrs = Window::default_attributes()
            .with_title("Rusty2600")
            .with_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("rusty2600: failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };
        let gfx = match Gfx::new(Arc::clone(&window), &self.config.video.present_mode) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("rusty2600: wgpu init failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.config.format,
            egui_wgpu::RendererOptions::default(),
        );

        // Power on the emulator at the configured region.
        let mut emu = EmuCore::new(self.seed);
        emu.region = self.config.region;
        if let Some(path) = self.pending_rom.take() {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    if let Err(e) = emu.load_rom(&bytes) {
                        eprintln!("rusty2600: failed to load {}: {e}", path.display());
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
        let (shared_input, frame_rx) = {
            let shared_input = Arc::new(crate::emu_thread::SharedInput::new());
            let (frame_tx, frame_rx) = crate::present_buffer::channel();

            let thread_core = Arc::clone(&core);
            let thread_input = Arc::clone(&shared_input);
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
                            lock.step_frame(&frame_tx, Some(input));
                            drop(lock);
                        } else {
                            drop(lock);
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                    }
                })
                .expect("failed to spawn emu thread");

            (shared_input, frame_rx)
        };

        self.active = Some(Active {
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
            next_step_at: std::time::Instant::now(),
            #[cfg(feature = "emu-thread")]
            shared_input,
            #[cfg(feature = "emu-thread")]
            frame_rx,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(active) = self.active.as_mut() else {
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
        if let Some(active) = self.active.as_ref() {
            active.window.request_redraw();
        }
    }
}

impl App {
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
        #[cfg(feature = "emu-thread")]
        active.shared_input.store(active.host_input);

        // --- (1) Step one frame + copy the framebuffer + read-only info under a BRIEF lock. ---
        let (fb, fb_dims, info) = {
            #[cfg(feature = "emu-thread")]
            let emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
            #[cfg(not(feature = "emu-thread"))]
            let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);

            #[cfg(not(feature = "emu-thread"))]
            {
                let now = std::time::Instant::now();
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

            #[cfg(feature = "emu-thread")]
            let fb = active
                .frame_rx
                .take()
                .map_or_else(|| emu.framebuffer().to_vec(), |f| f.pixels);
            #[cfg(not(feature = "emu-thread"))]
            let fb = emu.framebuffer().to_vec();

            let dims = emu.fb_dims();
            let info = ShellInfo {
                board_tier: emu.board_tier().map(str::to_string),
                region: emu.region,
                fps: 0.0, // TODO(impl-phase): wire the pacer's smoothed FPS estimate.
                rom_loaded: emu.rom_loaded,
                cpu_info: format!(
                    "A: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PC: {:04X}, P: {:?}",
                    emu.system.cpu.a,
                    emu.system.cpu.x,
                    emu.system.cpu.y,
                    emu.system.cpu.s,
                    emu.system.cpu.pc,
                    emu.system.cpu.p
                ),
                tia_info: format!(
                    "Color Clock: {}, Scanline: {}",
                    emu.system.bus.tia.color_clock, emu.system.bus.tia.scanline
                ),
                riot_info: format!(
                    "Pins: {:02X} {:02X}, DDR: {:02X} {:02X}, INTIM: {:02X}",
                    emu.system.bus.riot.pins[0],
                    emu.system.bus.riot.pins[1],
                    emu.system.bus.riot.ddr[0],
                    emu.system.bus.riot.ddr[1],
                    emu.system.bus.riot.timer.value
                ),
            };
            drop(emu); // release the brief lock BEFORE the wgpu upload + egui pass
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

        // --- (3) Blit the framebuffer (clears then draws the fullscreen triangle). ---
        active.gfx.blit(&mut encoder, &view);

        // --- (4) Run the always-on egui shell pass. The shell NEVER touches the emu lock. ---
        let raw_input = active.egui_state.take_egui_input(&active.window);
        let mut actions = Vec::new();
        // Split disjoint borrows so the egui closure can take `&mut shell` + `&mut config` while
        // `run_ui` borrows the (cloned) context.
        let ctx = active.egui_ctx.clone();
        let shell = &mut active.shell;
        let config = &mut active.config;
        let full_output = ctx.run_ui(raw_input, |ui| {
            actions = shell.render(ui, &info, config);
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
    fn dispatch_actions(
        active: &mut Active,
        event_loop: &ActiveEventLoop,
        actions: Vec<MenuAction>,
    ) {
        for action in actions {
            match action {
                MenuAction::OpenRom => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Atari 2600 ROM", &["a26", "bin", "rom"])
                        .pick_file()
                    {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                let mut emu =
                                    active.core.lock().unwrap_or_else(PoisonError::into_inner);
                                if let Err(e) = emu.load_rom(&bytes) {
                                    active.shell.status = format!("load failed: {e}");
                                } else {
                                    active.shell.status = format!("Loaded {}", path.display());
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
                    active.shell.status = "ROM closed".into();
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
                MenuAction::ToggleFullscreen => {
                    active.fullscreen = !active.fullscreen;
                    let mode = active
                        .fullscreen
                        .then_some(winit::window::Fullscreen::Borderless(None));
                    active.window.set_fullscreen(mode);
                }
                MenuAction::OpenSettings => active.shell.settings_open = true,
                MenuAction::Quit => event_loop.exit(),
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
}
