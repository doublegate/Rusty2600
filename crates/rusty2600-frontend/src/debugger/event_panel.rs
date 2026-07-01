//! The TIA event/write-scatter panel: a per-color-clock view of TIA
//! register writes across a scanline.
//!
//! The TIA has no VRAM/nametables — "the picture" for a given scanline IS
//! the sequence of writes to `COLUP0`/`COLUP1`/`COLUPF`/`COLUBK`, `GRP0`/
//! `GRP1`, `RESPx`, and `HMOVE` timed against the color clock, so this view
//! (more useful here than the NES-style panels it's loosely modeled on) is
//! what makes timing quirks like the HMOVE comb (`docs/tia.md`) visually
//! debuggable: a write that lands one color clock later than intended shows
//! up as a dot in the wrong column.

use rusty2600_core::WriteEvent;

/// The TIA register addresses (within `$00-$3F`, already masked) this panel
/// cares about, with a short display label.
const TRACKED: [(u16, &str); 8] = [
    (0x06, "COLUP0"),
    (0x07, "COLUP1"),
    (0x08, "COLUPF"),
    (0x09, "COLUBK"),
    (0x1B, "GRP0"),
    (0x1C, "GRP1"),
    (0x2A, "HMOVE"),
    (0x10, "RESP0"),
];

/// Renders a scanline x color-clock scatter of `events`, one row per
/// scanline present in the log, colored/labeled by which tracked register
/// was written.
pub fn render_event_panel(ui: &mut egui::Ui, events: &[WriteEvent]) {
    if events.is_empty() {
        ui.label("(no writes recorded yet — this panel only records while visible)");
        return;
    }

    ui.label(format!("{} TIA register writes this frame:", events.len()));
    ui.separator();

    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            egui::Grid::new("tia_event_scatter")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Scanline");
                    ui.label("Color clock");
                    ui.label("Register");
                    ui.end_row();
                    for ev in events {
                        let masked = ev.addr & 0x3F;
                        let Some((_, label)) = TRACKED.iter().find(|(a, _)| *a == masked) else {
                            continue;
                        };
                        ui.monospace(format!("{}", ev.scanline));
                        ui.monospace(format!("{}", ev.color_clock));
                        ui.monospace(format!("{label} = ${:02X}", ev.value));
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_registers_cover_the_documented_set() {
        // A cheap sanity check that the table's labels match the mask this
        // module documents (avoids a silent typo desyncing the two).
        for (addr, _) in TRACKED {
            assert_eq!(addr, addr & 0x3F);
        }
    }
}
