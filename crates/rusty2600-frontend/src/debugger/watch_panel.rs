//! The watch/conditional-breakpoint panel.
//!
//! A user-maintained list of [`super::expr`] expressions, each shown with
//! its live PASS/FAIL/ERROR result and an optional "break" toggle that arms
//! it as a conditional breakpoint (checked by `MenuAction::DebugContinue`'s
//! step loop via [`super::DebuggerState::any_breakpoint_watch_triggered`]).

use super::{DebugSnapshot, DebuggerState};

/// Renders the watch list against `snap`'s live [`super::expr::EvalContext`].
pub fn render_watch_panel(ui: &mut egui::Ui, snap: &DebugSnapshot, state: &mut DebuggerState) {
    ui.horizontal(|ui| {
        ui.label("Add watch:");
        ui.text_edit_singleline(&mut state.watch_input);
        if ui.button("Add").clicked() {
            state.commit_watch_input();
        }
    });
    ui.small("e.g. \"a == $42\", \"[$80] != 0\", \"scanline >= 192\"");
    ui.separator();

    let ctx = snap.eval_context();
    let mut to_remove = None;
    for (i, watch) in state.watches.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut watch.break_when_true, "break");
            match super::expr::evaluate(&watch.expr, &ctx) {
                Ok(true) => ui.colored_label(egui::Color32::GREEN, &watch.expr),
                Ok(false) => ui.colored_label(egui::Color32::GRAY, &watch.expr),
                Err(_) => ui.colored_label(egui::Color32::RED, format!("{} (error)", watch.expr)),
            };
            if ui.small_button("x").clicked() {
                to_remove = Some(i);
            }
        });
    }
    if let Some(i) = to_remove {
        state.watches.remove(i);
    }
}
