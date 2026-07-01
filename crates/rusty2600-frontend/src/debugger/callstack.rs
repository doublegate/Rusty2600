//! The live JSR/RTS call stack panel.
//!
//! Unlike the CPU's own hardware stack (the `S` register indexing into
//! whatever RIOT RAM mirror is mapped at `$0100-$01FF` — the 2600 has no
//! dedicated stack RAM), this is a debugger-tracked call TREE: [`track_instruction`]
//! is called once per stepped instruction from `crate::app`'s Step/Continue
//! handlers, watching for `JSR` ($20, push the 3-byte instruction's return
//! address) and `RTS` ($60, pop) so the panel can show real call depth and
//! return addresses, not just a raw stack-pointer dump.

/// The `JSR abs` opcode (3 bytes: opcode + a 16-bit target).
const JSR: u8 = 0x20;
/// The `RTS` opcode (1 byte).
const RTS: u8 = 0x60;

/// Updates `call_stack` for one just-executed instruction.
///
/// Call this with the opcode byte and PC observed BEFORE stepping (a
/// side-effect-free `Bus::peek` at the old PC), immediately before
/// `System::step_instruction`.
pub fn track_instruction(call_stack: &mut Vec<u16>, opcode: u8, pc_before: u16) {
    match opcode {
        JSR => call_stack.push(pc_before.wrapping_add(3)),
        RTS => {
            call_stack.pop();
        }
        _ => {}
    }
}

/// Renders the call stack, deepest call last (top of the visual list).
pub fn render_callstack_panel(ui: &mut egui::Ui, call_stack: &[u16]) {
    if call_stack.is_empty() {
        ui.label("(empty — call a subroutine to populate)");
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            for (depth, addr) in call_stack.iter().rev().enumerate() {
                ui.monospace(format!("{:>3}: ${addr:04X}", call_stack.len() - depth));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsr_pushes_the_return_address() {
        let mut stack = Vec::new();
        track_instruction(&mut stack, JSR, 0xF000);
        assert_eq!(stack, vec![0xF003]);
    }

    #[test]
    fn rts_pops_the_stack() {
        let mut stack = vec![0xF003];
        track_instruction(&mut stack, RTS, 0xF010);
        assert!(stack.is_empty());
    }

    #[test]
    fn rts_with_empty_stack_does_not_panic() {
        let mut stack = Vec::new();
        track_instruction(&mut stack, RTS, 0xF010);
        assert!(stack.is_empty());
    }

    #[test]
    fn other_opcodes_leave_the_stack_untouched() {
        let mut stack = vec![0xF003];
        track_instruction(&mut stack, 0xEA, 0xF010); // NOP
        assert_eq!(stack, vec![0xF003]);
    }

    #[test]
    fn nested_calls_track_depth() {
        let mut stack = Vec::new();
        track_instruction(&mut stack, JSR, 0xF000);
        track_instruction(&mut stack, JSR, 0xF100);
        assert_eq!(stack, vec![0xF003, 0xF103]);
        track_instruction(&mut stack, RTS, 0xF200);
        assert_eq!(stack, vec![0xF003]);
    }
}
