//! Address access-counter / heatmap.
//!
//! A per-address write-count tally built from the existing per-write log
//! (`rusty2600_core::WriteEvent`, the same data `event_panel` renders as a
//! scatter) — address-space-agnostic and generic, unlike `event_panel`'s
//! TIA-register-specific labeling.

use std::collections::BTreeMap;

use rusty2600_core::WriteEvent;

/// Tally write counts per address across `events`.
#[must_use]
pub fn tally_writes(events: &[WriteEvent]) -> BTreeMap<u16, u32> {
    let mut counts = BTreeMap::new();
    for ev in events {
        *counts.entry(ev.addr).or_insert(0u32) += 1;
    }
    counts
}

/// Renders a simple address -> write-count table, sorted by address.
pub fn render_access_counter_panel(ui: &mut egui::Ui, events: &[WriteEvent]) {
    let counts = tally_writes(events);
    if counts.is_empty() {
        ui.label("(no writes recorded yet — this panel only records while visible)");
        return;
    }

    ui.label(format!(
        "{} distinct address(es) written this frame:",
        counts.len()
    ));
    ui.separator();

    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            egui::Grid::new("access_counter_heatmap")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Address");
                    ui.label("Write count");
                    ui.end_row();
                    for (addr, count) in &counts {
                        ui.monospace(format!("${addr:04X}"));
                        ui.monospace(format!("{count}"));
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(addr: u16) -> WriteEvent {
        WriteEvent {
            scanline: 0,
            color_clock: 0,
            addr,
            value: 0,
        }
    }

    #[test]
    fn tally_counts_repeats_at_the_same_address() {
        let events = [ev(0x80), ev(0x80), ev(0x81)];
        let counts = tally_writes(&events);
        assert_eq!(counts[&0x80], 2);
        assert_eq!(counts[&0x81], 1);
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn empty_log_yields_an_empty_tally() {
        assert!(tally_writes(&[]).is_empty());
    }
}
