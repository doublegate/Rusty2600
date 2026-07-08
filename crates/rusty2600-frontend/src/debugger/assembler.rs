//! A tiny inline 6507 assembler for the debugger panel (`[v2.12.0]`).
//!
//! Assembles user-typed 6502/6507 source (one instruction per line, e.g.
//! `LDA #$42`, `STA $0200,X`, `BNE loop_top`) into opcode bytes, ready to be
//! queued as [`super::DebugAction::Poke`] writes through the SAME gated
//! post-frame poke path the scripting engine's `emu.poke` already uses
//! (`crate::scripting::FrontendScriptBus::poke` — `system.bus.cpu_write`,
//! applied under the emu lock in `app.rs`'s `MenuAction::DebugPoke` arm) —
//! the assembler itself never touches the emu lock or the core simulation
//! path, so determinism is unaffected either way.
//!
//! The `(mnemonic, mode) -> opcode` encoding table is **derived at runtime
//! from `super::disasm::lookup`**, the canonical opcode table the CPU
//! debugger panel's own disassembly already uses, rather than hand-
//! maintained separately — so the assembler can never silently drift from
//! what the disassembler (and, by construction, the documented NMOS 6502/
//! 6507 instruction set) decodes. Only documented opcodes assemble;
//! undocumented/illegal opcodes are simply absent from the table, which is
//! honest (no invented encoding) rather than a silent gap.

use super::disasm::{Mode, lookup};

/// One opcode-table entry: a `(mnemonic, mode) -> opcode` mapping.
#[derive(Clone, Copy)]
struct OpEntry {
    mnemonic: &'static str,
    mode: Mode,
    opcode: u8,
}

/// The opcode-encoding table, built lazily from [`lookup`] exactly once and
/// reused for every [`assemble_line`] call.
static OPCODE_TABLE: std::sync::OnceLock<Vec<OpEntry>> = std::sync::OnceLock::new();

/// Borrow the cached opcode table, building it on first use.
fn opcode_table() -> &'static [OpEntry] {
    OPCODE_TABLE.get_or_init(|| {
        (0u16..=0xFF)
            .filter_map(|opcode| {
                #[allow(clippy::cast_possible_truncation)]
                let opcode = opcode as u8;
                let (mnemonic, mode) = lookup(opcode)?;
                Some(OpEntry {
                    mnemonic,
                    mode,
                    opcode,
                })
            })
            .collect()
    })
}

/// The parsed operand of a source line: a mode + a numeric value.
struct ParsedOperand {
    mode: Mode,
    value: u16,
}

/// Parse a hex/decimal number token (accepts `$NN`, `0xNN`, or plain decimal).
fn parse_num(tok: &str) -> Option<u16> {
    let t = tok.trim();
    if let Some(h) = t
        .strip_prefix('$')
        .or_else(|| t.strip_prefix("0x"))
        .or_else(|| t.strip_prefix("0X"))
    {
        return u16::from_str_radix(h, 16).ok();
    }
    t.parse::<u16>().ok()
}

/// Parse the operand portion of a source line into a `(mode, value)`. Branch
/// targets are resolved by the caller (which knows the instruction's PC).
fn parse_operand(operand: &str) -> Option<ParsedOperand> {
    let op = operand.trim();
    if op.is_empty() {
        return Some(ParsedOperand {
            mode: Mode::Implied,
            value: 0,
        });
    }
    if op.eq_ignore_ascii_case("A") {
        return Some(ParsedOperand {
            mode: Mode::Accumulator,
            value: 0,
        });
    }
    if let Some(rest) = op.strip_prefix('#') {
        return parse_num(rest).map(|v| ParsedOperand {
            mode: Mode::Immediate,
            value: v,
        });
    }
    if let Some(inner) = op.strip_prefix('(') {
        if let Some(zp) = inner
            .strip_suffix(",X)")
            .or_else(|| inner.strip_suffix(",x)"))
            .map(str::trim)
        {
            return parse_num(zp).map(|v| ParsedOperand {
                mode: Mode::IndirectX,
                value: v,
            });
        }
        if let Some(zp) = inner
            .strip_suffix(",Y")
            .or_else(|| inner.strip_suffix(",y"))
            .and_then(|s| s.strip_suffix(')'))
        {
            return parse_num(zp.trim()).map(|v| ParsedOperand {
                mode: Mode::IndirectY,
                value: v,
            });
        }
        if let Some(abs) = inner.strip_suffix(')').map(str::trim) {
            return parse_num(abs).map(|v| ParsedOperand {
                mode: Mode::Indirect,
                value: v,
            });
        }
        return None;
    }
    let wrote_wide = op.starts_with('$') && op.chars().filter(char::is_ascii_hexdigit).count() > 2;
    if let Some(base) = op.strip_suffix(",X").or_else(|| op.strip_suffix(",x")) {
        return parse_num(base).map(|v| ParsedOperand {
            mode: if wrote_wide || v > 0xFF {
                Mode::AbsoluteX
            } else {
                Mode::ZeroPageX
            },
            value: v,
        });
    }
    if let Some(base) = op.strip_suffix(",Y").or_else(|| op.strip_suffix(",y")) {
        return parse_num(base).map(|v| ParsedOperand {
            mode: if wrote_wide || v > 0xFF {
                Mode::AbsoluteY
            } else {
                Mode::ZeroPageY
            },
            value: v,
        });
    }
    parse_num(op).map(|v| ParsedOperand {
        mode: if wrote_wide || v > 0xFF {
            Mode::Absolute
        } else {
            Mode::ZeroPage
        },
        value: v,
    })
}

/// The documented 6502/6507 branch mnemonics — their operand always parses
/// as a numeric address, but the encoded addressing mode is [`Mode::Relative`],
/// not [`Mode::Absolute`]/[`Mode::ZeroPage`].
const BRANCH_MNEMONICS: [&str; 8] = ["BPL", "BMI", "BVC", "BVS", "BCC", "BCS", "BNE", "BEQ"];

/// Assemble one source line (no label, no comment — see
/// [`assemble_program`] for both) at `pc` into its opcode bytes.
///
/// `pc` is needed to compute relative-branch displacements.
///
/// # Errors
/// Returns a message describing why the line could not be assembled (empty
/// line, unknown mnemonic, invalid operand, or an out-of-range branch
/// target).
pub fn assemble_line(line: &str, pc: u16) -> Result<Vec<u8>, String> {
    let table = opcode_table();
    let line = line.trim();
    if line.is_empty() {
        return Err("empty line".into());
    }
    let mut parts = line.splitn(2, char::is_whitespace);
    let mnemonic = parts.next().unwrap_or("").to_ascii_uppercase();
    let operand = parts.next().unwrap_or("").trim().to_string();
    let parsed = parse_operand(&operand).ok_or_else(|| format!("bad operand: {operand:?}"))?;

    let want_mode = if BRANCH_MNEMONICS.contains(&mnemonic.as_str()) {
        Mode::Relative
    } else {
        parsed.mode
    };

    // A mnemonic that only has an absolute-mode encoding still accepts a
    // zero-page-range operand (standard assembler widening).
    let entry = table
        .iter()
        .find(|e| e.mnemonic == mnemonic && e.mode == want_mode)
        .or_else(|| {
            let widened = match want_mode {
                Mode::ZeroPage => Some(Mode::Absolute),
                Mode::ZeroPageX => Some(Mode::AbsoluteX),
                Mode::ZeroPageY => Some(Mode::AbsoluteY),
                _ => None,
            };
            widened.and_then(|wm| {
                table
                    .iter()
                    .find(|e| e.mnemonic == mnemonic && e.mode == wm)
            })
        })
        .ok_or_else(|| format!("no opcode for {mnemonic} with that addressing mode"))?;

    let mut bytes = vec![entry.opcode];
    match entry.mode {
        Mode::Implied | Mode::Accumulator => {}
        Mode::Relative => {
            let target = parsed.value;
            let next = pc.wrapping_add(2);
            let disp = i32::from(target) - i32::from(next);
            if !(-128..=127).contains(&disp) {
                return Err(format!("branch target ${target:04X} out of range"));
            }
            #[allow(clippy::cast_possible_truncation)]
            bytes.push((disp as i8).cast_unsigned());
        }
        m if m.operand_len() == 1 => {
            if parsed.value > 0xFF {
                return Err(format!(
                    "operand ${:04X} out of range for a single-byte addressing mode",
                    parsed.value
                ));
            }
            #[allow(clippy::cast_possible_truncation)]
            bytes.push(parsed.value as u8);
        }
        _ => {
            #[allow(clippy::cast_possible_truncation)]
            bytes.push(parsed.value as u8);
            bytes.push((parsed.value >> 8) as u8);
        }
    }
    Ok(bytes)
}

/// Assemble a multi-line source program into a flat `(address, byte)` list.
///
/// Ready to queue as [`super::DebugAction::Poke`]. Instructions are laid out
/// sequentially starting at `start`; blank lines and `;`-prefixed comment
/// lines are skipped.
///
/// No label support (`[v2.12.0]`'s deliberately scoped-down first cut — see
/// the plan's "land the real slice" convention): every operand must be a
/// literal numeric address. A labeled-branch/forward-reference assembler is
/// a natural, explicitly deferred follow-up.
///
/// # Errors
/// Returns a message identifying the 1-based source line that failed to
/// assemble.
pub fn assemble_program(source: &str, start: u16) -> Result<Vec<(u16, u8)>, String> {
    let mut pc = start;
    let mut out = Vec::new();
    for (idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let bytes = assemble_line(line, pc).map_err(|e| format!("line {}: {e}", idx + 1))?;
        #[allow(clippy::cast_possible_truncation)]
        let len = bytes.len() as u16;
        for (i, b) in bytes.into_iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            out.push((pc.wrapping_add(i as u16), b));
        }
        pc = pc.wrapping_add(len);
    }
    Ok(out)
}

/// Persistent state for the assembler panel: the target-address and source
/// text inputs, plus the last assemble/write status line.
#[derive(Debug, Default, Clone)]
pub struct AssemblerState {
    /// The hex text the user is typing into the "target address" field.
    pub target_input: String,
    /// The multi-line 6507 source text the user is editing.
    pub source: String,
    /// The result of the last "Assemble & Write" click (byte count on
    /// success, or the assembler's error message).
    pub status: Option<String>,
}

/// Render the inline-assembler panel.
///
/// Returns the assembled `(address, byte)` writes when the user clicks
/// "Assemble & Write" and assembly succeeds — the caller queues these as
/// [`super::DebugAction::Poke`], same as every other debugger action (never
/// written directly from here).
pub fn render_assembler_panel(
    ui: &mut egui::Ui,
    state: &mut AssemblerState,
) -> Option<Vec<(u16, u8)>> {
    ui.horizontal(|ui| {
        ui.label("Target address:");
        ui.text_edit_singleline(&mut state.target_input);
    });
    ui.label("Source (one instruction per line; `;` starts a comment):");
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut state.source)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            );
        });
    let mut result = None;
    ui.horizontal(|ui| {
        if ui.button("Assemble & Write").clicked() {
            match parse_num(&state.target_input) {
                Some(target) => match assemble_program(&state.source, target) {
                    Ok(writes) => {
                        state.status =
                            Some(format!("wrote {} byte(s) at ${target:04X}", writes.len()));
                        result = Some(writes);
                    }
                    Err(e) => state.status = Some(format!("assemble failed: {e}")),
                },
                None => state.status = Some("bad target address".to_string()),
            }
        }
        if ui.button("Clear").clicked() {
            state.source.clear();
            state.status = None;
        }
    });
    if let Some(status) = &state.status {
        ui.label(status);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_common_instructions() {
        assert_eq!(assemble_line("LDA #$42", 0x0000).unwrap(), vec![0xA9, 0x42]);
        assert_eq!(
            assemble_line("STA $0200,X", 0x0000).unwrap(),
            vec![0x9D, 0x00, 0x02]
        );
        assert_eq!(
            assemble_line("JMP ($1234)", 0x0000).unwrap(),
            vec![0x6C, 0x34, 0x12]
        );
        assert_eq!(assemble_line("NOP", 0x0000).unwrap(), vec![0xEA]);
        assert_eq!(assemble_line("INX", 0x0000).unwrap(), vec![0xE8]);
        assert_eq!(assemble_line("LDA $10", 0x0000).unwrap(), vec![0xA5, 0x10]);
    }

    #[test]
    fn branch_displacement() {
        assert_eq!(
            assemble_line("BNE $C010", 0xC000).unwrap(),
            vec![0xD0, 0x0E]
        );
        assert_eq!(
            assemble_line("BNE $C000", 0xC010).unwrap(),
            vec![0xD0, (-0x12i8).cast_unsigned()]
        );
        assert!(assemble_line("BNE $E000", 0xC000).is_err());
    }

    #[test]
    fn lowercase_indexed_indirect_parses() {
        let upper = assemble_line("LDA ($20,X)", 0x0000).unwrap();
        let lower = assemble_line("LDA ($20,x)", 0x0000).unwrap();
        assert_eq!(upper, vec![0xA1, 0x20]);
        assert_eq!(lower, upper);
    }

    #[test]
    fn rejects_garbage() {
        assert!(assemble_line("FOO #$01", 0).is_err());
        assert!(assemble_line("", 0).is_err());
    }

    #[test]
    fn rejects_out_of_range_single_byte_operands() {
        // A 16-bit value handed to a single-byte addressing mode must error,
        // never silently truncate (e.g. `LDA #$100` must NOT assemble as
        // the very different `LDA #$00`).
        assert!(assemble_line("LDA #$100", 0).is_err());
        assert!(assemble_line("LDA ($1234,X)", 0).is_err());
        // The in-range boundary still assembles normally.
        assert_eq!(assemble_line("LDA #$FF", 0).unwrap(), vec![0xA9, 0xFF]);
    }

    #[test]
    fn parse_num_accepts_dollar_0x_and_decimal() {
        assert_eq!(parse_num("$1000"), Some(0x1000));
        assert_eq!(parse_num("0x1000"), Some(0x1000));
        assert_eq!(parse_num("0X1000"), Some(0x1000));
        assert_eq!(parse_num("4096"), Some(4096));
        assert_eq!(parse_num("not a number"), None);
    }

    #[test]
    fn round_trips_through_disassembler() {
        for (src, pc) in [("LDX #$00", 0u16), ("STA $0300", 0), ("CMP ($20),Y", 0)] {
            let bytes = assemble_line(src, pc).unwrap();
            let text = super::super::disasm::disassemble_one(
                |a| bytes.get(a as usize).copied().unwrap_or(0),
                0,
            )
            .text;
            let mnemonic = src.split_whitespace().next().unwrap();
            assert!(text.starts_with(mnemonic), "{text} vs {mnemonic}");
        }
    }

    #[test]
    fn assembles_a_multi_line_program_sequentially() {
        let src = "; comment\nLDA #$01\nSTA $80\n\nINX\n";
        let writes = assemble_program(src, 0xF000).unwrap();
        assert_eq!(
            writes,
            vec![
                (0xF000, 0xA9),
                (0xF001, 0x01),
                (0xF002, 0x85),
                (0xF003, 0x80),
                (0xF004, 0xE8),
            ]
        );
    }

    #[test]
    fn program_error_identifies_the_failing_line() {
        let src = "LDA #$01\nFOO\n";
        let err = assemble_program(src, 0).unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
    }
}
