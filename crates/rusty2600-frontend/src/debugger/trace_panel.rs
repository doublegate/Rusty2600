//! The instruction-execution trace logger panel (`[v2.12.0]`).
//!
//! A capped ring buffer of recently-stepped instructions, captured from
//! `crate::app`'s `MenuAction::DebugStep`/`DebugContinue` handlers — the
//! SAME call sites [`super::callstack::track_instruction`] already observes
//! `Step`/`Continue` from, following the identical "observe, don't
//! instrument the core" pattern: [`track_instruction`] here is a plain
//! function taking already-fetched primitives (PC, opcode, register file),
//! never touching `rusty2600_cpu`/`rusty2600_core` itself. The ring only
//! grows while [`TraceState::enabled`] is set (the panel's own "Record"
//! checkbox), so an unused/closed trace panel costs nothing beyond the one
//! boolean check per stepped instruction.

use std::collections::VecDeque;

/// The ring buffer's capacity. Generous enough to cover several frames'
/// worth of single-stepping without unbounded growth; `Export` still writes
/// the FULL current ring, not just the panel's visible tail.
const RING_CAPACITY: usize = 4096;

/// How many of the most recent records the live view renders (the ring may
/// hold far more; `Export` writes the whole ring).
const TAIL_ROWS: usize = 256;

/// One captured trace record: the 6507's register file immediately BEFORE
/// the instruction executed, plus its pre-formatted disassembly text.
#[derive(Debug, Clone)]
pub struct TraceEntry {
    /// The program counter the instruction executed at.
    pub pc: u16,
    /// Accumulator (before execution).
    pub a: u8,
    /// X index register (before execution).
    pub x: u8,
    /// Y index register (before execution).
    pub y: u8,
    /// Stack pointer (before execution).
    pub s: u8,
    /// The disassembled `"MNE operand"` text (see
    /// `super::disasm::disassemble_one`), pre-formatted at capture time so
    /// rendering never needs a fresh `Bus::peek`.
    pub text: String,
}

/// Persistent trace-panel state: whether recording is on, the captured
/// ring, and the last export result.
#[derive(Debug, Default, Clone)]
pub struct TraceState {
    /// Whether [`track_instruction`]'s caller should append new records.
    pub enabled: bool,
    /// The captured ring, oldest-first, capped at `RING_CAPACITY`.
    pub ring: VecDeque<TraceEntry>,
    /// The result of the last "Export…" click (the path written, or an
    /// error), shown under the toolbar.
    pub export_status: Option<String>,
}

/// Append one just-executed instruction's trace record.
///
/// Call this from `crate::app`'s `DebugStep`/`DebugContinue` handlers with
/// the SAME pre-fetched `(pc_before, opcode)` those already compute for
/// [`super::callstack::track_instruction`], plus the register file read
/// before stepping and `text` (the disassembly of `opcode` at `pc_before`).
/// No-ops when `state.enabled` is false — the cheap common case.
pub fn track_instruction(
    state: &mut TraceState,
    pc: u16,
    a: u8,
    x: u8,
    y: u8,
    s: u8,
    text: String,
) {
    if !state.enabled {
        return;
    }
    if state.ring.len() >= RING_CAPACITY {
        state.ring.pop_front();
    }
    state.ring.push_back(TraceEntry {
        pc,
        a,
        x,
        y,
        s,
        text,
    });
}

/// Format one record as a fixed-width trace line.
fn fmt_rec(r: &TraceEntry) -> String {
    format!(
        "${:04X}  A:{:02X} X:{:02X} Y:{:02X} S:{:02X}  {}",
        r.pc, r.a, r.x, r.y, r.s, r.text
    )
}

/// Render the trace panel: a Record toggle, Clear/Export controls, and the
/// most-recent `TAIL_ROWS` captured instructions.
pub fn render_trace_panel(ui: &mut egui::Ui, state: &mut TraceState) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.enabled, "Record");
        if ui.button("Clear").clicked() {
            state.ring.clear();
            state.export_status = None;
        }
        ui.label(format!("{} record(s)", state.ring.len()));
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button("Export…").clicked() {
            state.export_status = Some(export_trace(&state.ring));
        }
    });
    if let Some(s) = &state.export_status {
        ui.weak(s);
    }
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(300.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.ring.is_empty() {
                ui.weak("(no records — enable Record and Step/Continue)");
            }
            let skip = state.ring.len().saturating_sub(TAIL_ROWS);
            for r in state.ring.iter().skip(skip) {
                ui.monospace(fmt_rec(r));
            }
        });
}

/// Write the entire trace ring to `<temp>/rusty2600-trace.log`. Returns a
/// status string (the path on success, or the error).
#[cfg(not(target_arch = "wasm32"))]
fn export_trace(ring: &VecDeque<TraceEntry>) -> String {
    let mut out = String::with_capacity(ring.len() * 48);
    for r in ring {
        out.push_str(&fmt_rec(r));
        out.push('\n');
    }
    let path = std::env::temp_dir().join("rusty2600-trace.log");
    match std::fs::write(&path, out) {
        Ok(()) => format!("wrote {} record(s) to {}", ring.len(), path.display()),
        Err(e) => format!("export failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default_and_records_nothing() {
        let mut state = TraceState::default();
        track_instruction(&mut state, 0xF000, 0, 0, 0, 0xFF, "NOP".into());
        assert!(state.ring.is_empty());
    }

    #[test]
    fn enabled_records_instructions_in_order() {
        let mut state = TraceState {
            enabled: true,
            ..Default::default()
        };
        track_instruction(&mut state, 0xF000, 1, 2, 3, 0xFD, "LDA #$01".into());
        track_instruction(&mut state, 0xF002, 1, 2, 3, 0xFD, "NOP".into());
        assert_eq!(state.ring.len(), 2);
        assert_eq!(state.ring[0].pc, 0xF000);
        assert_eq!(state.ring[1].pc, 0xF002);
    }

    #[test]
    fn ring_caps_at_capacity_dropping_oldest() {
        let mut state = TraceState {
            enabled: true,
            ..Default::default()
        };
        for i in 0..RING_CAPACITY + 10 {
            #[allow(clippy::cast_possible_truncation)]
            track_instruction(&mut state, i as u16, 0, 0, 0, 0, "NOP".into());
        }
        assert_eq!(state.ring.len(), RING_CAPACITY);
        // The oldest 10 were evicted, so the first remaining record is #10.
        assert_eq!(state.ring[0].pc, 10);
    }
}
