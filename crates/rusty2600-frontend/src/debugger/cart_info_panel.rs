//! The cartridge/bankswitch info panel (`[v2.12.0]`).
//!
//! Displays the loaded ROM's bankswitch-scheme metadata: the short scheme
//! code (`rusty2600_cart::Cartridge::scheme_name`), the accuracy tier (the
//! `Tier` honesty marker — ADR 0003, `docs/cart.md`), and the raw ROM byte
//! size. This is deliberately NOT an iNES-style header editor — the 2600
//! cartridge format has no header at all (unlike NES `.nes` images), so
//! there is nothing to parse or edit here, only bankswitch-catalogue
//! metadata the frontend already computed at load time (see
//! `crate::emu_thread::EmuCore::{board_tier, board_scheme, rom_size}`).

/// Render the cart-info panel from already-fetched, side-effect-free
/// metadata (no emu-lock access here — the caller reads it once per frame
/// alongside every other `ShellInfo` field).
pub fn render_cart_info_panel(
    ui: &mut egui::Ui,
    scheme: Option<&str>,
    tier: Option<&str>,
    rom_size: Option<usize>,
) {
    let (Some(scheme), Some(tier), Some(rom_size)) = (scheme, tier, rom_size) else {
        ui.label("(no ROM loaded)");
        return;
    };
    egui::Grid::new("cart_info_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Scheme:");
            ui.monospace(scheme);
            ui.end_row();

            ui.label("Accuracy tier:");
            ui.monospace(tier);
            ui.end_row();

            ui.label("ROM size:");
            #[allow(clippy::cast_precision_loss)]
            ui.monospace(format!(
                "{rom_size} bytes ({:.1} KiB)",
                rom_size as f64 / 1024.0
            ));
            ui.end_row();
        });
    ui.separator();
    ui.weak(
        "The Atari 2600 cartridge format has no header (unlike NES .nes \
         images) — this panel shows bankswitch-catalogue metadata only.",
    );
}
