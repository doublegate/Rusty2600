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
const ATARI_H: u32 = 192; // Typical NTSC height

struct Emu {
    system: Option<System>,
    input: InputState,
    key_bindings: KeyBindings,
}

thread_local! {
    static EMU: Rc<RefCell<Emu>> = Rc::new(RefCell::new(Emu {
        system: None,
        input: InputState::default(),
        key_bindings: KeyBindings::default(),
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
                    for x in 0..ATARI_W as usize {
                        let src = y * 160 + x;
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

                let clamped = wasm_bindgen::Clamped(framebuffer.as_slice());
                if let Ok(image_data) =
                    ImageData::new_with_u8_clamped_array_and_sh(clamped, ATARI_W, ATARI_H)
                {
                    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
                }
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
