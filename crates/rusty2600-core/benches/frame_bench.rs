//! Criterion bench for `rusty2600-core`: one full NTSC frame driven through
//! the WHOLE `System` (6507 + Bus: TIA + RIOT + cart), not a single chip in
//! isolation — the per-chip benches in `rusty2600-{cpu,tia,riot,cart}`
//! already cover those; this is the end-to-end proxy `docs/performance.md`
//! flagged as missing ("A true end-to-end frame bench ... is not yet part of
//! this suite").
#![allow(missing_docs)] // criterion_group!/main! expand into undocumented fns.

use criterion::{Criterion, criterion_group, criterion_main};
use rusty2600_core::{System, detect};
use std::hint::black_box;

/// One color clock per line: 228 (NTSC). One frame: 262 lines.
const COLOR_CLOCKS_PER_LINE: u64 = 228;
/// NTSC frame height (3 VSYNC + 37 VBLANK + 192 visible + 30 overscan).
const LINES_PER_FRAME: u64 = 262;
const COLOR_CLOCKS_PER_FRAME: u64 = COLOR_CLOCKS_PER_LINE * LINES_PER_FRAME;

/// A minimal, hand-assembled 4 KiB cartridge image (`Rom4K`, the plain
/// Atari-standard scheme — the simplest entry in the catalogue, so `detect()`
/// never has to disambiguate a hotspot signature) whose reset vector points
/// at a real NTSC kernel: 3 lines VSYNC, 37 lines VBLANK, 192 visible lines,
/// 30 lines overscan, looping forever — the canonical 262-line 2600 frame
/// shape, driven entirely by `WSYNC` beam-stalls (exercising the CPU decode
/// path, the TIA register-write path, AND the scheduler's RDY-stall path,
/// unlike a bare TIA-only bench).
///
/// This project cannot bundle a real commercial ROM (never committed, per
/// `.gitignore`), and the only 2600-cartridge-shaped images actually
/// committed under `tests/roms/test_suite/` at the time this bench was
/// written are two generic 6502 CPU conformance binaries (not a TIA-driving
/// kernel) — so a hand-built synthetic image is the right choice here, not a
/// workaround. See `docs/performance.md` for the full byte-by-byte layout.
fn make_ntsc_frame_rom() -> [u8; 0x1000] {
    let mut rom = [0u8; 0x1000];

    // Addresses below are conceptually $F000-based (the standard 4 KiB cart
    // window); only the low 12 bits matter to `Rom4K::cpu_read` (`addr &
    // 0x0FFF`), so array offsets equal `address & 0x0FFF` directly.
    let prog: &[u8] = &[
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        // -- MAINLOOP ($F005) --
        0xA9, 0x02, // LDA #$02          (VSYNC on)
        0x85, 0x00, // STA VSYNC
        0x85, 0x02, // STA WSYNC (line 1 of 3)
        0x85, 0x02, // STA WSYNC (line 2 of 3)
        0x85, 0x02, // STA WSYNC (line 3 of 3)
        0xA9, 0x00, // LDA #$00
        0x85, 0x00, // STA VSYNC        (VSYNC off)
        // -- 37 lines VBLANK --
        0xA2, 0x25, // LDX #37
        0x85, 0x02, // VBLANK_LOOP: STA WSYNC
        0xCA, // DEX
        0xD0, 0xFB, // BNE VBLANK_LOOP
        // -- 192 visible lines --
        0xA2, 0xC0, // LDX #192
        0x85, 0x02, // VISIBLE_LOOP: STA WSYNC
        0xCA, // DEX
        0xD0, 0xFB, // BNE VISIBLE_LOOP
        // -- 30 lines overscan --
        0xA2, 0x1E, // LDX #30
        0x85, 0x02, // OVERSCAN_LOOP: STA WSYNC
        0xCA, // DEX
        0xD0, 0xFB, // BNE OVERSCAN_LOOP
        // -- back to MAINLOOP ($F005) --
        0x4C, 0x05, 0xF0, // JMP $F005
    ];
    rom[0x000..prog.len()].copy_from_slice(prog);

    // Reset vector -> $F000 (offset 0x000). IRQ/BRK vector also pointed at
    // $F000 for safety, though `SEI` at the top means it's never taken.
    rom[0xFFC] = 0x00;
    rom[0xFFD] = 0xF0;
    rom[0xFFE] = 0x00;
    rom[0xFFF] = 0xF0;

    rom
}

fn bench_full_ntsc_frame(c: &mut Criterion) {
    let rom = make_ntsc_frame_rom();
    let board = detect(&rom).expect("synthetic 4 KiB image must detect as a known scheme");

    let mut system = System::new(0);
    system.bus.board = Some(board);
    system.reset();

    c.bench_function("system_full_ntsc_frame", |b| {
        b.iter(|| {
            let start = system.color_clocks();
            let target = start.wrapping_add(COLOR_CLOCKS_PER_FRAME);
            while system.color_clocks().wrapping_sub(start) < COLOR_CLOCKS_PER_FRAME {
                black_box(system.step_instruction());
            }
            black_box(target);
        });
    });
}

criterion_group!(benches, bench_full_ntsc_frame);
criterion_main!(benches);
