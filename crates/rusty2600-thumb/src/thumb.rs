//! Thumb-1 instruction decode/execute — all 19 instruction-format classes
//! from the ARM7TDMI Data Sheet, ported from Gopher2600's `thumb.go`.
//!
//! Skipped versus the reference: the disassembly-string generation
//! interleaved into the same decode functions there (this crate has no
//! disassembler yet); the IT-block guards (`itMask == 0b0000` checks —
//! Thumb-1 has no IT blocks, so these always held true in practice, and
//! every flag update below is unconditional to match); and the
//! `hook.ARMinterrupt` real-ARM32-function-call path a `BX` to non-Thumb
//! code can trigger in the reference (an out-of-scope cartridge-specific
//! integration mechanism — see the crate-level docs; reported as a fault
//! here instead of silently mishandled).
//!
//! Truncating `u32 -> u8`/`u16` casts throughout are the correct, intended
//! behavior of byte/halfword store instructions, not bugs — allowed at the
//! module level rather than annotated at every call site.
#![allow(clippy::cast_possible_truncation)]

use crate::Arm7Tdmi;
use crate::Status;
use crate::cycles::BusAccess;
use crate::memory::{Fault, ThumbMemory};
use crate::registers::{REG_LR, REG_PC, REG_SP};

/// `Ok(Some(()))` = normal continuation, `Ok(None)` = the program reached
/// its expected return address (`BX`/`BLX`), `Err` = a memory fault.
type StepResult = Result<Option<()>, Fault>;

const fn align_to_32bits(v: u32) -> u32 {
    v & 0xffff_fffc
}

/// Go's `<<` operator is DEFINED to yield 0 for a shift count at or beyond
/// the operand's bit width (see the Go language spec's "Operators"
/// section) — unlike Rust's native shift operators, which panic (debug
/// builds) or are unspecified (release) for out-of-range counts.
/// Gopher2600's ROR-by-register carry computation (format 4) relies on
/// this Go-specific behavior with an UNMASKED shift count, so this helper
/// reproduces it exactly for differential parity with the reference.
const fn go_shl(value: u32, amount: u32) -> u32 {
    if amount >= 32 { 0 } else { value << amount }
}

/// Decode and execute one 16-bit Thumb opcode.
pub(crate) fn execute<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    // Working backwards up the table in Figure 5-1 of the ARM7TDMI Data Sheet
    // (matches Gopher2600's own `decodeThumb` dispatch order exactly).
    if opcode & 0xf000 == 0xf000 {
        long_branch_with_link(arm, opcode)
    } else if opcode & 0xf000 == 0xe000 {
        unconditional_branch(arm, opcode)
    } else if opcode & 0xff00 == 0xdf00 {
        software_interrupt(arm)
    } else if opcode & 0xf000 == 0xd000 {
        conditional_branch(arm, opcode)
    } else if opcode & 0xf000 == 0xc000 {
        multiple_load_store(arm, opcode)
    } else if opcode & 0xf600 == 0xb400 {
        push_pop_registers(arm, opcode)
    } else if opcode & 0xff00 == 0xb000 {
        add_offset_to_sp(arm, opcode)
    } else if opcode & 0xf000 == 0xa000 {
        load_address(arm, opcode)
    } else if opcode & 0xf000 == 0x9000 {
        sp_relative_load_store(arm, opcode)
    } else if opcode & 0xf000 == 0x8000 {
        load_store_halfword(arm, opcode)
    } else if opcode & 0xe000 == 0x6000 {
        load_store_with_imm_offset(arm, opcode)
    } else if opcode & 0xf200 == 0x5200 {
        load_store_sign_extended(arm, opcode)
    } else if opcode & 0xf200 == 0x5000 {
        load_store_with_register_offset(arm, opcode)
    } else if opcode & 0xf800 == 0x4800 {
        pc_relative_load(arm, opcode)
    } else if opcode & 0xfc00 == 0x4400 {
        hi_register_ops(arm, opcode)
    } else if opcode & 0xfc00 == 0x4000 {
        alu_operations(arm, opcode)
    } else if opcode & 0xe000 == 0x2000 {
        mov_cmp_add_sub_imm(arm, opcode)
    } else if opcode & 0xf800 == 0x1800 {
        add_subtract(arm, opcode)
    } else {
        // opcode & 0xe000 == 0x0000
        move_shifted_register(arm, opcode)
    }
}

/// Format 1 — Move shifted register (`LSL`/`LSR`/`ASR` by immediate).
fn move_shifted_register<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let op = (opcode & 0x1800) >> 11;
    let shift = u32::from((opcode & 0x7c0) >> 6);
    let src_reg = usize::from((opcode & 0x38) >> 3);
    let dest_reg = usize::from(opcode & 0x07);
    let src_val = arm.registers[src_reg];

    match op {
        0b00 => {
            // LSL
            if shift == 0 {
                arm.registers[dest_reg] = src_val;
            } else {
                let m = 1u32 << (32 - shift);
                arm.status.set(Status::CARRY, src_val & m == m);
                arm.registers[dest_reg] = src_val << shift;
            }
        }
        0b01 => {
            // LSR
            if shift == 0 {
                arm.status
                    .set(Status::CARRY, src_val & 0x8000_0000 == 0x8000_0000);
                arm.registers[dest_reg] = 0;
            } else {
                let m = 1u32 << (shift - 1);
                arm.status.set(Status::CARRY, src_val & m == m);
                arm.registers[dest_reg] = src_val >> shift;
            }
        }
        0b10 => {
            // ASR
            if shift == 0 {
                arm.status
                    .set(Status::CARRY, src_val & 0x8000_0000 == 0x8000_0000);
                arm.registers[dest_reg] = if arm.status.contains(Status::CARRY) {
                    0xffff_ffff
                } else {
                    0
                };
            } else {
                let m = 1u32 << (shift - 1);
                arm.status.set(Status::CARRY, src_val & m == m);
                let mut a = src_val >> shift;
                if src_val & 0x8000_0000 == 0x8000_0000 {
                    a |= 0xffff_ffffu32 << (32 - shift);
                }
                arm.registers[dest_reg] = a;
            }
        }
        _ => unreachable!("format-1 op 0b11 is dispatched to format 2 (add/subtract) instead"),
    }

    arm.status.set_nz(arm.registers[dest_reg]);

    if shift > 0 {
        arm.icycle();
    }

    Ok(Some(()))
}

/// Format 2 — Add/subtract (register or 3-bit immediate).
fn add_subtract<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let immediate = opcode & 0x0400 == 0x0400;
    let subtract = opcode & 0x0200 == 0x0200;
    let imm = u32::from((opcode & 0x01c0) >> 6);
    let src_reg = usize::from((opcode & 0x038) >> 3);
    let dest_reg = usize::from(opcode & 0x07);

    // When `!immediate`, the same 3-bit field is a register index instead of
    // a literal value — matches Gopher2600's reuse of the `imm` variable.
    let val = if immediate {
        imm
    } else {
        arm.registers[imm as usize]
    };

    if subtract {
        arm.status.set_add_flags(arm.registers[src_reg], !val, 1);
        arm.registers[dest_reg] = arm.registers[src_reg].wrapping_sub(val);
    } else {
        arm.status.set_add_flags(arm.registers[src_reg], val, 0);
        arm.registers[dest_reg] = arm.registers[src_reg].wrapping_add(val);
    }
    arm.status.set_nz(arm.registers[dest_reg]);

    Ok(Some(()))
}

/// Format 3 — Move/compare/add/subtract immediate (8-bit, on a Lo register).
fn mov_cmp_add_sub_imm<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let op = (opcode & 0x1800) >> 11;
    let dest_reg = usize::from((opcode & 0x0700) >> 8);
    let imm = u32::from(opcode & 0x00ff);

    match op {
        0b00 => {
            // MOV
            arm.registers[dest_reg] = imm;
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b01 => {
            // CMP
            arm.status.set_add_flags(arm.registers[dest_reg], !imm, 1);
            let cmp = arm.registers[dest_reg].wrapping_sub(imm);
            arm.status.set_nz(cmp);
        }
        0b10 => {
            // ADD
            arm.status.set_add_flags(arm.registers[dest_reg], imm, 0);
            arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_add(imm);
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b11 => {
            // SUB
            arm.status.set_add_flags(arm.registers[dest_reg], !imm, 1);
            arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_sub(imm);
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        _ => unreachable!("format 3's op field is exactly 2 bits"),
    }

    Ok(Some(()))
}

/// Format 4 — ALU operations between a Lo register pair.
fn alu_operations<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let op = (opcode & 0x03c0) >> 6;
    let src_reg = usize::from((opcode & 0x38) >> 3);
    let dest_reg = usize::from(opcode & 0x07);

    let mut shift: u32 = 0;
    let mut mul = false;
    let mut mul_operand: u32 = 0;

    match op {
        0b0000 => {
            // AND
            arm.registers[dest_reg] &= arm.registers[src_reg];
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0001 => {
            // EOR
            arm.registers[dest_reg] ^= arm.registers[src_reg];
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0010 => {
            // LSL (by register)
            shift = arm.registers[src_reg];
            if shift > 0 && shift < 32 {
                let m = 1u32 << (32 - shift);
                arm.status
                    .set(Status::CARRY, arm.registers[dest_reg] & m == m);
                arm.registers[dest_reg] <<= shift;
            } else if shift == 32 {
                arm.status
                    .set(Status::CARRY, arm.registers[dest_reg] & 0x01 == 0x01);
                arm.registers[dest_reg] = 0;
            } else if shift > 32 {
                arm.status.set(Status::CARRY, false);
                arm.registers[dest_reg] = 0;
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0011 => {
            // LSR (by register)
            shift = arm.registers[src_reg];
            if shift > 0 && shift < 32 {
                let m = 1u32 << (shift - 1);
                arm.status
                    .set(Status::CARRY, arm.registers[dest_reg] & m == m);
                arm.registers[dest_reg] >>= shift;
            } else if shift == 32 {
                arm.status.set(
                    Status::CARRY,
                    arm.registers[dest_reg] & 0x8000_0000 == 0x8000_0000,
                );
                arm.registers[dest_reg] = 0;
            } else if shift > 32 {
                arm.status.set(Status::CARRY, false);
                arm.registers[dest_reg] = 0;
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0100 => {
            // ASR (by register)
            shift = arm.registers[src_reg];
            if shift > 0 && shift < 32 {
                let src = arm.registers[dest_reg];
                let m = 1u32 << (shift - 1);
                arm.status.set(Status::CARRY, src & m == m);
                let mut a = src >> shift;
                if src & 0x8000_0000 == 0x8000_0000 {
                    a |= 0xffff_ffffu32 << (32 - shift);
                }
                arm.registers[dest_reg] = a;
            } else if shift >= 32 {
                arm.status.set(
                    Status::CARRY,
                    arm.registers[dest_reg] & 0x8000_0000 == 0x8000_0000,
                );
                arm.registers[dest_reg] = if arm.status.contains(Status::CARRY) {
                    0xffff_ffff
                } else {
                    0
                };
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0101 => {
            // ADC
            if arm.status.contains(Status::CARRY) {
                arm.status
                    .set_add_flags(arm.registers[dest_reg], arm.registers[src_reg], 1);
                arm.registers[dest_reg] = arm.registers[dest_reg]
                    .wrapping_add(arm.registers[src_reg])
                    .wrapping_add(1);
            } else {
                arm.status
                    .set_add_flags(arm.registers[dest_reg], arm.registers[src_reg], 0);
                arm.registers[dest_reg] =
                    arm.registers[dest_reg].wrapping_add(arm.registers[src_reg]);
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0110 => {
            // SBC
            if arm.status.contains(Status::CARRY) {
                arm.status
                    .set_add_flags(arm.registers[dest_reg], !arm.registers[src_reg], 1);
                arm.registers[dest_reg] =
                    arm.registers[dest_reg].wrapping_sub(arm.registers[src_reg]);
            } else {
                arm.status
                    .set_add_flags(arm.registers[dest_reg], !arm.registers[src_reg], 0);
                arm.registers[dest_reg] = arm.registers[dest_reg]
                    .wrapping_sub(arm.registers[src_reg])
                    .wrapping_sub(1);
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b0111 => {
            // ROR (by register)
            shift = arm.registers[src_reg];
            if shift & 0xff == 0 {
                // unaffected
            } else if shift & 0x1f == 0 {
                arm.status.set(
                    Status::CARRY,
                    arm.registers[dest_reg] & 0x8000_0000 == 0x8000_0000,
                );
            } else {
                let m = go_shl(1, shift - 1);
                arm.status
                    .set(Status::CARRY, arm.registers[dest_reg] & m == m);
                arm.registers[dest_reg] = arm.registers[dest_reg].rotate_right(shift & 0x1f);
            }
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b1000 => {
            // TST
            let w = arm.registers[dest_reg] & arm.registers[src_reg];
            arm.status.set_nz(w);
        }
        0b1001 => {
            // NEG
            arm.status.set_add_flags(0, !arm.registers[src_reg], 1);
            arm.registers[dest_reg] = 0u32.wrapping_sub(arm.registers[src_reg]);
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b1010 => {
            // CMP
            arm.status
                .set_add_flags(arm.registers[dest_reg], !arm.registers[src_reg], 1);
            let cmp = arm.registers[dest_reg].wrapping_sub(arm.registers[src_reg]);
            arm.status.set_nz(cmp);
        }
        0b1011 => {
            // CMN
            arm.status
                .set_add_flags(arm.registers[dest_reg], arm.registers[src_reg], 0);
            let cmp = arm.registers[dest_reg].wrapping_add(arm.registers[src_reg]);
            arm.status.set_nz(cmp);
        }
        0b1100 => {
            // ORR
            arm.registers[dest_reg] |= arm.registers[src_reg];
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b1101 => {
            // MUL
            mul = true;
            mul_operand = arm.registers[src_reg];
            arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_mul(arm.registers[src_reg]);
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b1110 => {
            // BIC
            arm.registers[dest_reg] &= !arm.registers[src_reg];
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        0b1111 => {
            // MVN
            arm.registers[dest_reg] = !arm.registers[src_reg];
            arm.status.set_nz(arm.registers[dest_reg]);
        }
        _ => unreachable!("format 4's op field is exactly 4 bits"),
    }

    // "7.2 Instruction Cycle Count Summary" in "ARM7TDMI-S Technical
    // Reference Manual r4p3": multiply cost depends on how many of the
    // multiplier's high bits are all-zero or all-one.
    if mul {
        let p = (mul_operand & 0xffff_ff00).count_ones();
        if p == 0 || p == 24 {
            arm.icycle();
        } else {
            let p = (mul_operand & 0xffff_0000).count_ones();
            if p == 0 || p == 16 {
                arm.icycle();
                arm.icycle();
            } else {
                let p = (mul_operand & 0xff00_0000).count_ones();
                if p == 0 || p == 8 {
                    arm.icycle();
                    arm.icycle();
                    arm.icycle();
                } else {
                    arm.icycle();
                    arm.icycle();
                    arm.icycle();
                    arm.icycle();
                }
            }
        }
    } else if shift > 0 {
        arm.icycle();
    }

    Ok(Some(()))
}

/// Format 5 — Hi register operations / branch exchange.
fn hi_register_ops<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let op = (opcode & 0x300) >> 8;
    let hi1 = opcode & 0x80 == 0x80;
    let hi2 = opcode & 0x40 == 0x40;
    let mut src_reg = usize::from((opcode & 0x38) >> 3);
    let mut dest_reg = usize::from(opcode & 0x07);
    if hi1 {
        dest_reg += 8;
    }
    if hi2 {
        src_reg += 8;
    }

    match op {
        0b00 => {
            // ADD (not two's complement)
            arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_add(arm.registers[src_reg]);
            if dest_reg == REG_PC {
                // "the value will be the address of the instruction + 4 with
                // bit 0 cleared" — PC is already +4-relative here (see
                // registers.rs), so we only add the remaining +2 and mask.
                arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_add(2) & 0xffff_fffe;
            }
        }
        0b01 => {
            // CMP
            arm.status
                .set_add_flags(arm.registers[dest_reg], !arm.registers[src_reg], 1);
            let cmp = arm.registers[dest_reg].wrapping_sub(arm.registers[src_reg]);
            arm.status.set_nz(cmp);
        }
        0b10 => {
            // MOV
            arm.registers[dest_reg] = arm.registers[src_reg];
            if dest_reg == REG_PC {
                arm.registers[dest_reg] = arm.registers[dest_reg].wrapping_add(2) & 0xffff_fffe;
            }
        }
        0b11 => {
            // BX / BLX
            let thumb_mode = arm.registers[src_reg] & 0x01 == 0x01;
            let new_pc = if src_reg == REG_PC {
                arm.registers[REG_PC].wrapping_add(2)
            } else {
                arm.registers[src_reg].wrapping_add(2) & 0xffff_fffe
            };

            if thumb_mode {
                arm.registers[REG_PC] = new_pc;
                return Ok(Some(()));
            }

            if new_pc == arm.expected_return_address {
                return Ok(None);
            }

            // A BX to real (non-Thumb) ARM code with no expected-return
            // match means the program wants to call a native ARM32 helper
            // function — Gopher2600's `hook.ARMinterrupt` mechanism. That's
            // a cartridge-specific integration point deliberately out of
            // scope for this interpreter core (see the crate-level docs);
            // report it rather than silently returning wrong results.
            return Err(Fault::UnimplementedPeripheral(new_pc));
        }
        _ => unreachable!("format 5's op field is exactly 2 bits"),
    }

    Ok(Some(()))
}

/// Format 6 — PC-relative load.
fn pc_relative_load<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let dest_reg = usize::from((opcode & 0x0700) >> 8);
    let imm = u32::from(opcode & 0x00ff) << 2;

    // "Bit 1 of the PC value is forced to zero for the purpose of this
    // calculation, so the address is always word-aligned."
    let pc = align_to_32bits(arm.registers[REG_PC]);
    let addr = pc.wrapping_add(imm);
    arm.registers[dest_reg] = arm.read32(addr, false)?;
    arm.ncycle(BusAccess::DataRead, addr);
    arm.icycle();

    Ok(Some(()))
}

/// Format 7 — Load/store with register offset.
fn load_store_with_register_offset<M: ThumbMemory>(
    arm: &mut Arm7Tdmi<M>,
    opcode: u16,
) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let byte_transfer = opcode & 0x0400 == 0x0400;
    let offset_reg = usize::from((opcode & 0x01c0) >> 6);
    let base_reg = usize::from((opcode & 0x0038) >> 3);
    let reg = usize::from(opcode & 0x0007);
    let addr = arm.registers[base_reg].wrapping_add(arm.registers[offset_reg]);

    if load {
        if byte_transfer {
            arm.registers[reg] = u32::from(arm.read8(addr)?);
        } else {
            arm.registers[reg] = arm.read32(addr, false)?;
        }
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else if byte_transfer {
        arm.write8(addr, arm.registers[reg] as u8)?;
        arm.store_register_cycles(addr);
    } else {
        arm.write32(addr, arm.registers[reg], false)?;
        arm.store_register_cycles(addr);
    }

    Ok(Some(()))
}

/// Format 8 — Load/store sign-extended byte/halfword.
fn load_store_sign_extended<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let hi = opcode & 0x0800 == 0x0800;
    let sign = opcode & 0x0400 == 0x0400;
    let offset_reg = usize::from((opcode & 0x01c0) >> 6);
    let base_reg = usize::from((opcode & 0x0038) >> 3);
    let reg = usize::from(opcode & 0x0007);
    let addr = arm.registers[base_reg].wrapping_add(arm.registers[offset_reg]);

    if sign {
        if hi {
            // LDRSH
            let mut v = u32::from(arm.read16(addr, false)?);
            if v & 0x8000 == 0x8000 {
                v |= 0xffff_0000;
            }
            arm.registers[reg] = v;
        } else {
            // LDRSB
            let mut v = u32::from(arm.read8(addr)?);
            if v & 0x0080 == 0x0080 {
                v |= 0xffff_ff00;
            }
            arm.registers[reg] = v;
        }
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else if hi {
        // LDRH
        arm.registers[reg] = u32::from(arm.read16(addr, false)?);
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else {
        // STRH
        arm.write16(addr, arm.registers[reg] as u16, false)?;
        arm.store_register_cycles(addr);
    }

    Ok(Some(()))
}

/// Format 9 — Load/store with immediate offset.
fn load_store_with_imm_offset<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let byte_transfer = opcode & 0x1000 == 0x1000;
    let mut offset = u32::from((opcode & 0x07c0) >> 6);
    let base_reg = usize::from((opcode & 0x0038) >> 3);
    let reg = usize::from(opcode & 0x0007);
    if !byte_transfer {
        offset <<= 2;
    }
    let addr = arm.registers[base_reg].wrapping_add(offset);

    if load {
        if byte_transfer {
            arm.registers[reg] = u32::from(arm.read8(addr)?);
        } else {
            arm.registers[reg] = arm.read32(addr, false)?;
        }
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else if byte_transfer {
        arm.write8(addr, arm.registers[reg] as u8)?;
        arm.store_register_cycles(addr);
    } else {
        arm.write32(addr, arm.registers[reg], false)?;
        arm.store_register_cycles(addr);
    }

    Ok(Some(()))
}

/// Format 10 — Load/store halfword.
fn load_store_halfword<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let offset = u32::from((opcode & 0x07c0) >> 6) << 1;
    let base_reg = usize::from((opcode & 0x0038) >> 3);
    let reg = usize::from(opcode & 0x0007);
    let addr = arm.registers[base_reg].wrapping_add(offset);

    if load {
        arm.registers[reg] = u32::from(arm.read16(addr, false)?);
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else {
        arm.write16(addr, arm.registers[reg] as u16, false)?;
        arm.store_register_cycles(addr);
    }

    Ok(Some(()))
}

/// Format 11 — SP-relative load/store.
fn sp_relative_load_store<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let reg = usize::from((opcode & 0x07ff) >> 8);
    let offset = u32::from(opcode & 0xff) << 2;
    let addr = arm.registers[REG_SP].wrapping_add(offset);

    if load {
        arm.registers[reg] = arm.read32(addr, false)?;
        arm.ncycle(BusAccess::DataRead, addr);
        arm.icycle();
    } else {
        arm.write32(addr, arm.registers[reg], false)?;
        arm.store_register_cycles(addr);
    }

    Ok(Some(()))
}

/// Format 12 — Load address (`ADD Rd, PC/SP, #imm`).
fn load_address<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let sp = opcode & 0x0800 == 0x0800;
    let dest_reg = usize::from((opcode & 0x0700) >> 8);
    let offset = u32::from(opcode & 0x00ff) << 2;

    if sp {
        arm.registers[dest_reg] = arm.registers[REG_SP].wrapping_add(offset);
    } else {
        // "Where the PC is used as the source register (SP = 0), bit 1 of
        // the PC is always read as 0."
        let pc = (arm.registers[REG_PC] & 0xffff_fffd).wrapping_add(offset);
        arm.registers[dest_reg] = align_to_32bits(pc);
    }

    Ok(Some(()))
}

/// Format 13 — Add offset to stack pointer.
fn add_offset_to_sp<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let sign = opcode & 0x80 == 0x80;
    let imm = u32::from(opcode & 0x7f) << 2;

    if sign {
        arm.registers[REG_SP] = arm.registers[REG_SP].wrapping_sub(imm);
    } else {
        arm.registers[REG_SP] = arm.registers[REG_SP].wrapping_add(imm);
    }

    Ok(Some(()))
}

/// Format 14 — Push/pop registers (`R0..=R7` plus `LR`/`PC`).
fn push_pop_registers<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let pclr = opcode & 0x0100 == 0x0100;
    let reg_list = (opcode & 0x00ff) as u8;

    if load {
        let mut addr = arm.registers[REG_SP];
        let mut num_matches = 0u32;
        for i in 0..=7usize {
            let m = 1u8 << i;
            if reg_list & m == m {
                num_matches += 1;
                if num_matches == 1 {
                    arm.ncycle(BusAccess::DataRead, addr);
                } else {
                    arm.scycle(BusAccess::DataRead, addr);
                }
                arm.registers[i] = arm.read32(addr, true)?;
                addr = addr.wrapping_add(4);
            }
        }

        if pclr {
            num_matches += 1;
            if num_matches == 1 {
                arm.ncycle(BusAccess::DataRead, addr);
            } else {
                arm.scycle(BusAccess::DataRead, addr);
            }
            let v = arm.read32(addr, true)? & 0xffff_fffe;
            arm.registers[REG_PC] = v.wrapping_add(2);
            addr = addr.wrapping_add(4);
        }

        arm.icycle();
        arm.registers[REG_SP] = addr;
    } else {
        let c = if pclr {
            (u32::from(reg_list.count_ones()) + 1) * 4
        } else {
            u32::from(reg_list.count_ones()) * 4
        };
        let mut addr = arm.registers[REG_SP].wrapping_sub(c);
        let mut num_matches = 0u32;

        for i in 0..=7usize {
            let m = 1u8 << i;
            if reg_list & m == m {
                num_matches += 1;
                if num_matches == 1 {
                    arm.store_register_cycles(addr);
                } else {
                    arm.scycle(BusAccess::DataWrite, addr);
                }
                arm.write32(addr, arm.registers[i], true)?;
                addr = addr.wrapping_add(4);
            }
        }

        if pclr {
            num_matches += 1;
            let lr = arm.registers[REG_LR];
            arm.write32(addr, lr, true)?;
            if num_matches == 1 {
                arm.store_register_cycles(addr);
            } else {
                arm.scycle(BusAccess::DataWrite, addr);
            }
        }

        arm.registers[REG_SP] = arm.registers[REG_SP].wrapping_sub(c);
    }

    Ok(Some(()))
}

/// Format 15 — Multiple load/store (`LDMIA`/`STMIA`). The 8-bit register
/// list can only ever select `R0..=R7` (real Thumb-1 hardware has no way to
/// encode `R8..=R15` in this format), matching the reference's own
/// `regList` field width.
fn multiple_load_store<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let load = opcode & 0x0800 == 0x0800;
    let base_reg = usize::from((opcode & 0x07ff) >> 8);
    let reg_list = (opcode & 0x00ff) as u8;
    let mut addr = arm.registers[base_reg];

    if load {
        // ARM references disagree on whether the base register should be
        // updated when it's also part of the loaded list; Gopher2600's own
        // comment cites an observed real-hardware case where it should NOT
        // be, so that's preserved here.
        let mut update_base_reg = true;
        let mut num_matches = 0u32;

        for i in 0..=7usize {
            let m = 1u8 << i;
            if reg_list & m == m {
                if i == base_reg {
                    update_base_reg = false;
                }
                num_matches += 1;
                if num_matches == 1 {
                    arm.ncycle(BusAccess::DataWrite, addr);
                } else {
                    arm.scycle(BusAccess::DataWrite, addr);
                }
                arm.registers[i] = arm.read32(addr, true)?;
                addr = addr.wrapping_add(4);
            }
        }

        arm.icycle();
        if update_base_reg {
            arm.registers[base_reg] = addr;
        }
    } else {
        let mut num_matches = 0u32;
        for i in 0..=7usize {
            let m = 1u8 << i;
            if reg_list & m == m {
                num_matches += 1;
                if num_matches == 1 {
                    arm.store_register_cycles(addr);
                } else {
                    arm.scycle(BusAccess::DataWrite, addr);
                }
                arm.write32(addr, arm.registers[i], true)?;
                addr = addr.wrapping_add(4);
            }
        }
        arm.registers[base_reg] = addr;
    }

    Ok(Some(()))
}

/// Format 16 — Conditional branch.
fn conditional_branch<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let cond = ((opcode & 0x0f00) >> 8) as u8;
    let mut offset = u32::from(opcode & 0x00ff) << 1;
    if offset & 0x100 == 0x100 {
        offset |= 0xffff_ff00;
    }
    offset = offset.wrapping_add(2);

    if arm.status.condition(cond) {
        arm.registers[REG_PC] = arm.registers[REG_PC].wrapping_add(offset);
    }

    Ok(Some(()))
}

/// Format 17 — Software interrupt. Unimplemented in the reference too
/// (`panic`s there); reported as a fault here instead, so cartridge-supplied
/// bytecode can never crash the interpreter.
fn software_interrupt<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>) -> StepResult {
    Err(Fault::UnimplementedPeripheral(arm.instruction_pc))
}

/// Format 18 — Unconditional branch.
fn unconditional_branch<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let mut offset = u32::from(opcode & 0x07ff) << 1;
    if offset & 0x800 == 0x800 {
        offset |= 0xffff_f800;
    }
    offset = offset.wrapping_add(2);
    arm.registers[REG_PC] = arm.registers[REG_PC].wrapping_add(offset);

    Ok(Some(()))
}

/// Format 19 — Long branch with link (`BL`), encoded as two consecutive
/// 16-bit halves.
fn long_branch_with_link<M: ThumbMemory>(arm: &mut Arm7Tdmi<M>, opcode: u16) -> StepResult {
    let low = opcode & 0x800 == 0x800;
    let offset = u32::from(opcode & 0x07ff);

    if low {
        let offset = offset << 1;
        let tgt = arm.registers[REG_LR].wrapping_add(offset);
        let pc = arm.registers[REG_PC];
        arm.registers[REG_PC] = tgt;
        arm.registers[REG_LR] = pc.wrapping_sub(1);
    } else {
        let mut offset = offset << 12;
        if offset & 0x0040_0000 == 0x0040_0000 {
            offset |= 0xffc0_0000;
        }
        offset = offset.wrapping_add(2);
        arm.registers[REG_LR] = arm.registers[REG_PC].wrapping_add(offset);
    }

    Ok(Some(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StepOutcome;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A flat 64 KiB backing store mapped at every address — bounds are
    /// enforced by `memory.rs`'s slice indexing, not by this harness.
    struct TestMemory {
        mem: Vec<u8>,
        sp: u32,
        lr: u32,
        pc: u32,
    }

    impl TestMemory {
        fn new(code: &[u16]) -> Self {
            let mut mem = vec![0u8; 0x1_0000];
            for (i, opcode) in code.iter().enumerate() {
                let addr = i * 2;
                let bytes = opcode.to_le_bytes();
                mem[addr] = bytes[0];
                mem[addr + 1] = bytes[1];
            }
            Self {
                mem,
                sp: 0xff00,
                lr: 0x1ffe,
                pc: 0x0000,
            }
        }
    }

    impl ThumbMemory for TestMemory {
        fn map(&mut self, _addr: u32, _write: bool, _executing: bool) -> Option<(&mut [u8], u32)> {
            Some((&mut self.mem, 0))
        }
        fn reset_vectors(&self) -> (u32, u32, u32) {
            (self.sp, self.lr, self.pc)
        }
        fn is_executable(&self, _addr: u32) -> bool {
            true
        }
    }

    fn arm_with(code: &[u16]) -> Arm7Tdmi<TestMemory> {
        Arm7Tdmi::new(TestMemory::new(code))
    }

    fn step_normal(arm: &mut Arm7Tdmi<TestMemory>) -> u32 {
        let (outcome, cycles) = arm.step();
        assert_eq!(outcome, StepOutcome::Normal);
        cycles
    }

    // -- Opcode encoders, mirroring each format's field layout exactly (see
    // the corresponding decode function above for the authoritative bit
    // positions) so tests can't silently hand-compute a wrong hex literal.

    const fn fmt1(op: u16, shift: u16, src: u16, dest: u16) -> u16 {
        (op << 11) | (shift << 6) | (src << 3) | dest
    }
    const fn fmt2(immediate: bool, subtract: bool, imm_or_reg: u16, src: u16, dest: u16) -> u16 {
        0x1800
            | ((immediate as u16) << 10)
            | ((subtract as u16) << 9)
            | (imm_or_reg << 6)
            | (src << 3)
            | dest
    }
    const fn fmt3(op: u16, dest: u16, imm: u16) -> u16 {
        0x2000 | (op << 11) | (dest << 8) | imm
    }
    const fn fmt4(op: u16, src: u16, dest: u16) -> u16 {
        0x4000 | (op << 6) | (src << 3) | dest
    }
    const fn fmt5(op: u16, hi1: bool, hi2: bool, src: u16, dest: u16) -> u16 {
        0x4400 | (op << 8) | ((hi1 as u16) << 7) | ((hi2 as u16) << 6) | (src << 3) | dest
    }
    const fn fmt6(dest: u16, imm: u16) -> u16 {
        0x4800 | (dest << 8) | imm
    }
    const fn fmt7(load: bool, byte: bool, offset_reg: u16, base: u16, reg: u16) -> u16 {
        0x5000
            | ((load as u16) << 11)
            | ((byte as u16) << 10)
            | (offset_reg << 6)
            | (base << 3)
            | reg
    }
    const fn fmt8(hi: bool, sign: bool, offset_reg: u16, base: u16, reg: u16) -> u16 {
        0x5200 | ((hi as u16) << 11) | ((sign as u16) << 10) | (offset_reg << 6) | (base << 3) | reg
    }
    const fn fmt9(load: bool, byte: bool, offset: u16, base: u16, reg: u16) -> u16 {
        0x6000 | ((byte as u16) << 12) | ((load as u16) << 11) | (offset << 6) | (base << 3) | reg
    }
    const fn fmt10(load: bool, offset: u16, base: u16, reg: u16) -> u16 {
        0x8000 | ((load as u16) << 11) | (offset << 6) | (base << 3) | reg
    }
    const fn fmt11(load: bool, reg: u16, offset: u16) -> u16 {
        0x9000 | ((load as u16) << 11) | (reg << 8) | offset
    }
    const fn fmt12(sp: bool, dest: u16, offset: u16) -> u16 {
        0xa000 | ((sp as u16) << 11) | (dest << 8) | offset
    }
    const fn fmt13(sign: bool, imm: u16) -> u16 {
        0xb000 | ((sign as u16) << 7) | imm
    }
    const fn fmt14(load: bool, pclr: bool, reg_list: u16) -> u16 {
        0xb400 | ((load as u16) << 11) | ((pclr as u16) << 8) | reg_list
    }
    const fn fmt15(load: bool, base_reg: u16, reg_list: u16) -> u16 {
        0xc000 | ((load as u16) << 11) | (base_reg << 8) | reg_list
    }
    const fn fmt16(cond: u16, offset: u16) -> u16 {
        0xd000 | (cond << 8) | offset
    }
    const fn fmt18(offset11: u16) -> u16 {
        0xe000 | offset11
    }
    const fn fmt19(low: bool, offset11: u16) -> u16 {
        0xf000 | ((low as u16) << 11) | offset11
    }

    #[test]
    fn format1_lsl_by_immediate() {
        let mut arm = arm_with(&[fmt1(0b00, 4, 1, 0)]); // LSL R0, R1, #4
        arm.set_register(1, 0x0000_0001);
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0x10);
        assert!(!arm.status().contains(Status::CARRY));
        assert!(!arm.status().contains(Status::ZERO));
    }

    #[test]
    fn format1_lsr_by_immediate_sets_carry_from_shifted_out_bit() {
        let mut arm = arm_with(&[fmt1(0b01, 1, 3, 2)]); // LSR R2, R3, #1
        arm.set_register(3, 0x0000_0001);
        step_normal(&mut arm);
        assert_eq!(arm.register(2), 0);
        assert!(arm.status().contains(Status::CARRY));
        assert!(arm.status().contains(Status::ZERO));
    }

    #[test]
    fn format1_asr_by_zero_sign_extends_from_carry() {
        let mut arm = arm_with(&[fmt1(0b10, 0, 1, 0)]); // ASR R0, R1, #0
        arm.set_register(1, 0x8000_0000);
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0xffff_ffff);
        assert!(arm.status().contains(Status::CARRY));
        assert!(arm.status().contains(Status::NEGATIVE));
    }

    #[test]
    fn format2_add_register_computes_flags() {
        let mut arm = arm_with(&[fmt2(false, false, 2, 1, 0)]); // ADD R0, R1, R2
        arm.set_register(1, 5);
        arm.set_register(2, 7);
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 12);
        assert!(!arm.status().contains(Status::CARRY));
        assert!(!arm.status().contains(Status::OVERFLOW));
    }

    #[test]
    fn format2_sub_immediate_clears_carry_on_borrow() {
        let mut arm = arm_with(&[fmt2(true, true, 5, 4, 3)]); // SUB R3, R4, #5
        arm.set_register(4, 3);
        step_normal(&mut arm);
        assert_eq!(arm.register(3), 3u32.wrapping_sub(5));
        assert!(!arm.status().contains(Status::CARRY), "3 - 5 borrows");
        assert!(arm.status().contains(Status::NEGATIVE));
    }

    #[test]
    fn format3_mov_immediate() {
        let mut arm = arm_with(&[fmt3(0b00, 5, 0x42)]); // MOV R5, #0x42
        step_normal(&mut arm);
        assert_eq!(arm.register(5), 0x42);
    }

    #[test]
    fn format3_cmp_immediate_sets_zero_on_equality() {
        let mut arm = arm_with(&[fmt3(0b01, 6, 0x10)]); // CMP R6, #0x10
        arm.set_register(6, 0x10);
        step_normal(&mut arm);
        assert_eq!(arm.register(6), 0x10, "CMP must not modify its register");
        assert!(arm.status().contains(Status::ZERO));
    }

    #[test]
    fn format4_and_masks_registers() {
        let mut arm = arm_with(&[fmt4(0b0000, 1, 0)]); // AND R0, R1
        arm.set_register(0, 0xff);
        arm.set_register(1, 0x0f);
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0x0f);
    }

    #[test]
    fn format4_mul_multiplies_and_sets_zero() {
        let mut arm = arm_with(&[fmt4(0b1101, 2, 1)]); // MUL R1, R2
        arm.set_register(1, 6);
        arm.set_register(2, 7);
        step_normal(&mut arm);
        assert_eq!(arm.register(1), 42);
        assert!(!arm.status().contains(Status::ZERO));
    }

    #[test]
    fn format4_ror_by_register_rotates() {
        let mut arm = arm_with(&[fmt4(0b0111, 1, 0)]); // ROR R0, R1
        arm.set_register(0, 0x0000_0001);
        arm.set_register(1, 4); // rotate right by 4
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0x1000_0000);
    }

    #[test]
    fn format5_bx_to_expected_return_address_ends_program() {
        // BX LR, with LR left at its reset-vector value (0x1ffe), so
        // new_pc = (0x1ffe + 2) & !1 == expected_return_address.
        let mut arm = arm_with(&[fmt5(0b11, false, true, 6, 0)]); // BX LR (LR = R14, hi2 -> src=6+8=14)
        let (outcome, _) = arm.step();
        assert_eq!(outcome, StepOutcome::ProgramEnded);
    }

    #[test]
    fn format5_mov_hi_register() {
        let mut arm = arm_with(&[fmt5(0b10, true, false, 1, 0)]); // MOV R8, R1 (hi1 -> dest=0+8=8)
        arm.set_register(1, 0x1234);
        step_normal(&mut arm);
        assert_eq!(arm.register(8), 0x1234);
    }

    #[test]
    fn format6_pc_relative_load() {
        // LDR R0, [PC, #0]. During execution PC reads as `fetch_addr + 4`
        // (already word-aligned here), so with imm=0 the literal must sit
        // at word offset 4 from the instruction — one padding halfword
        // (never executed) is needed to reach that alignment, exactly as a
        // real Thumb assembler would insert.
        let mut arm = arm_with(&[fmt6(0, 0), 0x0000, 0xdead, 0xbeef]);
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0xbeef_dead);
    }

    #[test]
    fn format7_load_store_with_register_offset_round_trips() {
        let mut arm = arm_with(&[
            fmt7(false, false, 2, 1, 0), // STR R0, [R1, R2]
            fmt7(true, false, 2, 1, 3),  // LDR R3, [R1, R2]
        ]);
        arm.set_register(0, 0xdead_beef);
        arm.set_register(1, 0x2000);
        arm.set_register(2, 0x10);
        step_normal(&mut arm);
        step_normal(&mut arm);
        assert_eq!(arm.register(3), 0xdead_beef);
    }

    #[test]
    fn format8_load_sign_extended_byte() {
        let mut arm = arm_with(&[
            fmt9(false, true, 0, 1, 0), // STRB R0, [R1, #0]
            fmt8(false, true, 2, 1, 3), // LDSB R3, [R1, R2] (R2 = 0)
        ]);
        arm.set_register(0, 0x80); // negative as a signed byte
        arm.set_register(1, 0x2000);
        arm.set_register(2, 0);
        step_normal(&mut arm);
        step_normal(&mut arm);
        assert_eq!(arm.register(3), 0xffff_ff80);
    }

    #[test]
    fn format9_load_store_with_immediate_offset_round_trips() {
        let mut arm = arm_with(&[
            fmt9(false, false, 3, 1, 0), // STR R0, [R1, #12]
            fmt9(true, false, 3, 1, 2),  // LDR R2, [R1, #12]
        ]);
        arm.set_register(0, 0x1122_3344);
        arm.set_register(1, 0x2000);
        step_normal(&mut arm);
        step_normal(&mut arm);
        assert_eq!(arm.register(2), 0x1122_3344);
    }

    #[test]
    fn format10_load_store_halfword_round_trips() {
        let mut arm = arm_with(&[
            fmt10(false, 2, 1, 0), // STRH R0, [R1, #4]
            fmt10(true, 2, 1, 3),  // LDRH R3, [R1, #4]
        ]);
        arm.set_register(0, 0x0000_beef);
        arm.set_register(1, 0x2000);
        step_normal(&mut arm);
        step_normal(&mut arm);
        assert_eq!(arm.register(3), 0xbeef);
    }

    #[test]
    fn format11_sp_relative_load_store_round_trips() {
        let mut arm = arm_with(&[
            fmt11(false, 0, 4), // STR R0, [SP, #16]
            fmt11(true, 1, 4),  // LDR R1, [SP, #16]
        ]);
        arm.set_register(0, 0xcafe_babe);
        step_normal(&mut arm);
        step_normal(&mut arm);
        assert_eq!(arm.register(1), 0xcafe_babe);
    }

    #[test]
    fn format12_load_address_from_sp() {
        let mut arm = arm_with(&[fmt12(true, 0, 2)]); // ADD R0, SP, #8
        step_normal(&mut arm);
        assert_eq!(arm.register(0), arm.register(REG_SP) + 8);
    }

    #[test]
    fn format13_add_offset_to_sp_both_signs() {
        let mut arm = arm_with(&[fmt13(false, 2), fmt13(true, 2)]); // ADD SP,#8 ; ADD SP,#-8
        let original_sp = arm.register(REG_SP);
        step_normal(&mut arm);
        assert_eq!(arm.register(REG_SP), original_sp + 8);
        step_normal(&mut arm);
        assert_eq!(arm.register(REG_SP), original_sp);
    }

    #[test]
    fn format14_push_pop_round_trips_including_link_register() {
        let mut arm = arm_with(&[
            fmt14(false, true, 0b0000_0011), // PUSH {R0, R1, LR}
            fmt14(true, true, 0b0000_0011),  // POP {R0, R1, PC}
        ]);
        arm.set_register(0, 0x1111_1111);
        arm.set_register(1, 0x2222_2222);
        let original_sp = arm.register(REG_SP);
        step_normal(&mut arm); // PUSH
        assert_eq!(arm.register(REG_SP), original_sp - 12);
        arm.set_register(0, 0);
        arm.set_register(1, 0);
        let (outcome, _) = arm.step(); // POP {R0, R1, PC} -> branches via popped PC
        assert_eq!(outcome, StepOutcome::Normal);
        assert_eq!(arm.register(0), 0x1111_1111);
        assert_eq!(arm.register(1), 0x2222_2222);
        assert_eq!(arm.register(REG_SP), original_sp);
    }

    #[test]
    fn format15_multiple_load_store_round_trips() {
        let mut arm = arm_with(&[
            fmt15(false, 2, 0b0000_0011), // STMIA R2!, {R0, R1}
            fmt15(true, 3, 0b0000_1100),  // LDMIA R3!, {R2, R3} (base reg R3 is also in the list)
        ]);
        arm.set_register(0, 0xaaaa_aaaa);
        arm.set_register(1, 0xbbbb_bbbb);
        arm.set_register(2, 0x3000);
        step_normal(&mut arm); // STMIA R2!, {R0, R1} -> writes at 0x3000, 0x3004; R2 becomes 0x3008
        assert_eq!(arm.register(2), 0x3008);
        arm.set_register(3, 0x3000);
        let (outcome, _) = arm.step(); // LDMIA R3!, {R2, R3}
        assert_eq!(outcome, StepOutcome::Normal);
        assert_eq!(arm.register(2), 0xaaaa_aaaa);
    }

    #[test]
    fn format16_conditional_branch_taken() {
        // CMP R0, R0 sets Z; BEQ #0 branches to (branch_addr + 4) per the
        // ARM7TDMI's pipeline-relative branch formula, which — because the
        // very next sequential instruction is only at (branch_addr + 2) —
        // lands one instruction further on, skipping the first MOV.
        let mut arm = arm_with(&[
            fmt4(0b1010, 0, 0),
            fmt16(0b0000, 0),
            fmt3(0, 1, 0xaa),
            fmt3(0, 1, 0xbb),
        ]);
        step_normal(&mut arm); // CMP R0, R0
        assert!(arm.status().contains(Status::ZERO));
        step_normal(&mut arm); // BEQ (taken)
        step_normal(&mut arm); // lands on the second MOV, skipping the first
        assert_eq!(arm.register(1), 0xbb);
    }

    #[test]
    fn format16_conditional_branch_not_taken() {
        let mut arm = arm_with(&[
            fmt4(0b1010, 1, 0),
            fmt16(0b0000, 1),
            fmt3(0, 1, 0xaa),
            fmt3(0, 1, 0xbb),
        ]);
        arm.set_register(1, 1); // ensure R0 != R1 so CMP clears Z
        step_normal(&mut arm); // CMP R0, R1
        assert!(!arm.status().contains(Status::ZERO));
        step_normal(&mut arm); // BEQ (not taken)
        step_normal(&mut arm); // falls through to the first MOV
        assert_eq!(arm.register(1), 0xaa);
    }

    #[test]
    fn format17_software_interrupt_faults_rather_than_panics() {
        let mut arm = arm_with(&[0xdf00]); // SWI #0
        let (outcome, _) = arm.step();
        assert_eq!(
            outcome,
            StepOutcome::Fault(Fault::UnimplementedPeripheral(0))
        );
    }

    #[test]
    fn format18_unconditional_branch() {
        // B #0 branches to (branch_addr + 4), which — same pipeline-relative
        // reasoning as the conditional-branch test above — skips the first MOV.
        let mut arm = arm_with(&[fmt18(0), fmt3(0, 0, 0xaa), fmt3(0, 0, 0xbb)]);
        step_normal(&mut arm); // branch, skipping the first MOV
        step_normal(&mut arm);
        assert_eq!(arm.register(0), 0xbb);
    }

    #[test]
    fn format19_long_branch_with_link() {
        // Real ARM7TDMI BL semantics: target = entry_pc + 4 (the standard
        // Thumb "PC reads as instruction + 4" convention, here anchored to
        // the FIRST of the two halfwords) + (offset_low << 1); LR becomes
        // the address immediately after the BL pair, with bit 0 set.
        let mut arm = arm_with(&[fmt19(false, 0), fmt19(true, 4)]);
        let entry_pc = arm.register(REG_PC);
        step_normal(&mut arm); // high half: LR = entry_pc + 4 + (offset_high << 12)
        step_normal(&mut arm); // low half: PC = LR + (offset_low << 1); LR = return addr | 1
        assert_eq!(arm.register(REG_PC), entry_pc.wrapping_add(4 + 8));
        assert_eq!(
            arm.register(REG_LR),
            entry_pc.wrapping_add(4) | 1,
            "LR is the address after the BL pair, with bit 0 set"
        );
    }
}
