//! Diagnostic-only tool: run a ROM headlessly for N frames and dump selected
//! frames as PPM (P6) images for visual/pixel-diff inspection. Not part of the
//! shipped product surface — used while debugging TIA video output.
//!
//! Usage: cargo run --example `dump_frame` -- <rom-path> <num-frames> <out-dir> [dump-from]
//!
//! Native-only: uses `rusty2600_frontend::EmuCore` (gated `not(target_arch = "wasm32")` in
//! `lib.rs`) plus plain `std::fs`/CLI-arg I/O that has no wasm32 use case anyway — this is a
//! developer-only headless diagnostic tool, never shipped to the browser build. `main` below is
//! `not(target_arch = "wasm32")`-gated with an empty wasm32 stub (matching `src/main.rs`'s own
//! established convention for the real binary) rather than a crate-level `#![cfg]`, since an
//! example target still needs SOME `main` fn to exist for cargo to build it on every target. First
//! surfaced by verifying `cargo clippy --target wasm32-unknown-unknown --all-targets` (`[v2.8.0]`;
//! this exact invocation had not been run before this release).

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
use rusty2600_frontend::EmuCore;
#[cfg(not(target_arch = "wasm32"))]
use rusty2600_frontend::input::InputState;
#[cfg(not(target_arch = "wasm32"))]
use std::env;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;

#[cfg(not(target_arch = "wasm32"))]
fn write_ppm(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let mut f = fs::File::create(path).expect("create ppm");
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[0..3]);
    }
    f.write_all(&rgb).unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<String> = env::args().collect();
    let rom_path = args
        .get(1)
        .expect("usage: dump_frame <rom> <frames> <outdir>");
    let num_frames: usize = args.get(2).map_or(60, |s| s.parse().unwrap());
    let out_dir = args.get(3).map_or("/tmp/frames", String::as_str);
    let dump_from = args.get(4).map_or(usize::MAX, |s| s.parse().unwrap());
    fs::create_dir_all(out_dir).unwrap();

    let rom = fs::read(rom_path).expect("read rom");
    let mut core = EmuCore::new(0);
    core.load_rom(&rom).expect("load rom");

    let (w, h) = core.fb_dims();
    println!("fb dims: {w}x{h}");

    let mut prev_clocks = core.system.color_clocks();
    for i in 0..num_frames {
        // Hold Reset for the first 10 frames to start the game (skip the title screen).
        let mut input = InputState::default();
        input.switches.reset = i < 10;
        core.run_frame(Some(input));
        let clocks = core.system.color_clocks();
        let delta = clocks - prev_clocks;
        prev_clocks = clocks;
        if i >= dump_from || i + 1 >= num_frames.saturating_sub(5) || i % 20 == 0 {
            let path = format!("{out_dir}/frame_{i:04}.ppm");
            write_ppm(&path, w, h, core.framebuffer());
            let pos = core.system.bus.tia.objects.pos;
            let colu = core.system.bus.tia.objects.colu;
            println!(
                "wrote {path}  clocks_this_frame={delta} pos(p0,p1,m0,m1,bl)={pos:?} colu(p0,p1,pf,bk)={colu:?}"
            );
        } else if delta != 262 * 228 {
            println!(
                "frame {i}: clocks_this_frame={delta} (expected {})",
                262 * 228
            );
        }
    }
}
