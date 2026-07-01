//! The player/missile/ball position panel.
//!
//! The 2600 analog of an NES OAM sprite grid, except the 2600 has exactly 5
//! fixed movable objects (no sprite table to browse), so this is a live
//! register-and-ruler view instead: each object's horizontal counter, its
//! `NUSIZx` copy spacing (P0/P1/M0/M1 only — the ball has none), reflect
//! (`REFPx`, P0/P1 only), and `HMxx` fine-adjust, drawn against a 160-dot
//! scanline ruler.

use super::TiaSnapshot;

const OBJECT_NAMES: [&str; 5] = ["P0", "P1", "M0", "M1", "BL"];

/// Decodes `NUSIZx`'s copy-count/spacing bits (0-2) into a short label,
/// per the TIA's documented encoding (`docs/tia.md`).
fn nusiz_copies_label(nusiz: u8) -> &'static str {
    match nusiz & 0x07 {
        0 => "one copy",
        1 => "two copies (close)",
        2 => "two copies (medium)",
        3 => "three copies (close)",
        4 => "two copies (wide)",
        5 => "double-size",
        6 => "three copies (medium)",
        7 => "quad-size",
        _ => unreachable!("masked to 3 bits"),
    }
}

/// Renders the live player/missile/ball state.
pub fn render_pmb_panel(ui: &mut egui::Ui, snap: &TiaSnapshot) {
    egui::Grid::new("pmb_registers")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.label("Object");
            ui.label("Position");
            ui.label("HM fine-adjust");
            ui.label("NUSIZ / REFP");
            ui.end_row();
            for (i, name) in OBJECT_NAMES.iter().enumerate() {
                ui.label(*name);
                ui.monospace(format!("{}", snap.pos[i]));
                ui.monospace(format!("{:+}", snap.hm[i]));
                let extra = if i < 2 {
                    format!(
                        "{} / reflect={}",
                        nusiz_copies_label(snap.nusiz[i]),
                        snap.refp[i]
                    )
                } else if i < 4 {
                    nusiz_copies_label(snap.nusiz[i % 2]).to_string()
                } else {
                    String::new()
                };
                ui.monospace(extra);
                ui.end_row();
            }
        });
    ui.separator();
    ui.label("Scanline ruler (160 visible color clocks):");
    render_ruler(ui, snap.pos);
}

/// A single-line ASCII ruler with each object's position marked by its
/// initial letter (later objects win a shared column — good enough for
/// "are these overlapping" at a glance, not a replacement for the exact
/// per-object rows above).
fn render_ruler(ui: &mut egui::Ui, pos: [u8; 5]) {
    let mut line = vec![b'.'; 160];
    for (i, &p) in pos.iter().enumerate() {
        let idx = usize::from(p).min(159);
        line[idx] = OBJECT_NAMES[i].as_bytes()[0];
    }
    ui.monospace(String::from_utf8_lossy(&line).into_owned());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nusiz_labels_cover_all_eight_encodings() {
        for n in 0..8u8 {
            // Must not panic (the `unreachable!` branch is truly unreachable).
            let _ = nusiz_copies_label(n);
        }
    }

    #[test]
    fn nusiz_masks_off_the_size_bits() {
        // Bits above the low 3 (e.g. the missile-width bits, 4-5) must not
        // change the copy-count decode.
        assert_eq!(
            nusiz_copies_label(0b0000_0000),
            nusiz_copies_label(0b0011_0000)
        );
    }
}
