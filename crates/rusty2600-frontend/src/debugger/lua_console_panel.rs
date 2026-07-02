//! The Lua script console panel.
//!
//! Shows a loaded script's captured `print()` output and runtime errors
//! (`crate::scripting::ScriptState::log`, `rusty2600_script::ScriptLog`).
//!
//! Output-only: this is NOT an interactive Lua REPL. Executing arbitrary
//! ad-hoc Lua from the debugger would need to respect the same
//! `WritesLocked` determinism gate the normal `onFrame` tick already
//! enforces (see `crate::scripting`'s module doc) — real additional design
//! work deliberately out of scope for this panel.

use rusty2600_script::LogLine;

use crate::scripting::ScriptState;

/// Renders the Lua console panel.
///
/// Shows a "no script loaded" placeholder if `script` is `None` (mirrors
/// `cheevos_panel`'s "not logged in" state), otherwise the captured log
/// (oldest-first, errors in red) plus a Clear button.
pub fn render_lua_console_panel(ui: &mut egui::Ui, script: Option<&ScriptState>) {
    let Some(script) = script else {
        ui.label("(no script loaded — Tools -> Load Script...)");
        return;
    };

    let log = script.log();
    ui.horizontal(|ui| {
        ui.label(format!("{} line(s)", log.borrow().lines().len()));
        if ui.button("Clear").clicked() {
            log.borrow_mut().clear();
        }
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let log = log.borrow();
            if log.lines().is_empty() {
                ui.weak("(no output yet — a script's print()/errors show up here)");
            }
            for line in log.lines() {
                render_line(ui, line);
            }
        });
}

fn render_line(ui: &mut egui::Ui, line: &LogLine) {
    if line.is_error() {
        ui.colored_label(egui::Color32::RED, line.text());
    } else {
        ui.monospace(line.text());
    }
}

// No `#[cfg(test)] mod tests` here: `render_lua_console_panel` needs a live
// `egui::Ui`, which needs a full `egui::Context`/frame — not worth standing
// up (no sibling panel here does UI-level testing either; see
// `pmb_panel.rs`/`event_panel.rs`, whose own tests only cover pure helper
// functions, which this panel has none of). The real coverage for this
// feature is `rusty2600-script/src/engine.rs`'s print/error-capture tests
// (the data this panel renders).
