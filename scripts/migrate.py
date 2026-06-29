import re

with open("/home/parobek/Code/OSS_Public-Projects/RustyNES/crates/rustynes-cpu/src/cpu.rs", "r") as f:
    text = f.read()

# Extract everything from `fn addr_zp` down to the end of the file.
start_idx = text.find("    fn addr_zp<B: Bus>")
end_idx = text.rfind("}")
helpers_and_dispatch = text[start_idx:end_idx]

# Replace `Bus` with `CpuBus`
helpers_and_dispatch = helpers_and_dispatch.replace("<B: Bus>", "<B: CpuBus>")
helpers_and_dispatch = helpers_and_dispatch.replace("B: Bus", "B: CpuBus")

# Inject BCD into adc and sbc
adc_pattern = re.compile(r"    fn adc\(&mut self, value: u8\) \{.*?(?=    fn sbc)", re.DOTALL)
adc_replacement = """    fn adc(&mut self, value: u8) {
        if self.p.contains(Status::DECIMAL) {
            let mut al = (self.a & 0x0F) + (value & 0x0F) + (self.p.contains(Status::CARRY) as u8);
            let mut ah = (self.a >> 4) + (value >> 4) + (if al > 0x09 { 1 } else { 0 });
            
            // Flags N, V, Z are set based on the binary result on NMOS 6502
            let bin_sum = (self.a as u16) + (value as u16) + (self.p.contains(Status::CARRY) as u16);
            let bin_result = bin_sum as u8;
            self.p.set(Status::ZERO, bin_result == 0);
            self.p.set(Status::NEGATIVE, (bin_result & 0x80) != 0);
            self.p.set(Status::OVERFLOW, ((self.a ^ bin_result) & (value ^ bin_result) & 0x80) != 0);
            
            if al > 0x09 { al = al.wrapping_add(0x06); }
            if ah > 0x09 { ah = ah.wrapping_add(0x06); }
            self.p.set(Status::CARRY, ah > 0x0F);
            self.a = (ah << 4) | (al & 0x0F);
        } else {
            let carry = u16::from(self.p.contains(Status::CARRY));
            let sum = u16::from(self.a) + u16::from(value) + carry;
            let result = sum as u8;
            self.p.set(Status::CARRY, sum > 0xFF);
            let overflow = ((self.a ^ result) & (value ^ result) & 0x80) != 0;
            self.p.set(Status::OVERFLOW, overflow);
            self.a = result;
            self.p.set_nz(self.a);
        }
    }
"""

sbc_pattern = re.compile(r"    fn sbc\(&mut self, value: u8\) \{.*?(?=    fn cmp_with)", re.DOTALL)
sbc_replacement = """    fn sbc(&mut self, value: u8) {
        if self.p.contains(Status::DECIMAL) {
            let carry = self.p.contains(Status::CARRY) as u8;
            let bin_sum = (self.a as u16) + ((value ^ 0xFF) as u16) + (carry as u16);
            let bin_result = bin_sum as u8;
            
            self.p.set(Status::ZERO, bin_result == 0);
            self.p.set(Status::NEGATIVE, (bin_result & 0x80) != 0);
            self.p.set(Status::OVERFLOW, ((self.a ^ bin_result) & ((value ^ 0xFF) ^ bin_result) & 0x80) != 0);
            
            let mut al = (self.a & 0x0F).wrapping_sub(value & 0x0F).wrapping_sub(1 - carry);
            let mut ah = (self.a >> 4).wrapping_sub(value >> 4);
            if (al as i8) < 0 {
                al = al.wrapping_sub(0x06);
                ah = ah.wrapping_sub(1);
            }
            if (ah as i8) < 0 {
                ah = ah.wrapping_sub(0x06);
                self.p.set(Status::CARRY, false);
            } else {
                self.p.set(Status::CARRY, true);
            }
            self.a = (ah << 4) | (al & 0x0F);
        } else {
            self.adc(value ^ 0xFF);
        }
    }
"""

helpers_and_dispatch = adc_pattern.sub(adc_replacement, helpers_and_dispatch)
helpers_and_dispatch = sbc_pattern.sub(sbc_replacement, helpers_and_dispatch)

# Replace branch method entirely
branch_pattern = re.compile(r"    fn branch<B: CpuBus>\(&mut self, bus: &mut B, offset: u8, condition: bool\) -> u8 \{.*?(?=    fn lax)", re.DOTALL)
branch_replacement = """    fn branch<B: CpuBus>(&mut self, bus: &mut B, offset: u8, condition: bool) -> u8 {
        if !condition {
            return 2;
        }
        let _ = self.read1(bus, self.pc); // C3 dummy
        let signed = offset as i8 as i16;
        let old_pc = self.pc;
        let new_pc = (self.pc as i32 + i32::from(signed)) as u16;
        let crossed = (old_pc & 0xFF00) != (new_pc & 0xFF00);
        if crossed {
            let dummy = (old_pc & 0xFF00) | (new_pc & 0x00FF);
            let _ = self.read1(bus, dummy);
        }
        self.pc = new_pc;
        if crossed { 4 } else { 3 }
    }
"""
helpers_and_dispatch = branch_pattern.sub(branch_replacement, helpers_and_dispatch)

# Remove `self.skip_irq_sample = true;`
helpers_and_dispatch = helpers_and_dispatch.replace("self.skip_irq_sample = true;", "")

# Replace BRK arm
brk_pattern = re.compile(r"(0x00 => \{.*?)\}(?=            0x01)", re.DOTALL)
brk_replacement = """0x00 => { // BRK
                let _ = self.read1(bus, self.pc); // dummy fetch
                self.pc = self.pc.wrapping_add(1);
                self.push_u16(bus, self.pc);
                let mut p = self.p;
                p.insert(Status::BREAK);
                self.push(bus, p.bits());
                self.p.insert(Status::INTERRUPT_DISABLE);
                let lo = self.read1(bus, 0xFFFE);
                let hi = self.read1(bus, 0xFFFF);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
                *cycles = 7;
            }"""
helpers_and_dispatch = brk_pattern.sub(brk_replacement, helpers_and_dispatch)

# Remove `irq_sample_i_flag` update in RTI
helpers_and_dispatch = re.sub(r"\s*self\.irq_sample_i_flag = self\.p\.contains\(Status::INTERRUPT_DISABLE\);", "", helpers_and_dispatch)

header = """//! `rusty2600-cpu` — MOS 6507 (the Atari 2600 / VCS CPU).
//!
//! The 6507 is a cost-reduced 6502 in a 28-pin DIP: the core 6502 instruction
//! decode + register file are unchanged, but the package brings out only **13
//! address pins (A0..=A12 → 8 KiB visible)** and **no IRQ / NMI pins are wired**
//! (the VCS has nothing to drive them). RDY *is* wired, and the TIA's WSYNC
//! beam-stall asserts it — see the scheduler in `rusty2600-core`.

#![no_std]
#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
extern crate alloc;

use bitflags::bitflags;

bitflags! {
    /// The 6502 processor status register `P`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Status: u8 {
        /// Carry.
        const CARRY = 0b0000_0001;
        /// Zero.
        const ZERO = 0b0000_0010;
        /// Interrupt-disable (inert on the 2600 — no IRQ line is wired).
        const INTERRUPT_DISABLE = 0b0000_0100;
        /// Decimal mode (BCD `ADC`/`SBC`; the 6502 implements it, unlike the 2A03).
        const DECIMAL = 0b0000_1000;
        /// Break (set only in the pushed copy).
        const BREAK = 0b0001_0000;
        /// Unused — reads as 1.
        const UNUSED = 0b0010_0000;
        /// Overflow.
        const OVERFLOW = 0b0100_0000;
        /// Negative.
        const NEGATIVE = 0b1000_0000;
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::UNUSED | Self::INTERRUPT_DISABLE
    }
}

impl Status {
    pub const fn power_on() -> Self {
        Self::from_bits_truncate(Self::INTERRUPT_DISABLE.bits() | Self::UNUSED.bits())
    }
    pub fn set_nz(&mut self, value: u8) {
        self.set(Status::ZERO, value == 0);
        self.set(Status::NEGATIVE, (value & 0x80) != 0);
    }
}

pub trait CpuBus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

const STACK_BASE: u16 = 0x0100;
const RESET_VECTOR: u16 = 0xFFFC;

#[derive(Debug, Clone)]
pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub pc: u16,
    pub s: u8,
    pub p: Status,
    pub cycles: u64,
    pub jammed: bool,
    pub(crate) cycles_emitted: u8,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct Operand {
    addr: u16,
    page_crossed: bool,
}

impl Cpu {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            pc: 0,
            s: 0xFD,
            p: Status::power_on(),
            cycles: 0,
            jammed: false,
            cycles_emitted: 0,
        }
    }

    #[must_use]
    pub const fn power_on() -> Self {
        let mut cpu = Self::new();
        cpu.s = 0x00;
        cpu
    }

    pub fn reset<B: CpuBus>(&mut self, bus: &mut B) {
        self.s = self.s.wrapping_sub(3);
        self.p.insert(Status::INTERRUPT_DISABLE);
        self.jammed = false;
        
        self.cycles_emitted = 0;
        for _ in 0..6 {
            self.idle_tick(bus);
        }
        let lo = self.read1(bus, RESET_VECTOR);
        let hi = self.read1(bus, RESET_VECTOR + 1);
        self.pc = u16::from(lo) | (u16::from(hi) << 8);
    }

    pub const fn set_pc(&mut self, addr: u16) {
        self.pc = addr;
    }
    
    pub fn step<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        if self.jammed {
            return 0;
        }

        self.cycles_emitted = 0;

        let opcode = self.fetch_pc(bus);
        let mut cycles = 0u8;
        self.dispatch(bus, opcode, &mut cycles);
        while self.cycles_emitted < cycles {
            self.idle_tick(bus);
        }
        cycles
    }

    // Stub tick() function to maintain compatibility if anyone calls it
    pub fn tick<B: CpuBus>(&mut self, bus: &mut B) {
        self.step(bus);
    }

    fn fetch_pc<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        let v = self.read1(bus, self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn fetch_pc_u16<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch_pc(bus);
        let hi = self.fetch_pc(bus);
        u16::from(lo) | (u16::from(hi) << 8)
    }

    fn read_u16_with_wrap<B: CpuBus>(&mut self, bus: &mut B, addr: u16) -> u16 {
        let lo = self.read1(bus, addr);
        let hi_addr = (addr & 0xFF00) | u16::from((addr as u8).wrapping_add(1));
        let hi = self.read1(bus, hi_addr);
        u16::from(lo) | (u16::from(hi) << 8)
    }

    fn push<B: CpuBus>(&mut self, bus: &mut B, value: u8) {
        self.write1(bus, STACK_BASE | u16::from(self.s), value);
        self.s = self.s.wrapping_sub(1);
    }

    fn pull<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        self.s = self.s.wrapping_add(1);
        self.read1(bus, STACK_BASE | u16::from(self.s))
    }

    fn push_u16<B: CpuBus>(&mut self, bus: &mut B, value: u16) {
        self.push(bus, (value >> 8) as u8);
        self.push(bus, (value & 0xFF) as u8);
    }

    fn pull_u16<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.pull(bus);
        let hi = self.pull(bus);
        u16::from(lo) | (u16::from(hi) << 8)
    }

    fn idle_tick<B: CpuBus>(&mut self, _bus: &mut B) {
        self.cycles_emitted = self.cycles_emitted.saturating_add(1);
        self.cycles = self.cycles.wrapping_add(1);
    }

    #[inline(always)]
    fn implied_dummy_read<B: CpuBus>(&mut self, bus: &mut B) {
        let _ = self.read1(bus, self.pc);
    }

    fn read1<B: CpuBus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        self.cycles_emitted = self.cycles_emitted.saturating_add(1);
        self.cycles = self.cycles.wrapping_add(1);
        bus.read(addr & 0x1FFF)
    }

    fn write1<B: CpuBus>(&mut self, bus: &mut B, addr: u16, value: u8) {
        self.cycles_emitted = self.cycles_emitted.saturating_add(1);
        self.cycles = self.cycles.wrapping_add(1);
        bus.write(addr & 0x1FFF, value);
    }

    fn sh_store<B: CpuBus>(&mut self, bus: &mut B, base: u16, index_reg: u8, value_reg: u8) {
        let unfixed = base.wrapping_add(u16::from(index_reg));
        let page_crossed = (base & 0xFF00) != (unfixed & 0xFF00);
        let dummy_addr = if page_crossed {
            unfixed.wrapping_sub(0x0100)
        } else {
            unfixed
        };
        let _ = self.read1(bus, dummy_addr);
        
        let stored_value = value_reg & ((dummy_addr >> 8) as u8).wrapping_add(1);
        let target_addr = if page_crossed {
            (u16::from(stored_value) << 8) | (unfixed & 0x00FF)
        } else {
            unfixed
        };
        self.write1(bus, target_addr, stored_value);
    }
"""

with open("crates/rusty2600-cpu/src/lib.rs", "w") as f:
    f.write(header)
    f.write(helpers_and_dispatch)
    f.write("}\n")
    # append the tests from current lib.rs
    f.write("""
#[cfg(test)]
mod tests {
    use super::*;

    struct FlatBus {
        mem: [u8; 0x2000],
    }

    impl CpuBus for FlatBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[(addr & 0x1FFF) as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[(addr & 0x1FFF) as usize] = val;
        }
    }

    #[test]
    fn constructs() {
        let cpu = Cpu::new();
        assert_eq!(cpu.a, 0);
        assert!(cpu.p.contains(Status::UNUSED));
    }

    #[test]
    fn reset_loads_vector() {
        let mut bus = FlatBus { mem: [0; 0x2000] };
        bus.mem[0x1FFC] = 0x34;
        bus.mem[0x1FFD] = 0x12;
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.s, 0xFD);
    }
}
""")
