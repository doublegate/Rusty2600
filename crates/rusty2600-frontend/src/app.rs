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
        // `Arc<Mutex<EmuCore>>` is the right shape for the (default-off) dedicated emulation thread
        // + the present path. It is not yet `Send + Sync` only because `rusty2600-cart`'s `Board`
        // trait is not `Send` (the RustyNES `Mapper: Send` rule the cart phase will land); once it
        // is, the `emu-thread` default returns and this allow goes away. TODO(T-PS-063).
        #[allow(clippy::arc_with_non_send_sync)]
        let core = Arc::new(Mutex::new(emu));

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
    fn render(active: &mut Active) -> Vec<MenuAction> {
        // --- (1) Step one frame + copy the framebuffer + read-only info under a BRIEF lock. ---
        let (fb, fb_dims, info) = {
            let mut emu = active.core.lock().unwrap_or_else(PoisonError::into_inner);
            // Synchronous drive (the `emu-thread` feature is off by default): step exactly one
            // frame here, then copy out.
            emu.run_frame(Some(active.host_input));
            let dims = emu.fb_dims();
            let fb = emu.framebuffer().to_vec();
            let info = ShellInfo {
                board_tier: emu.board_tier().map(str::to_string),
                region: emu.region,
                fps: 0.0, // TODO(impl-phase): wire the pacer's smoothed FPS estimate.
                rom_loaded: emu.rom_loaded,
                cpu_info: format!("A: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PC: {:04X}, P: {:?}", emu.system.cpu.a, emu.system.cpu.x, emu.system.cpu.y, emu.system.cpu.s, emu.system.cpu.pc, emu.system.cpu.p),
                tia_info: format!("Color Clock: {}, Scanline: {}", emu.system.bus.tia.color_clock, emu.system.bus.tia.scanline),
                riot_info: format!("Pins: {:02X} {:02X}, DDR: {:02X} {:02X}, INTIM: {:02X}", emu.system.bus.riot.pins[0], emu.system.bus.riot.pins[1], emu.system.bus.riot.ddr[0], emu.system.bus.riot.ddr[1], emu.system.bus.riot.timer.value),
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
