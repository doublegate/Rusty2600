//! End-to-end proof-of-mechanism for the `hd-pack` live rendering splice
//! (v2.7.0 "True Colors"): loads a tiny synthetic 2600 ROM that lights player
//! 0 with a known, static `GRP0`/`NUSIZ0` pair, drives one frame with a
//! matching HD-pack replacement bitmap loaded, and asserts the produced
//! frontend pixel buffer actually shows the replacement bitmap's color, not
//! the flat resolved TIA color -- with a control run (same ROM, no pack
//! loaded) confirming the placeholder color never appears without the
//! splice active.
//!
//! No committed test-suite ROM has simple, known, static player graphics to
//! key off (`tests/roms/test_suite/` holds only 6502 CPU conformance
//! tests) -- this hand-assembled fixture stands in, giving an exact, fully
//! deterministic `(GRP0, NUSIZ0)` pair to key the replacement pack to,
//! instead of reverse-engineering a homebrew game's sprite data.
#![cfg(all(not(target_arch = "wasm32"), feature = "hd-pack"))]

use rusty2600_frontend::emu_thread::EmuCore;
use rusty2600_frontend::present_buffer;
use rusty2600_frontend::sprite_pack::SpritePack;

/// A minimal 4 KiB (`Rom4K`) cartridge image: on power-on, lights player 0
/// (`GRP0 = 0xFF`, `NUSIZ0 = 0`, `COLUP0 = 0x0E`), strobes `RESP0` once to
/// position it, then loops forever.
fn synthetic_player0_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x1000];
    // Program, placed at image offset 0x000 (CPU address $1000, the base of
    // the cart's address window; `Rom4K::cpu_read` maps `addr & 0x0FFF`).
    let program: &[u8] = &[
        0xA9, 0xFF, // LDA #$FF
        0x85, 0x1B, // STA $1B      ; GRP0 = 0xFF
        0xA9, 0x0E, // LDA #$0E
        0x85, 0x06, // STA $06      ; COLUP0 = 0x0E
        0xA9, 0x00, // LDA #$00
        0x85, 0x04, // STA $04      ; NUSIZ0 = 0 (one copy, normal size)
        0x85, 0x10, // STA $10      ; RESP0 strobe
        0x4C, 0x0E, 0x10, // loop: JMP $100E (self)
    ];
    rom[..program.len()].copy_from_slice(program);
    // Reset + IRQ vectors -> $1000 (image offset 0xFFC..=0xFFF), little-endian.
    rom[0x0FFC] = 0x00;
    rom[0x0FFD] = 0x10;
    rom[0x0FFE] = 0x00;
    rom[0x0FFF] = 0x10;
    rom
}

fn load_demo_pack() -> SpritePack {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hd_pack_demo");
    SpritePack::load(&dir).expect("load hd_pack_demo fixture")
}

const MAGENTA: [u8; 4] = [0xFF, 0x00, 0xFF, 0xFF];

#[test]
fn hd_pack_splice_replaces_player0_footprint_with_replacement_bitmap() {
    let rom = synthetic_player0_rom();

    // Run WITH the matching HD-pack loaded.
    let mut emu = EmuCore::new(0);
    emu.load_rom(&rom).expect("load synthetic ROM");
    emu.set_sprite_pack(Some(load_demo_pack()));
    let (tx, rx) = present_buffer::channel();
    emu.step_frame(&tx, None);
    let spliced = rx.take().expect("frame published");

    let spliced_has_magenta = spliced.pixels.chunks_exact(4).any(|p| p == MAGENTA);
    assert!(
        spliced_has_magenta,
        "expected the HD-pack replacement magenta pixel to appear in player 0's footprint"
    );

    // Control: identical ROM/seed/input, no pack loaded -- the placeholder
    // color must never appear without the splice active (it isn't a real
    // NTSC palette entry the TIA could produce on its own).
    let mut control = EmuCore::new(0);
    control.load_rom(&rom).expect("load synthetic ROM");
    let (ctrl_tx, ctrl_rx) = present_buffer::channel();
    control.step_frame(&ctrl_tx, None);
    let unspliced = ctrl_rx.take().expect("control frame published");

    let control_has_magenta = unspliced.pixels.chunks_exact(4).any(|p| p == MAGENTA);
    assert!(
        !control_has_magenta,
        "the placeholder magenta must never appear without the HD-pack splice active"
    );
}
