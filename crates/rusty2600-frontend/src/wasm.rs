//! The wasm32 entry point (`#[wasm_bindgen(start)]`).
//!
//! A canvas-2D WebAssembly bootstrap: `requestAnimationFrame`-driven emulation
//! with real keyboard input, reusing this crate's own [`crate::input`] model
//! (the same [`crate::input::KeyBindings`]/[`crate::input::InputState`] the
//! native build uses) rather than a separate wasm-only input scheme. Browser
//! `KeyboardEvent.code` values (`"ArrowUp"`, `"KeyZ"`, `"F1"`, ...) are the
//! same physical-key naming convention winit's `KeyCode` Debug format uses,
//! so [`crate::input::KeyBindings::action_for`] resolves them directly with
//! no translation layer.
//!
//! This is the ONE wasm entry point that exists today — the `wasm-winit`/
//! `wasm-canvas` Cargo features are currently identical empty placeholders
//! (see `Cargo.toml`'s `[features]` doc comment); a real winit + wgpu + egui
//! browser build (matching the native binary, the way the `wasm-winit` name
//! implies) is future work, not attempted here.

use std::cell::RefCell;
use std::rc::Rc;

use crate::input::{InputState, KeyBindings};
use rusty2600_cart::detect;
use rusty2600_core::System;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
#[allow(unused_imports)]
use web_sys::*;

const ATARI_W: u32 = 160;
const ATARI_H: u32 = 192; // NTSC active picture height (post-VBLANK crop).

/// NTSC scanlines before the active picture starts (VSYNC + VBLANK) — the
/// same crop `emu_thread::run_frame` applies natively. The TIA's raw
/// `video_buffer` starts at scanline 0 (VSYNC), not the first visible line;
/// skipping this offset was a real bug (fixed alongside real audio/keyboard
/// input): the canvas was showing the top ~40 non-picture scanlines and
/// cutting off the bottom of the actual picture.
const NTSC_VBLANK_LINES: usize = 37;

/// The TIA's audio sample rate: two samples pushed per scanline (color
/// clocks 114 and 227 of each 228-color-clock line), so one sample every
/// 114 color clocks of the 3.579545 MHz NTSC dot clock.
const AUDIO_SAMPLE_RATE: f32 = 3_579_545.0 / 114.0;

struct Emu {
    system: Option<System>,
    input: InputState,
    key_bindings: KeyBindings,
    /// 1-pole DC-blocker state, matching `emu_thread`'s native audio path
    /// exactly (same `r = 0.995` coefficient) so the browser's audio isn't
    /// dominated by the TIA's large silence-level DC offset.
    dc_blocker_x: f32,
    dc_blocker_y: f32,
    audio: Option<AudioSink>,
}

/// A minimal Web Audio buffered-playback sink: each frame's drained samples
/// become one `AudioBuffer`, scheduled back-to-back via `AudioBufferSourceNode`
/// (the standard gapless-queue pattern for streaming synthesized audio,
/// avoiding the extra complexity of an `AudioWorklet` module for this small
/// canvas-2D bootstrap). `AudioContext` requires a user gesture to start, so
/// this is only constructed once the user picks a ROM file.
struct AudioSink {
    ctx: AudioContext,
    /// The `AudioContext.currentTime` at which the next scheduled buffer
    /// should start — kept running just ahead of playback so buffers queue
    /// gaplessly instead of restarting the clock (and audibly clicking)
    /// every frame.
    next_start: f64,
}

impl AudioSink {
    fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        let next_start = ctx.current_time();
        Ok(Self { ctx, next_start })
    }

    /// Schedule one frame's worth of already-normalized samples for gapless
    /// playback, resyncing to the context clock if playback has fallen more
    /// than a fraction of a second behind (e.g. the tab was backgrounded).
    // `samples` isn't mutated by this fn's own body — it's `&mut` only
    // because `AudioBuffer::copy_to_channel`'s web-sys binding requires it
    // (the underlying JS call reads, not writes, the slice).
    #[allow(clippy::needless_pass_by_ref_mut)]
    fn push_samples(&mut self, samples: &mut [f32]) -> Result<(), JsValue> {
        if samples.is_empty() {
            return Ok(());
        }
        let now = self.ctx.current_time();
        if self.next_start < now - 0.25 {
            self.next_start = now;
        }

        // A single frame's audio sample count (a few hundred at ~31.4 kHz /
        // 60 fps) never approaches `u32::MAX`.
        #[allow(clippy::cast_possible_truncation)]
        let len = samples.len() as u32;

        let buffer = self.ctx.create_buffer(1, len, AUDIO_SAMPLE_RATE)?;
        buffer.copy_to_channel(samples, 0)?;

        let src = self.ctx.create_buffer_source()?;
        src.set_buffer(Some(&buffer));
        src.connect_with_audio_node(&self.ctx.destination())?;
        let start = self.next_start.max(now);
        src.start_with_when(start)?;
        self.next_start = start + f64::from(len) / f64::from(AUDIO_SAMPLE_RATE);
        Ok(())
    }
}

thread_local! {
    static EMU: Rc<RefCell<Emu>> = Rc::new(RefCell::new(Emu {
        system: None,
        input: InputState::default(),
        key_bindings: KeyBindings::default(),
        dc_blocker_x: 0.0,
        dc_blocker_y: 0.0,
        audio: None,
    }));
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"Rusty2600 wasm32 — boot".into());

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    let canvas: HtmlCanvasElement = document
        .get_element_by_id("atari-canvas")
        .ok_or("missing <canvas id=\"atari-canvas\">")?
        .dyn_into()?;
    canvas.set_width(ATARI_W);
    canvas.set_height(ATARI_H);

    let rom_input: HtmlInputElement = document
        .get_element_by_id("rom-input")
        .ok_or("missing <input id=\"rom-input\">")?
        .dyn_into()?;

    install_rom_loader(&rom_input);
    install_keyboard_input(&window)?;
    start_raf_loop(&canvas)?;

    web_sys::console::log_1(&"Rusty2600 wasm32 — armed. Load a .bin ROM to begin.".into());
    Ok(())
}

/// Drive a console switch from JS (the on-screen Reset/Select/Color/
/// Difficulty buttons in `index.html`).
///
/// Reuses the exact same [`InputState::apply_action`] path the keyboard
/// bindings use — `name` is one of `"select"`/`"reset"`/`"color"`/
/// `"left_difficulty"`/`"right_difficulty"`; unrecognized names are a
/// silent no-op. Momentary switches (select/reset) should be called with
/// `pressed = true` on pointer-down and `false` on pointer-up; the latching
/// toggles (color/difficulty) only need a single `pressed = true` call per
/// click, matching `apply_action`'s press-edge-only toggle semantics.
#[wasm_bindgen]
pub fn set_console_switch(name: &str, pressed: bool) {
    use crate::input::InputAction;
    let Some(action) = (match name {
        "select" => Some(InputAction::SwitchSelect),
        "reset" => Some(InputAction::SwitchReset),
        "color" => Some(InputAction::SwitchColor),
        "left_difficulty" => Some(InputAction::SwitchLeftDifficulty),
        "right_difficulty" => Some(InputAction::SwitchRightDifficulty),
        _ => None,
    }) else {
        return;
    };
    EMU.with(|emu| emu.borrow_mut().input.apply_action(action, pressed));
}

fn install_rom_loader(rom_input: &HtmlInputElement) {
    let on_change = Closure::<dyn FnMut(Event)>::new(move |ev: Event| {
        // Construct (or resume) the `AudioContext` synchronously, right here
        // in the file-input's `change` handler — the browser autoplay policy
        // requires audio to start from a real user gesture, and that
        // gesture window can close by the time the async `FileReader`
        // callback below fires.
        EMU.with(|emu| {
            let mut emu = emu.borrow_mut();
            if emu.audio.is_none() {
                match AudioSink::new() {
                    Ok(sink) => emu.audio = Some(sink),
                    Err(e) => web_sys::console::warn_2(&"Rusty2600: audio init failed".into(), &e),
                }
            }
        });

        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };

        let Ok(reader) = FileReader::new() else {
            return;
        };
        let reader_clone = reader.clone();
        let on_load = Closure::<dyn FnMut()>::new(move || {
            let Ok(buffer) = reader_clone.result() else {
                return;
            };
            let array = js_sys::Uint8Array::new(&buffer);
            let bytes = array.to_vec();

            if let Some(board) = detect(&bytes) {
                let mut system = System::new(0);
                system.bus.board = Some(board);
                EMU.with(|emu| emu.borrow_mut().system = Some(system));
                web_sys::console::log_1(
                    &format!("ROM loaded ({} bytes). Running.", bytes.len()).into(),
                );
            } else {
                web_sys::console::log_1(&"ROM parse error".into());
            }
        });
        reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
        on_load.forget();
        let _ = reader.read_as_array_buffer(&file);
    });
    rom_input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
}

/// Wires `keydown`/`keyup` on `window` into [`InputState::apply_action`] via
/// the shared [`KeyBindings`] table — the SAME key layout the native build
/// documents (arrows/`Z`/`Space` for P1, `WASD`/`Q` for P2, `F1`-`F5` for the
/// console switches). A bound key's default browser action (e.g. arrow-key
/// page scroll, `F1` opening help) is suppressed; unbound keys pass through
/// untouched so devtools shortcuts etc. keep working.
fn install_keyboard_input(window: &Window) -> Result<(), JsValue> {
    let on_key = |pressed: bool| {
        Closure::<dyn FnMut(KeyboardEvent)>::new(move |ev: KeyboardEvent| {
            let code = ev.code();
            EMU.with(|emu| {
                let mut emu = emu.borrow_mut();
                if let Some(action) = emu.key_bindings.action_for(&code) {
                    emu.input.apply_action(action, pressed);
                    ev.prevent_default();
                }
            });
        })
    };

    let on_keydown = on_key(true);
    window.add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())?;
    on_keydown.forget();

    let on_keyup = on_key(false);
    window.add_event_listener_with_callback("keyup", on_keyup.as_ref().unchecked_ref())?;
    on_keyup.forget();

    Ok(())
}

fn start_raf_loop(canvas: &HtmlCanvasElement) -> Result<(), JsValue> {
    let ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        EMU.with(|emu| {
            let mut emu = emu.borrow_mut();
            let input = emu.input;
            // Copied out (not accessed through `emu.` directly) so the
            // borrow checker sees them as disjoint from the `emu.system`
            // borrow below — `RefMut`'s `DerefMut` indirection means Rust
            // can't split-borrow `emu`'s fields the way it could a plain
            // struct, so this mirrors the existing `let input = emu.input;`
            // copy-out above.
            let mut dc_x = emu.dc_blocker_x;
            let mut dc_y = emu.dc_blocker_y;
            let mut pending_audio: Vec<f32> = Vec::new();
            if let Some(ref mut system) = emu.system {
                // Late-latch the current input state into the RIOT/TIA ports,
                // same convention the native `emu_thread::run_frame` uses
                // (`InputState::riot_ports`/`fire_inputs`).
                let (swcha, swchb) = input.riot_ports();
                system.bus.riot.pins[0] = swcha;
                system.bus.riot.pins[1] = swchb;
                let (inpt4, inpt5) = input.fire_inputs();
                system.bus.tia.inpt[4] = inpt4;
                system.bus.tia.inpt[5] = inpt5;

                // Drive one frame by running instructions until the VSYNC 1->0
                // edge (the CPU drives its own ticking now — see
                // `rusty2600-core::scheduler`'s module doc comment; this can no
                // longer sample one dot per color clock from a fixed-iteration
                // loop, so it reads the TIA's own accumulated video buffer
                // once the frame boundary is reached).
                let mut old_vsync = system.bus.tia.objects.vsync;
                for _ in 0..200_000u32 {
                    system.step_instruction();
                    let vsync = system.bus.tia.objects.vsync;
                    if (old_vsync & 0x02 != 0) && (vsync & 0x02 == 0) {
                        break;
                    }
                    old_vsync = vsync;
                }

                let mut framebuffer = vec![0u8; (ATARI_W * ATARI_H * 4) as usize];
                let video = &system.bus.tia.video_buffer;
                for y in 0..ATARI_H as usize {
                    // Skip VSYNC+VBLANK (the raw `video_buffer` starts at
                    // scanline 0, not the first visible line) — same crop
                    // `emu_thread::run_frame` applies natively. Without this,
                    // the canvas showed the top ~40 non-picture scanlines and
                    // cut off the bottom of the real picture.
                    let sl = y + NTSC_VBLANK_LINES;
                    for x in 0..ATARI_W as usize {
                        let src = sl * 160 + x;
                        let color_idx = video.get(src).copied().unwrap_or(0);
                        let rgb = crate::palette::Region::Ntsc.table()[(color_idx >> 1) as usize];
                        let off = (y * ATARI_W as usize + x) * 4;
                        if off + 3 < framebuffer.len() {
                            framebuffer[off] = (rgb >> 16) as u8;
                            framebuffer[off + 1] = (rgb >> 8) as u8;
                            framebuffer[off + 2] = rgb as u8;
                            framebuffer[off + 3] = 255;
                        }
                    }
                }

                // Drain this frame's audio samples (DC-blocked + normalized),
                // matching `emu_thread`'s native audio path exactly. Pushed to
                // the Web Audio sink AFTER this `system` borrow ends (below),
                // since the sink lives in a different `emu` field.
                let samples = core::mem::take(&mut system.bus.tia.audio_buffer);
                pending_audio.reserve(samples.len());
                for s in samples {
                    let normalized = (f32::from(s) / 15.0) - 1.0;
                    let r = 0.995;
                    let y = normalized - dc_x + r * dc_y;
                    dc_x = normalized;
                    dc_y = y;
                    pending_audio.push(y);
                }

                let clamped = wasm_bindgen::Clamped(framebuffer.as_slice());
                if let Ok(image_data) =
                    ImageData::new_with_u8_clamped_array_and_sh(clamped, ATARI_W, ATARI_H)
                {
                    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
                }
            }
            emu.dc_blocker_x = dc_x;
            emu.dc_blocker_y = dc_y;
            if !pending_audio.is_empty()
                && let Some(sink) = emu.audio.as_mut()
                && let Err(e) = sink.push_samples(&mut pending_audio)
            {
                web_sys::console::warn_2(&"Rusty2600: audio push failed".into(), &e);
            }
        });

        // Request next frame
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}
