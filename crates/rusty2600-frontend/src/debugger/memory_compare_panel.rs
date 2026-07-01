//! Memory-compare panel: a byte-by-byte diff between a captured baseline
//! and the live memory view.
//!
//! Address-space-agnostic and generic — works against any two equal-length
//! byte snapshots (e.g. two `DebugSnapshot::riot_ram` captures).

/// One differing byte: `(offset, baseline value, current value)`.
pub type Diff = (usize, u8, u8);

/// Diffs `baseline` against `current`, byte by byte, stopping at the
/// shorter of the two lengths.
#[must_use]
pub fn diff(baseline: &[u8], current: &[u8]) -> Vec<Diff> {
    baseline
        .iter()
        .zip(current.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (&a, &b))| (i, a, b))
        .collect()
}

/// Renders the diff between `baseline` (if captured) and `current`, plus a
/// "Capture baseline" button that stores `current` into `*baseline`.
pub fn render_memory_compare_panel(
    ui: &mut egui::Ui,
    baseline: &mut Option<Vec<u8>>,
    current: &[u8],
) {
    ui.horizontal(|ui| {
        if ui.button("Capture baseline").clicked() {
            *baseline = Some(current.to_vec());
        }
        if ui.button("Clear baseline").clicked() {
            *baseline = None;
        }
    });
    ui.separator();

    let Some(base) = baseline else {
        ui.label("(no baseline captured yet)");
        return;
    };

    if base.len() != current.len() {
        ui.label(format!(
            "Comparing snapshots of different lengths ({} vs {}) — showing overlap only.",
            base.len(),
            current.len()
        ));
    }

    let diffs = diff(base, current);
    if diffs.is_empty() {
        ui.label("No differences from baseline.");
        return;
    }

    ui.label(format!("{} differing byte(s):", diffs.len()));
    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            egui::Grid::new("memory_compare_diff")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Offset");
                    ui.label("Baseline");
                    ui.label("Current");
                    ui.end_row();
                    for (offset, a, b) in &diffs {
                        ui.monospace(format!("${offset:04X}"));
                        ui.monospace(format!("${a:02X}"));
                        ui.monospace(format!("${b:02X}"));
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_only_the_changed_bytes() {
        let a = [1, 2, 3, 4];
        let b = [1, 9, 3, 8];
        let d = diff(&a, &b);
        assert_eq!(d, vec![(1, 2, 9), (3, 4, 8)]);
    }

    #[test]
    fn identical_slices_diff_to_nothing() {
        let a = [1, 2, 3];
        assert!(diff(&a, &a).is_empty());
    }

    #[test]
    fn mismatched_lengths_compare_only_the_overlap() {
        let a = [1, 2, 3];
        let b = [1, 9];
        assert_eq!(diff(&a, &b), vec![(1, 2, 9)]);
    }
}
