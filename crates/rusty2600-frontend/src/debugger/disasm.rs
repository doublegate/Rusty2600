//! A standalone 6502 disassembler for the debugger's CPU/trace panels.
//!
//! Deliberately independent of `rusty2600_cpu`'s internal opcode dispatch
//! (which is private and cycle-focused, not text-focused) — this is a
//! from-scratch mnemonic + addressing-mode table for DISPLAY purposes only.
//! Covers the full documented NMOS 6502 instruction set; undocumented
//! opcodes not covered here (a small minority) render as `.byte $xx`, which
//! is honest (no invented mnemonic) rather than wrong.

/// The 6502's addressing modes, used only to know the instruction's total
/// length and how to format its operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY,
    Relative,
}

impl Mode {
    const fn operand_len(self) -> u16 {
        match self {
            Self::Implied | Self::Accumulator => 0,
            Self::Immediate
            | Self::ZeroPage
            | Self::ZeroPageX
            | Self::ZeroPageY
            | Self::IndirectX
            | Self::IndirectY
            | Self::Relative => 1,
            Self::Absolute | Self::AbsoluteX | Self::AbsoluteY | Self::Indirect => 2,
        }
    }
}

/// One disassembled instruction: the mnemonic + formatted operand text, and
/// how many bytes (including the opcode) it occupies.
#[derive(Debug, Clone)]
pub struct Instruction {
    /// The formatted text, e.g. `"LDA $1000,X"` or `"BNE $10F2"`.
    pub text: String,
    /// Total length in bytes (1-3).
    pub len: u16,
}

/// Look up `(mnemonic, mode)` for an opcode byte. Returns `None` for opcodes
/// not in the documented NMOS 6502 set (illegal/undocumented opcodes).
#[allow(clippy::too_many_lines)]
const fn lookup(opcode: u8) -> Option<(&'static str, Mode)> {
    use Mode::{
        Absolute, AbsoluteX, AbsoluteY, Accumulator, Immediate, Implied, Indirect, IndirectX,
        IndirectY, Relative, ZeroPage, ZeroPageX, ZeroPageY,
    };
    Some(match opcode {
        0x00 => ("BRK", Implied),
        0x01 => ("ORA", IndirectX),
        0x05 => ("ORA", ZeroPage),
        0x06 => ("ASL", ZeroPage),
        0x08 => ("PHP", Implied),
        0x09 => ("ORA", Immediate),
        0x0A => ("ASL", Accumulator),
        0x0D => ("ORA", Absolute),
        0x0E => ("ASL", Absolute),
        0x10 => ("BPL", Relative),
        0x11 => ("ORA", IndirectY),
        0x15 => ("ORA", ZeroPageX),
        0x16 => ("ASL", ZeroPageX),
        0x18 => ("CLC", Implied),
        0x19 => ("ORA", AbsoluteY),
        0x1D => ("ORA", AbsoluteX),
        0x1E => ("ASL", AbsoluteX),
        0x20 => ("JSR", Absolute),
        0x21 => ("AND", IndirectX),
        0x24 => ("BIT", ZeroPage),
        0x25 => ("AND", ZeroPage),
        0x26 => ("ROL", ZeroPage),
        0x28 => ("PLP", Implied),
        0x29 => ("AND", Immediate),
        0x2A => ("ROL", Accumulator),
        0x2C => ("BIT", Absolute),
        0x2D => ("AND", Absolute),
        0x2E => ("ROL", Absolute),
        0x30 => ("BMI", Relative),
        0x31 => ("AND", IndirectY),
        0x35 => ("AND", ZeroPageX),
        0x36 => ("ROL", ZeroPageX),
        0x38 => ("SEC", Implied),
        0x39 => ("AND", AbsoluteY),
        0x3D => ("AND", AbsoluteX),
        0x3E => ("ROL", AbsoluteX),
        0x40 => ("RTI", Implied),
        0x41 => ("EOR", IndirectX),
        0x45 => ("EOR", ZeroPage),
        0x46 => ("LSR", ZeroPage),
        0x48 => ("PHA", Implied),
        0x49 => ("EOR", Immediate),
        0x4A => ("LSR", Accumulator),
        0x4C => ("JMP", Absolute),
        0x4D => ("EOR", Absolute),
        0x4E => ("LSR", Absolute),
        0x50 => ("BVC", Relative),
        0x51 => ("EOR", IndirectY),
        0x55 => ("EOR", ZeroPageX),
        0x56 => ("LSR", ZeroPageX),
        0x58 => ("CLI", Implied),
        0x59 => ("EOR", AbsoluteY),
        0x5D => ("EOR", AbsoluteX),
        0x5E => ("LSR", AbsoluteX),
        0x60 => ("RTS", Implied),
        0x61 => ("ADC", IndirectX),
        0x65 => ("ADC", ZeroPage),
        0x66 => ("ROR", ZeroPage),
        0x68 => ("PLA", Implied),
        0x69 => ("ADC", Immediate),
        0x6A => ("ROR", Accumulator),
        0x6C => ("JMP", Indirect),
        0x6D => ("ADC", Absolute),
        0x6E => ("ROR", Absolute),
        0x70 => ("BVS", Relative),
        0x71 => ("ADC", IndirectY),
        0x75 => ("ADC", ZeroPageX),
        0x76 => ("ROR", ZeroPageX),
        0x78 => ("SEI", Implied),
        0x79 => ("ADC", AbsoluteY),
        0x7D => ("ADC", AbsoluteX),
        0x7E => ("ROR", AbsoluteX),
        0x81 => ("STA", IndirectX),
        0x84 => ("STY", ZeroPage),
        0x85 => ("STA", ZeroPage),
        0x86 => ("STX", ZeroPage),
        0x88 => ("DEY", Implied),
        0x8A => ("TXA", Implied),
        0x8C => ("STY", Absolute),
        0x8D => ("STA", Absolute),
        0x8E => ("STX", Absolute),
        0x90 => ("BCC", Relative),
        0x91 => ("STA", IndirectY),
        0x94 => ("STY", ZeroPageX),
        0x95 => ("STA", ZeroPageX),
        0x96 => ("STX", ZeroPageY),
        0x98 => ("TYA", Implied),
        0x99 => ("STA", AbsoluteY),
        0x9A => ("TXS", Implied),
        0x9D => ("STA", AbsoluteX),
        0xA0 => ("LDY", Immediate),
        0xA1 => ("LDA", IndirectX),
        0xA2 => ("LDX", Immediate),
        0xA4 => ("LDY", ZeroPage),
        0xA5 => ("LDA", ZeroPage),
        0xA6 => ("LDX", ZeroPage),
        0xA8 => ("TAY", Implied),
        0xA9 => ("LDA", Immediate),
        0xAA => ("TAX", Implied),
        0xAC => ("LDY", Absolute),
        0xAD => ("LDA", Absolute),
        0xAE => ("LDX", Absolute),
        0xB0 => ("BCS", Relative),
        0xB1 => ("LDA", IndirectY),
        0xB4 => ("LDY", ZeroPageX),
        0xB5 => ("LDA", ZeroPageX),
        0xB6 => ("LDX", ZeroPageY),
        0xB8 => ("CLV", Implied),
        0xB9 => ("LDA", AbsoluteY),
        0xBA => ("TSX", Implied),
        0xBC => ("LDY", AbsoluteX),
        0xBD => ("LDA", AbsoluteX),
        0xBE => ("LDX", AbsoluteY),
        0xC0 => ("CPY", Immediate),
        0xC1 => ("CMP", IndirectX),
        0xC4 => ("CPY", ZeroPage),
        0xC5 => ("CMP", ZeroPage),
        0xC6 => ("DEC", ZeroPage),
        0xC8 => ("INY", Implied),
        0xC9 => ("CMP", Immediate),
        0xCA => ("DEX", Implied),
        0xCC => ("CPY", Absolute),
        0xCD => ("CMP", Absolute),
        0xCE => ("DEC", Absolute),
        0xD0 => ("BNE", Relative),
        0xD1 => ("CMP", IndirectY),
        0xD5 => ("CMP", ZeroPageX),
        0xD6 => ("DEC", ZeroPageX),
        0xD8 => ("CLD", Implied),
        0xD9 => ("CMP", AbsoluteY),
        0xDD => ("CMP", AbsoluteX),
        0xDE => ("DEC", AbsoluteX),
        0xE0 => ("CPX", Immediate),
        0xE1 => ("SBC", IndirectX),
        0xE4 => ("CPX", ZeroPage),
        0xE5 => ("SBC", ZeroPage),
        0xE6 => ("INC", ZeroPage),
        0xE8 => ("INX", Implied),
        0xE9 => ("SBC", Immediate),
        0xEA => ("NOP", Implied),
        0xEC => ("CPX", Absolute),
        0xED => ("SBC", Absolute),
        0xEE => ("INC", Absolute),
        0xF0 => ("BEQ", Relative),
        0xF1 => ("SBC", IndirectY),
        0xF5 => ("SBC", ZeroPageX),
        0xF6 => ("INC", ZeroPageX),
        0xF8 => ("SED", Implied),
        0xF9 => ("SBC", AbsoluteY),
        0xFD => ("SBC", AbsoluteX),
        0xFE => ("INC", AbsoluteX),
        _ => return None,
    })
}

/// Disassemble one instruction starting at `pc`.
///
/// Uses `peek` (a side-effect-free byte read, e.g. `Bus::peek`) to fetch the
/// opcode and any operand bytes. Returns the formatted text and the
/// instruction's length in bytes (always at least 1, even for an
/// unrecognized opcode).
pub fn disassemble_one(peek: impl Fn(u16) -> u8, pc: u16) -> Instruction {
    let opcode = peek(pc);
    let Some((mnemonic, mode)) = lookup(opcode) else {
        return Instruction {
            text: format!(".byte ${opcode:02X}"),
            len: 1,
        };
    };

    let operand_text = match mode {
        Mode::Implied => String::new(),
        Mode::Accumulator => "A".into(),
        Mode::Immediate => format!("#${:02X}", peek(pc.wrapping_add(1))),
        Mode::ZeroPage => format!("${:02X}", peek(pc.wrapping_add(1))),
        Mode::ZeroPageX => format!("${:02X},X", peek(pc.wrapping_add(1))),
        Mode::ZeroPageY => format!("${:02X},Y", peek(pc.wrapping_add(1))),
        Mode::IndirectX => format!("(${:02X},X)", peek(pc.wrapping_add(1))),
        Mode::IndirectY => format!("(${:02X}),Y", peek(pc.wrapping_add(1))),
        Mode::Absolute => format!(
            "${:04X}",
            u16::from(peek(pc.wrapping_add(1))) | (u16::from(peek(pc.wrapping_add(2))) << 8)
        ),
        Mode::AbsoluteX => format!(
            "${:04X},X",
            u16::from(peek(pc.wrapping_add(1))) | (u16::from(peek(pc.wrapping_add(2))) << 8)
        ),
        Mode::AbsoluteY => format!(
            "${:04X},Y",
            u16::from(peek(pc.wrapping_add(1))) | (u16::from(peek(pc.wrapping_add(2))) << 8)
        ),
        Mode::Indirect => format!(
            "(${:04X})",
            u16::from(peek(pc.wrapping_add(1))) | (u16::from(peek(pc.wrapping_add(2))) << 8)
        ),
        Mode::Relative => {
            // The branch offset is signed and relative to the address AFTER
            // this 2-byte instruction (matching real 6502 PC-relative timing).
            let offset = peek(pc.wrapping_add(1)).cast_signed();
            #[allow(clippy::cast_sign_loss)]
            let target = pc.wrapping_add(2).wrapping_add(offset as u16);
            format!("${target:04X}")
        }
    };

    let text = if operand_text.is_empty() {
        String::from(mnemonic)
    } else {
        format!("{mnemonic} {operand_text}")
    };

    Instruction {
        text,
        len: 1 + mode.operand_len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Vec;

    fn peek_from(mem: &[u8]) -> impl Fn(u16) -> u8 + '_ {
        move |addr| mem[addr as usize]
    }

    #[test]
    fn disassembles_implied() {
        let mem = [0xEA]; // NOP
        let insn = disassemble_one(peek_from(&mem), 0);
        assert_eq!(insn.text, "NOP");
        assert_eq!(insn.len, 1);
    }

    #[test]
    fn disassembles_immediate() {
        let mem = [0xA9, 0x42]; // LDA #$42
        let insn = disassemble_one(peek_from(&mem), 0);
        assert_eq!(insn.text, "LDA #$42");
        assert_eq!(insn.len, 2);
    }

    #[test]
    fn disassembles_absolute() {
        let mem = [0x8D, 0x00, 0x02]; // STA $0200
        let insn = disassemble_one(peek_from(&mem), 0);
        assert_eq!(insn.text, "STA $0200");
        assert_eq!(insn.len, 3);
    }

    #[test]
    fn disassembles_relative_branch_target() {
        // BNE -2 (branches back to itself): at PC=0x1000, target = 0x1002 + (-2) = 0x1000.
        let mut mem = Vec::from([0u8; 0x1003]);
        mem[0x1000] = 0xD0; // BNE
        mem[0x1001] = 0xFE; // -2
        let insn = disassemble_one(peek_from(&mem), 0x1000);
        assert_eq!(insn.text, "BNE $1000");
        assert_eq!(insn.len, 2);
    }

    #[test]
    fn unrecognized_opcode_renders_as_byte_directive() {
        let mem = [0xFF]; // not in the documented NMOS table
        let insn = disassemble_one(peek_from(&mem), 0);
        assert_eq!(insn.text, ".byte $FF");
        assert_eq!(insn.len, 1);
    }
}
