//! The wasm32 entry point (`#[wasm_bindgen(start)]`).
//!
//! v0.1.0: Canvas-based WebAssembly bootstrap mapping `requestAnimationFrame`
//! and the Web Audio API.

use std::cell::RefCell;
use std::rc::Rc;

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
}

thread_local! {
    static EMU: Rc<RefCell<Emu>> = Rc::new(RefCell::new(Emu {
        system: None,
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
    start_raf_loop(&canvas)?;

    web_sys::console::log_1(&"Rusty2600 wasm32 — armed. Load a .bin ROM to begin.".into());
    Ok(())
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
            if let Some(ref mut system) = emu.system {
                // Drive one frame
                let clocks = 262 * 228;
                let mut framebuffer = vec![0u8; (ATARI_W * ATARI_H * 4) as usize];

                for _ in 0..clocks {
                    system.tick_one_color_clock();

                    let cc = system.bus.tia.color_clock;
                    let sl = system.bus.tia.scanline;

                    // Visible lines 68..=227, only up to active_height (192)
                    if cc >= 68 && sl < ATARI_H as u16 {
                        let x = (cc - 68) as usize;
                        let y = sl as usize;
                        let color_idx = system.bus.tia.current_color;
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
                system.bus.tia.scanline = 0;

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
