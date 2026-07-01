//! `rusty2600-cpu` — MOS 6507 (the Atari 2600 / VCS CPU).
//!
//! The 6507 is a cost-reduced 6502 in a 28-pin DIP: the core 6502 instruction
//! decode + register file are unchanged, but the package brings out only **13
//! address pins (A0..=A12 → 8 KiB visible)** and **no IRQ / NMI pins are wired**
//! (the VCS has nothing to drive them). RDY *is* wired, and the TIA's WSYNC
//! beam-stall asserts it — see the scheduler in `rusty2600-core`.

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
extern crate alloc;

use bitflags::bitflags;

bitflags! {
    /// The 6502 processor status register `P`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// Advance the rest of the system (TIA/RIOT/cart) by exactly one CPU
    /// cycle's worth of real time. Called once per bus access the CPU makes,
    /// so a multi-cycle instruction advances the world cycle-by-cycle instead
    /// of all at once — this is what keeps the CPU's notion of elapsed time in
    /// lockstep with the TIA's color clock. Default no-op for buses that don't
    /// model timing (flat-RAM test harnesses, the Klaus functional-test bus).
    fn tick_cycle(&mut self) {}

    /// Whether the bus is holding RDY low (the WSYNC beam-stall). The CPU
    /// spins on [`Self::tick_cycle`] while this is true instead of performing
    /// its next access. Default false for buses with no such concept.
    fn rdy_stall(&self) -> bool {
        false
    }
}

const STACK_BASE: u16 = 0x0100;
const RESET_VECTOR: u16 = 0xFFFC;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

    /// Execute one full instruction and return its cycle count.
    ///
    /// This looks like it runs "all at once," but it doesn't advance the rest
    /// of the system atomically: every bus access (and every dummy/idle cycle)
    /// inside `dispatch` funnels through [`Self::idle_tick`], which calls
    /// [`CpuBus::tick_cycle`] once per CPU cycle as it goes — so by the time
    /// this function returns, the TIA/RIOT/cart have been advanced
    /// cycle-by-cycle in step with the CPU, not all at the end. A `STA WSYNC`
    /// mid-instruction therefore takes effect at the exact color clock it's
    /// written on, and any subsequent access within the SAME instruction (rare,
    /// but possible for multi-cycle ops) sees the up-to-date world state.
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

    /// Run exactly one instruction. The caller (the scheduler's
    /// `step_instruction`) drives the outer loop; this no longer represents a
    /// single CPU cycle — see [`Self::step`]'s doc comment for how cycle-level
    /// timing is actually achieved.
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

    /// The single choke-point every bus access (and every dummy/idle cycle)
    /// goes through. This is what keeps the CPU's cycle count in lockstep with
    /// the TIA color clock: each call advances the rest of the system by
    /// exactly one CPU cycle via [`CpuBus::tick_cycle`] BEFORE the cycle's own
    /// counters move, so a multi-cycle instruction advances real time
    /// cycle-by-cycle instead of all at once when it finally returns.
    ///
    /// While [`CpuBus::rdy_stall`] is asserted (the WSYNC beam-stall), the CPU
    /// spins here — ticking the rest of the system without advancing its own
    /// cycle count — exactly matching real hardware: RDY held low freezes the
    /// CPU, but the color clock (and RIOT/cart) keep running.
    fn idle_tick<B: CpuBus>(&mut self, bus: &mut B) {
        while bus.rdy_stall() {
            bus.tick_cycle();
        }
        bus.tick_cycle();
        self.cycles_emitted = self.cycles_emitted.saturating_add(1);
        self.cycles = self.cycles.wrapping_add(1);
    }

    #[inline(always)]
    fn implied_dummy_read<B: CpuBus>(&mut self, bus: &mut B) {
        let _ = self.read1(bus, self.pc);
    }

    fn read1<B: CpuBus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        self.idle_tick(bus);
        bus.read(addr)
    }

    fn write1<B: CpuBus>(&mut self, bus: &mut B, addr: u16, value: u8) {
        self.idle_tick(bus);
        bus.write(addr, value);
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
    fn addr_zp<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        Operand {
            addr: u16::from(self.fetch_pc(bus)),
            page_crossed: false,
        }
    }

    fn addr_zp_x<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let base = self.fetch_pc(bus);
        // Zero-page-indexed addressing ALWAYS spends a cycle on a dummy read
        // at the UNINDEXED base address before the index add — unlike
        // absolute,X/Y, where the dummy read only happens on a page cross
        // (there's no "high byte to fix" here; the ALU just needs a cycle to
        // wrap the addition within the zero page). Skipping this cycle was a
        // real, systematic bug affecting every zp,X/zp,Y/(zp,X)/(zp),Y
        // opcode — confirmed against the SingleStepTests 65x02 cycle-exact
        // corpus (`crates/rusty2600-cpu/tests/singlestep_test.rs`).
        let _ = self.read1(bus, u16::from(base));
        Operand {
            addr: u16::from(base.wrapping_add(self.x)),
            page_crossed: false,
        }
    }

    fn addr_zp_y<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let base = self.fetch_pc(bus);
        // See `addr_zp_x` for the unconditional dummy-read rationale.
        let _ = self.read1(bus, u16::from(base));
        Operand {
            addr: u16::from(base.wrapping_add(self.y)),
            page_crossed: false,
        }
    }

    fn addr_abs<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        Operand {
            addr: self.fetch_pc_u16(bus),
            page_crossed: false,
        }
    }

    fn addr_abs_x<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let base = self.fetch_pc_u16(bus);
        let addr = base.wrapping_add(u16::from(self.x));
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        if page_crossed {
            // Canonical 6502 page-cross dummy read at the unfixed
            // address: (base_hi << 8) | ((base_lo + X) & 0xFF). The
            // high byte hasn't been incremented yet. This read has
            // side effects on PPU registers (`$2002` clears VBlank,
            // `$2007` advances the buffer) and is the hardware oracle
            // AccuracyCoin's `CPU Behavior :: Dummy read cycles`
            // Test 1 brackets via `LDA $20F2, X` with X=$10 reading
            // $2002 through the mirror.
            let dummy = (base & 0xFF00) | (addr & 0x00FF);
            let _ = self.read1(bus, dummy);
        }
        Operand { addr, page_crossed }
    }

    fn addr_abs_y<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let base = self.fetch_pc_u16(bus);
        let addr = base.wrapping_add(u16::from(self.y));
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        if page_crossed {
            // See addr_abs_x for the page-cross dummy-read rationale.
            let dummy = (base & 0xFF00) | (addr & 0x00FF);
            let _ = self.read1(bus, dummy);
        }
        Operand { addr, page_crossed }
    }

    // ABS,X / ABS,Y operands for read-modify-write opcodes (ASL, LSR, ROL,
    // ROR, INC, DEC, and the unofficial SLO/RLA/SRE/RRA/DCP/ISC). Canonical
    // 6502: the unfixed-address dummy read happens UNCONDITIONALLY at
    // cycle 4 (not just on page cross) because the CPU has 7 cycles to
    // fill and cannot know the fixed address until the high-byte add
    // completes. Reads with side effects (`$2002` clears VBlank, `$4015`
    // clears frame-IRQ, `$2007` advances buffer) therefore fire twice on
    // RMW ABS,X. Bracketed by AccuracyCoin's `Implied Dummy Reads`
    // test 2: `SLO $4015,X` with X=0 expects the dummy read to clear the
    // frame-IRQ flag so the subsequent real read returns 0.
    fn addr_abs_x_rmw<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let base = self.fetch_pc_u16(bus);
        let addr = base.wrapping_add(u16::from(self.x));
        let dummy = (base & 0xFF00) | (addr & 0x00FF);
        let _ = self.read1(bus, dummy);
        addr
    }

    fn addr_abs_y_rmw<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let base = self.fetch_pc_u16(bus);
        let addr = base.wrapping_add(u16::from(self.y));
        let dummy = (base & 0xFF00) | (addr & 0x00FF);
        let _ = self.read1(bus, dummy);
        addr
    }

    fn addr_ind_x<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let base = self.fetch_pc(bus);
        // Same unconditional dummy read as `addr_zp_x` — the X add onto the
        // zero-page pointer costs a cycle before the pointer is dereferenced.
        let _ = self.read1(bus, u16::from(base));
        let ptr = base.wrapping_add(self.x);
        let lo = self.read1(bus, u16::from(ptr));
        let hi = self.read1(bus, u16::from(ptr.wrapping_add(1)));
        Operand {
            addr: u16::from(lo) | (u16::from(hi) << 8),
            page_crossed: false,
        }
    }

    fn addr_ind_y<B: CpuBus>(&mut self, bus: &mut B) -> Operand {
        let ptr = self.fetch_pc(bus);
        let lo = self.read1(bus, u16::from(ptr));
        let hi = self.read1(bus, u16::from(ptr.wrapping_add(1)));
        let base = u16::from(lo) | (u16::from(hi) << 8);
        let addr = base.wrapping_add(u16::from(self.y));
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        if page_crossed {
            // Page-cross dummy read at the unfixed address — same as
            // addr_abs_x/y. Canonical 6502 behavior for LDA (zp),Y on
            // page crossing.
            let dummy = (base & 0xFF00) | (addr & 0x00FF);
            let _ = self.read1(bus, dummy);
        }
        Operand { addr, page_crossed }
    }

    // (zp),Y operand for read-modify-write opcodes (the unofficial
    // SLO/RLA/SRE/RRA/DCP/ISC). Same reasoning as `addr_abs_x_rmw`: the
    // dummy read at the unfixed address is UNCONDITIONAL, not just on page
    // cross, because these opcodes always take the extra cycle regardless.
    fn addr_ind_y_rmw<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let ptr = self.fetch_pc(bus);
        let lo = self.read1(bus, u16::from(ptr));
        let hi = self.read1(bus, u16::from(ptr.wrapping_add(1)));
        let base = u16::from(lo) | (u16::from(hi) << 8);
        let addr = base.wrapping_add(u16::from(self.y));
        let dummy = (base & 0xFF00) | (addr & 0x00FF);
        let _ = self.read1(bus, dummy);
        addr
    }

    // ------------------------------------------------------------------
    // Top-level dispatch.
    //
    // The 256-way match is the cleanest way to express the entire opcode
    // table; the doc-comments are intentionally absent at the arm level
    // because each one is a single line of the standard 6502 reference and
    // adding individual arm comments would overwhelm the readability of the
    // table.
    // ------------------------------------------------------------------

    #[allow(
        clippy::cognitive_complexity,
        clippy::too_many_lines,
        clippy::match_same_arms
    )]
    fn dispatch<B: CpuBus>(&mut self, bus: &mut B, op: u8, cycles: &mut u8) {
        match op {
            // === Loads ===
            0xA9 => {
                let v = self.fetch_pc(bus);
                self.lda(v);
                *cycles = 2;
            }
            0xA5 => {
                let o = self.addr_zp(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 3;
            }
            0xB5 => {
                let o = self.addr_zp_x(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 4;
            }
            0xAD => {
                let o = self.addr_abs(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 4;
            }
            0xBD => {
                let o = self.addr_abs_x(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xB9 => {
                let o = self.addr_abs_y(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xA1 => {
                let o = self.addr_ind_x(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 6;
            }
            0xB1 => {
                let o = self.addr_ind_y(bus);
                self.lda_addr(bus, o.addr);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0xA2 => {
                let v = self.fetch_pc(bus);
                self.ldx(v);
                *cycles = 2;
            }
            0xA6 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.ldx(v);
                *cycles = 3;
            }
            0xB6 => {
                let o = self.addr_zp_y(bus);
                let v = self.read1(bus, o.addr);
                self.ldx(v);
                *cycles = 4;
            }
            0xAE => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.ldx(v);
                *cycles = 4;
            }
            0xBE => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.ldx(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }

            0xA0 => {
                let v = self.fetch_pc(bus);
                self.ldy(v);
                *cycles = 2;
            }
            0xA4 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.ldy(v);
                *cycles = 3;
            }
            0xB4 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.ldy(v);
                *cycles = 4;
            }
            0xAC => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.ldy(v);
                *cycles = 4;
            }
            0xBC => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.ldy(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }

            // === Stores ===
            0x85 => {
                let o = self.addr_zp(bus);
                self.write1(bus, o.addr, self.a);
                *cycles = 3;
            }
            0x95 => {
                let o = self.addr_zp_x(bus);
                self.write1(bus, o.addr, self.a);
                *cycles = 4;
            }
            0x8D => {
                let o = self.addr_abs(bus);
                self.write1(bus, o.addr, self.a);
                *cycles = 4;
            }
            0x9D => {
                let o = self.addr_abs_x(bus);
                // Canonical 6502: STA absolute,X performs a dummy
                // read at cycle 4 even when no page is crossed (unlike
                // LDA where cycle 4 is the real read). `addr_abs_x`
                // already issues the dummy read at the unfixed address
                // when page-crossed; for the no-page-cross case we add
                // it here at the final address.
                if !o.page_crossed {
                    let _ = self.read1(bus, o.addr);
                }
                self.write1(bus, o.addr, self.a);
                *cycles = 5;
            }
            0x99 => {
                let o = self.addr_abs_y(bus);
                if !o.page_crossed {
                    let _ = self.read1(bus, o.addr);
                }
                self.write1(bus, o.addr, self.a);
                *cycles = 5;
            }
            0x81 => {
                let o = self.addr_ind_x(bus);
                self.write1(bus, o.addr, self.a);
                *cycles = 6;
            }
            0x91 => {
                let o = self.addr_ind_y(bus);
                // Canonical STA (zp),Y always dummy-reads at cycle 5
                // even when no page is crossed. `addr_ind_y` already
                // handles the page-cross dummy at the unfixed address;
                // add the no-page-cross dummy here at the final address.
                if !o.page_crossed {
                    let _ = self.read1(bus, o.addr);
                }
                self.write1(bus, o.addr, self.a);
                *cycles = 6;
            }

            0x86 => {
                let o = self.addr_zp(bus);
                self.write1(bus, o.addr, self.x);
                *cycles = 3;
            }
            0x96 => {
                let o = self.addr_zp_y(bus);
                self.write1(bus, o.addr, self.x);
                *cycles = 4;
            }
            0x8E => {
                let o = self.addr_abs(bus);
                self.write1(bus, o.addr, self.x);
                *cycles = 4;
            }

            0x84 => {
                let o = self.addr_zp(bus);
                self.write1(bus, o.addr, self.y);
                *cycles = 3;
            }
            0x94 => {
                let o = self.addr_zp_x(bus);
                self.write1(bus, o.addr, self.y);
                *cycles = 4;
            }
            0x8C => {
                let o = self.addr_abs(bus);
                self.write1(bus, o.addr, self.y);
                *cycles = 4;
            }

            // === Transfers ===
            0xAA => {
                self.implied_dummy_read(bus);
                self.x = self.a;
                self.p.set_nz(self.x);
                *cycles = 2;
            }
            0xA8 => {
                self.implied_dummy_read(bus);
                self.y = self.a;
                self.p.set_nz(self.y);
                *cycles = 2;
            }
            0xBA => {
                self.implied_dummy_read(bus);
                self.x = self.s;
                self.p.set_nz(self.x);
                *cycles = 2;
            }
            0x8A => {
                self.implied_dummy_read(bus);
                self.a = self.x;
                self.p.set_nz(self.a);
                *cycles = 2;
            }
            0x9A => {
                self.implied_dummy_read(bus);
                self.s = self.x;
                *cycles = 2;
            }
            0x98 => {
                self.implied_dummy_read(bus);
                self.a = self.y;
                self.p.set_nz(self.a);
                *cycles = 2;
            }

            // === Stack ===
            0x48 => {
                // PHA: C2 dummy read PC (the 6502 always reads the next byte on
                // the second cycle of a stack push), then the push.
                let _ = self.read1(bus, self.pc);
                self.push(bus, self.a);
                *cycles = 3;
            }
            0x08 => {
                let _ = self.read1(bus, self.pc);
                self.push(bus, (self.p | Status::BREAK | Status::UNUSED).bits());
                *cycles = 3;
            }
            0x68 => {
                // PLA: C2 dummy read PC, C3 dummy stack read (pre-increment),
                // then the pull.
                {
                    let _ = self.read1(bus, self.pc);
                    let _ = self.read1(bus, STACK_BASE | u16::from(self.s));
                }
                self.a = self.pull(bus);
                self.p.set_nz(self.a);
                *cycles = 4;
            }
            0x28 => {
                {
                    let _ = self.read1(bus, self.pc);
                    let _ = self.read1(bus, STACK_BASE | u16::from(self.s));
                }
                let v = self.pull(bus);
                let mut new_p = Status::from_bits_truncate(v);
                new_p.remove(Status::BREAK);
                new_p.insert(Status::UNUSED);
                self.p = new_p;
                *cycles = 4;
            }

            // === Logical ===
            0x29 => {
                let v = self.fetch_pc(bus);
                self.and(v);
                *cycles = 2;
            }
            0x25 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 3;
            }
            0x35 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 4;
            }
            0x2D => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 4;
            }
            0x3D => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x39 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x21 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 6;
            }
            0x31 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.and(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0x09 => {
                let v = self.fetch_pc(bus);
                self.ora(v);
                *cycles = 2;
            }
            0x05 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 3;
            }
            0x15 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 4;
            }
            0x0D => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 4;
            }
            0x1D => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x19 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x01 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 6;
            }
            0x11 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.ora(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0x49 => {
                let v = self.fetch_pc(bus);
                self.eor(v);
                *cycles = 2;
            }
            0x45 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 3;
            }
            0x55 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 4;
            }
            0x4D => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 4;
            }
            0x5D => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x59 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x41 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 6;
            }
            0x51 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.eor(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0x24 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.bit(v);
                *cycles = 3;
            }
            0x2C => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.bit(v);
                *cycles = 4;
            }

            // === Arithmetic ===
            0x69 => {
                let v = self.fetch_pc(bus);
                self.adc(v);
                *cycles = 2;
            }
            0x65 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 3;
            }
            0x75 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 4;
            }
            0x6D => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 4;
            }
            0x7D => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x79 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x61 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 6;
            }
            0x71 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.adc(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0xE9 | 0xEB => {
                let v = self.fetch_pc(bus);
                self.sbc(v);
                *cycles = 2;
            }
            0xE5 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 3;
            }
            0xF5 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 4;
            }
            0xED => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 4;
            }
            0xFD => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xF9 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xE1 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 6;
            }
            0xF1 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.sbc(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            // === Compare ===
            0xC9 => {
                let v = self.fetch_pc(bus);
                self.cmp_with(self.a, v);
                *cycles = 2;
            }
            0xC5 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 3;
            }
            0xD5 => {
                let o = self.addr_zp_x(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 4;
            }
            0xCD => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 4;
            }
            0xDD => {
                let o = self.addr_abs_x(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xD9 => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xC1 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 6;
            }
            0xD1 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.a, v);
                *cycles = 5 + u8::from(o.page_crossed);
            }

            0xE0 => {
                let v = self.fetch_pc(bus);
                self.cmp_with(self.x, v);
                *cycles = 2;
            }
            0xE4 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.x, v);
                *cycles = 3;
            }
            0xEC => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.x, v);
                *cycles = 4;
            }

            0xC0 => {
                let v = self.fetch_pc(bus);
                self.cmp_with(self.y, v);
                *cycles = 2;
            }
            0xC4 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.y, v);
                *cycles = 3;
            }
            0xCC => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.cmp_with(self.y, v);
                *cycles = 4;
            }

            // === Increments / decrements ===
            0xE6 => {
                let o = self.addr_zp(bus);
                self.inc_addr(bus, o.addr);
                *cycles = 5;
            }
            0xF6 => {
                let o = self.addr_zp_x(bus);
                self.inc_addr(bus, o.addr);
                *cycles = 6;
            }
            0xEE => {
                let o = self.addr_abs(bus);
                self.inc_addr(bus, o.addr);
                *cycles = 6;
            }
            0xFE => {
                let addr = self.addr_abs_x_rmw(bus);
                self.inc_addr(bus, addr);
                *cycles = 7;
            }
            0xC6 => {
                let o = self.addr_zp(bus);
                self.dec_addr(bus, o.addr);
                *cycles = 5;
            }
            0xD6 => {
                let o = self.addr_zp_x(bus);
                self.dec_addr(bus, o.addr);
                *cycles = 6;
            }
            0xCE => {
                let o = self.addr_abs(bus);
                self.dec_addr(bus, o.addr);
                *cycles = 6;
            }
            0xDE => {
                let addr = self.addr_abs_x_rmw(bus);
                self.dec_addr(bus, addr);
                *cycles = 7;
            }
            0xE8 => {
                self.implied_dummy_read(bus);
                self.x = self.x.wrapping_add(1);
                self.p.set_nz(self.x);
                *cycles = 2;
            }
            0xCA => {
                self.implied_dummy_read(bus);
                self.x = self.x.wrapping_sub(1);
                self.p.set_nz(self.x);
                *cycles = 2;
            }
            0xC8 => {
                self.implied_dummy_read(bus);
                self.y = self.y.wrapping_add(1);
                self.p.set_nz(self.y);
                *cycles = 2;
            }
            0x88 => {
                self.implied_dummy_read(bus);
                self.y = self.y.wrapping_sub(1);
                self.p.set_nz(self.y);
                *cycles = 2;
            }

            // === Shifts ===
            0x0A => {
                self.implied_dummy_read(bus);
                self.a = self.asl_value(self.a);
                *cycles = 2;
            }
            0x06 => {
                let o = self.addr_zp(bus);
                self.asl_addr(bus, o.addr);
                *cycles = 5;
            }
            0x16 => {
                let o = self.addr_zp_x(bus);
                self.asl_addr(bus, o.addr);
                *cycles = 6;
            }
            0x0E => {
                let o = self.addr_abs(bus);
                self.asl_addr(bus, o.addr);
                *cycles = 6;
            }
            0x1E => {
                let addr = self.addr_abs_x_rmw(bus);
                self.asl_addr(bus, addr);
                *cycles = 7;
            }

            0x4A => {
                self.implied_dummy_read(bus);
                self.a = self.lsr_value(self.a);
                *cycles = 2;
            }
            0x46 => {
                let o = self.addr_zp(bus);
                self.lsr_addr(bus, o.addr);
                *cycles = 5;
            }
            0x56 => {
                let o = self.addr_zp_x(bus);
                self.lsr_addr(bus, o.addr);
                *cycles = 6;
            }
            0x4E => {
                let o = self.addr_abs(bus);
                self.lsr_addr(bus, o.addr);
                *cycles = 6;
            }
            0x5E => {
                let addr = self.addr_abs_x_rmw(bus);
                self.lsr_addr(bus, addr);
                *cycles = 7;
            }

            0x2A => {
                self.implied_dummy_read(bus);
                self.a = self.rol_value(self.a);
                *cycles = 2;
            }
            0x26 => {
                let o = self.addr_zp(bus);
                self.rol_addr(bus, o.addr);
                *cycles = 5;
            }
            0x36 => {
                let o = self.addr_zp_x(bus);
                self.rol_addr(bus, o.addr);
                *cycles = 6;
            }
            0x2E => {
                let o = self.addr_abs(bus);
                self.rol_addr(bus, o.addr);
                *cycles = 6;
            }
            0x3E => {
                let addr = self.addr_abs_x_rmw(bus);
                self.rol_addr(bus, addr);
                *cycles = 7;
            }

            0x6A => {
                self.implied_dummy_read(bus);
                self.a = self.ror_value(self.a);
                *cycles = 2;
            }
            0x66 => {
                let o = self.addr_zp(bus);
                self.ror_addr(bus, o.addr);
                *cycles = 5;
            }
            0x76 => {
                let o = self.addr_zp_x(bus);
                self.ror_addr(bus, o.addr);
                *cycles = 6;
            }
            0x6E => {
                let o = self.addr_abs(bus);
                self.ror_addr(bus, o.addr);
                *cycles = 6;
            }
            0x7E => {
                let addr = self.addr_abs_x_rmw(bus);
                self.ror_addr(bus, addr);
                *cycles = 7;
            }

            // === Branches ===
            //
            // The `branch_delays_irq` quirk: real 6502 branches poll IRQ
            // at the same point a 2-cycle untaken branch would — at the
            // opcode-fetch cycle (the canonical 2-cycle "second-to-last"
            // poll).  The operand-fetch cycle and any extra taken /
            // page-cross cycles do NOT re-sample IRQ.  We suppress IRQ
            // sampling for the remaining cycles of the instruction
            // immediately *before* the operand fetch — the opcode-fetch
            // sample (in `step()`) has already happened by this point.
            // See `docs/cpu-6502.md` §Interrupt logic and
            // <https://www.nesdev.org/wiki/CPU_interrupts>.
            0x10 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, !self.p.contains(Status::NEGATIVE));
            }
            0x30 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, self.p.contains(Status::NEGATIVE));
            }
            0x50 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, !self.p.contains(Status::OVERFLOW));
            }
            0x70 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, self.p.contains(Status::OVERFLOW));
            }
            0x90 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, !self.p.contains(Status::CARRY));
            }
            0xB0 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, self.p.contains(Status::CARRY));
            }
            0xD0 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, !self.p.contains(Status::ZERO));
            }
            0xF0 => {
                let off = self.fetch_pc(bus);
                *cycles = self.branch(bus, off, self.p.contains(Status::ZERO));
            }

            // === Jumps / subroutine ===
            0x4C => {
                self.pc = self.fetch_pc_u16(bus);
                *cycles = 3;
            }
            0x6C => {
                let ptr = self.fetch_pc_u16(bus);
                self.pc = self.read_u16_with_wrap(bus, ptr);
                *cycles = 5;
            }
            0x20 => {
                // Canonical 6502 JSR cycle sequence — the high byte of
                // the target is read AFTER PC is pushed to the stack.
                // Wrong order is observable when JSR overwrites its own
                // operand via the pushed return address (AccuracyCoin
                // `CPU Behavior 2 :: JSR Edge Cases` Test 2 brackets
                // this exactly):
                //   C1: opcode fetch (already done by `tick` dispatcher)
                //   C2: fetch low byte of target → advances PC
                //   C3: dummy read from stack at $0100|S (no-op)
                //   C4: push PC high (PC is currently at the high-byte
                //       operand address, which is exactly the return
                //       address minus one)
                //   C5: push PC low
                //   C6: fetch high byte of target → PC = target
                let lo = self.fetch_pc(bus);
                let _ = self.read1(bus, STACK_BASE | u16::from(self.s));
                // self.pc now points at the high-byte operand; this is
                // the "return - 1" address JSR canonically pushes.
                let return_minus_one = self.pc;
                self.push(bus, (return_minus_one >> 8) as u8);
                self.push(bus, (return_minus_one & 0xFF) as u8);
                let hi = self.fetch_pc(bus);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
                *cycles = 6;
            }
            0x60 => {
                // Canonical 6502 RTS bus pattern (every cycle is a bus access):
                //   C1 opcode fetch (dispatcher) | C2 dummy read PC |
                //   C3 dummy stack read (pre-increment) |
                //   C4 pull PCL | C5 pull PCH | C6 dummy read at the return addr.
                // Default build burns C2/C3/C6 as `idle_tick` (no bus access);
                // `cpu-stack-dummy-reads` emits the canonical dummy reads — the
                // DC-6 Y=3-vs-4 fix. See the cell-trace cross-diff.
                {
                    let _ = self.read1(bus, self.pc);
                    let _ = self.read1(bus, STACK_BASE | u16::from(self.s));
                    let v = self.pull_u16(bus);
                    let _ = self.read1(bus, v);
                    self.pc = v.wrapping_add(1);
                }
                *cycles = 6;
            }
            0x40 => {
                // Canonical RTI bus pattern: C2 dummy read PC, C3 dummy stack
                // read (pre-increment) before the pulls. Default-off helper.
                {
                    let _ = self.read1(bus, self.pc);
                    let _ = self.read1(bus, STACK_BASE | u16::from(self.s));
                }
                let p = self.pull(bus);
                let mut new_p = Status::from_bits_truncate(p);
                new_p.remove(Status::BREAK);
                new_p.insert(Status::UNUSED);
                self.p = new_p;
                // RTI's I-flag change is observed by the IRQ sample
                // (unlike PLP / CLI / SEI which delay one instruction).
                self.pc = self.pull_u16(bus);
                *cycles = 6;
            }
            0x00 => {
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
            }
            0xEA => {
                self.implied_dummy_read(bus);
                *cycles = 2;
            }

            // === Flag manipulation ===
            0x18 => {
                self.implied_dummy_read(bus);
                self.p.remove(Status::CARRY);
                *cycles = 2;
            }
            0x38 => {
                self.implied_dummy_read(bus);
                self.p.insert(Status::CARRY);
                *cycles = 2;
            }
            0x58 => {
                self.implied_dummy_read(bus);
                self.p.remove(Status::INTERRUPT_DISABLE);
                *cycles = 2;
            }
            0x78 => {
                self.implied_dummy_read(bus);
                self.p.insert(Status::INTERRUPT_DISABLE);
                *cycles = 2;
            }
            0xB8 => {
                self.implied_dummy_read(bus);
                self.p.remove(Status::OVERFLOW);
                *cycles = 2;
            }
            0xD8 => {
                self.implied_dummy_read(bus);
                self.p.remove(Status::DECIMAL);
                *cycles = 2;
            }
            0xF8 => {
                self.implied_dummy_read(bus);
                self.p.insert(Status::DECIMAL);
                *cycles = 2;
            }

            // === Unofficial NOP variants ===
            // Implied / 1-byte NOPs
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {
                self.implied_dummy_read(bus);
                *cycles = 2;
            }
            // Immediate / zero-page DOP (double NOP) variants: skip 1 byte.
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                let _ = self.fetch_pc(bus);
                *cycles = 2;
            }
            0x04 | 0x44 | 0x64 => {
                let o = self.addr_zp(bus);
                let _ = self.read1(bus, o.addr); // unofficial DOP dummy read
                *cycles = 3;
            }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                let o = self.addr_zp_x(bus);
                let _ = self.read1(bus, o.addr); // unofficial DOP dummy read
                *cycles = 4;
            }
            // Absolute "TOP" (triple NOP) — must dummy-read the target so
            // that PPU-mirror side-effects (e.g. clearing $2002.7) fire,
            // matching real silicon and AccuracyCoin's All-NOPs Test 2.
            0x0C => {
                let o = self.addr_abs(bus);
                let _ = self.read1(bus, o.addr);
                *cycles = 4;
            }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let o = self.addr_abs_x(bus);
                let _ = self.read1(bus, o.addr); // dummy read on TOP
                *cycles = 4 + u8::from(o.page_crossed);
            }

            // === Stable unofficial: LAX, SAX ===
            0xA7 => {
                let o = self.addr_zp(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 3;
            }
            0xB7 => {
                let o = self.addr_zp_y(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 4;
            }
            0xAF => {
                let o = self.addr_abs(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 4;
            }
            0xBF => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0xA3 => {
                let o = self.addr_ind_x(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 6;
            }
            0xB3 => {
                let o = self.addr_ind_y(bus);
                let v = self.read1(bus, o.addr);
                self.lax(v);
                *cycles = 5 + u8::from(o.page_crossed);
            }
            0xAB => {
                // LXA / LAX #imm / ATX: A = X = (A | const) & operand — the
                // same unstable floating-bus constant as ANE/XAA ($8B) above;
                // 0xEE matches the SingleStepTests corpus 100%.
                let v = self.fetch_pc(bus);
                let r = (self.a | 0xEE) & v;
                self.a = r;
                self.x = r;
                self.p.set_nz(r);
                *cycles = 2;
            }

            0x87 => {
                let o = self.addr_zp(bus);
                self.write1(bus, o.addr, self.a & self.x);
                *cycles = 3;
            }
            0x97 => {
                let o = self.addr_zp_y(bus);
                self.write1(bus, o.addr, self.a & self.x);
                *cycles = 4;
            }
            0x8F => {
                let o = self.addr_abs(bus);
                self.write1(bus, o.addr, self.a & self.x);
                *cycles = 4;
            }
            0x83 => {
                let o = self.addr_ind_x(bus);
                self.write1(bus, o.addr, self.a & self.x);
                *cycles = 6;
            }

            // === DCP (DEC + CMP) ===
            0xC7 => {
                let o = self.addr_zp(bus);
                self.dcp_addr(bus, o.addr);
                *cycles = 5;
            }
            0xD7 => {
                let o = self.addr_zp_x(bus);
                self.dcp_addr(bus, o.addr);
                *cycles = 6;
            }
            0xCF => {
                let o = self.addr_abs(bus);
                self.dcp_addr(bus, o.addr);
                *cycles = 6;
            }
            0xDF => {
                let addr = self.addr_abs_x_rmw(bus);
                self.dcp_addr(bus, addr);
                *cycles = 7;
            }
            0xDB => {
                let addr = self.addr_abs_y_rmw(bus);
                self.dcp_addr(bus, addr);
                *cycles = 7;
            }
            0xC3 => {
                let o = self.addr_ind_x(bus);
                self.dcp_addr(bus, o.addr);
                *cycles = 8;
            }
            0xD3 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.dcp_addr(bus, addr);
                *cycles = 8;
            }

            // === ISC (INC + SBC) ===
            0xE7 => {
                let o = self.addr_zp(bus);
                self.isc_addr(bus, o.addr);
                *cycles = 5;
            }
            0xF7 => {
                let o = self.addr_zp_x(bus);
                self.isc_addr(bus, o.addr);
                *cycles = 6;
            }
            0xEF => {
                let o = self.addr_abs(bus);
                self.isc_addr(bus, o.addr);
                *cycles = 6;
            }
            0xFF => {
                let addr = self.addr_abs_x_rmw(bus);
                self.isc_addr(bus, addr);
                *cycles = 7;
            }
            0xFB => {
                let addr = self.addr_abs_y_rmw(bus);
                self.isc_addr(bus, addr);
                *cycles = 7;
            }
            0xE3 => {
                let o = self.addr_ind_x(bus);
                self.isc_addr(bus, o.addr);
                *cycles = 8;
            }
            0xF3 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.isc_addr(bus, addr);
                *cycles = 8;
            }

            // === SLO (ASL + ORA) ===
            0x07 => {
                let o = self.addr_zp(bus);
                self.slo_addr(bus, o.addr);
                *cycles = 5;
            }
            0x17 => {
                let o = self.addr_zp_x(bus);
                self.slo_addr(bus, o.addr);
                *cycles = 6;
            }
            0x0F => {
                let o = self.addr_abs(bus);
                self.slo_addr(bus, o.addr);
                *cycles = 6;
            }
            0x1F => {
                let addr = self.addr_abs_x_rmw(bus);
                self.slo_addr(bus, addr);
                *cycles = 7;
            }
            0x1B => {
                let addr = self.addr_abs_y_rmw(bus);
                self.slo_addr(bus, addr);
                *cycles = 7;
            }
            0x03 => {
                let o = self.addr_ind_x(bus);
                self.slo_addr(bus, o.addr);
                *cycles = 8;
            }
            0x13 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.slo_addr(bus, addr);
                *cycles = 8;
            }

            // === RLA (ROL + AND) ===
            0x27 => {
                let o = self.addr_zp(bus);
                self.rla_addr(bus, o.addr);
                *cycles = 5;
            }
            0x37 => {
                let o = self.addr_zp_x(bus);
                self.rla_addr(bus, o.addr);
                *cycles = 6;
            }
            0x2F => {
                let o = self.addr_abs(bus);
                self.rla_addr(bus, o.addr);
                *cycles = 6;
            }
            0x3F => {
                let addr = self.addr_abs_x_rmw(bus);
                self.rla_addr(bus, addr);
                *cycles = 7;
            }
            0x3B => {
                let addr = self.addr_abs_y_rmw(bus);
                self.rla_addr(bus, addr);
                *cycles = 7;
            }
            0x23 => {
                let o = self.addr_ind_x(bus);
                self.rla_addr(bus, o.addr);
                *cycles = 8;
            }
            0x33 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.rla_addr(bus, addr);
                *cycles = 8;
            }

            // === SRE (LSR + EOR) ===
            0x47 => {
                let o = self.addr_zp(bus);
                self.sre_addr(bus, o.addr);
                *cycles = 5;
            }
            0x57 => {
                let o = self.addr_zp_x(bus);
                self.sre_addr(bus, o.addr);
                *cycles = 6;
            }
            0x4F => {
                let o = self.addr_abs(bus);
                self.sre_addr(bus, o.addr);
                *cycles = 6;
            }
            0x5F => {
                let addr = self.addr_abs_x_rmw(bus);
                self.sre_addr(bus, addr);
                *cycles = 7;
            }
            0x5B => {
                let addr = self.addr_abs_y_rmw(bus);
                self.sre_addr(bus, addr);
                *cycles = 7;
            }
            0x43 => {
                let o = self.addr_ind_x(bus);
                self.sre_addr(bus, o.addr);
                *cycles = 8;
            }
            0x53 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.sre_addr(bus, addr);
                *cycles = 8;
            }

            // === RRA (ROR + ADC) ===
            0x67 => {
                let o = self.addr_zp(bus);
                self.rra_addr(bus, o.addr);
                *cycles = 5;
            }
            0x77 => {
                let o = self.addr_zp_x(bus);
                self.rra_addr(bus, o.addr);
                *cycles = 6;
            }
            0x6F => {
                let o = self.addr_abs(bus);
                self.rra_addr(bus, o.addr);
                *cycles = 6;
            }
            0x7F => {
                let addr = self.addr_abs_x_rmw(bus);
                self.rra_addr(bus, addr);
                *cycles = 7;
            }
            0x7B => {
                let addr = self.addr_abs_y_rmw(bus);
                self.rra_addr(bus, addr);
                *cycles = 7;
            }
            0x63 => {
                let o = self.addr_ind_x(bus);
                self.rra_addr(bus, o.addr);
                *cycles = 8;
            }
            0x73 => {
                let addr = self.addr_ind_y_rmw(bus);
                self.rra_addr(bus, addr);
                *cycles = 8;
            }

            // === ANC, ALR, ARR, AXS ===
            0x0B | 0x2B => {
                let v = self.fetch_pc(bus);
                self.a &= v;
                self.p.set_nz(self.a);
                self.p.set(Status::CARRY, self.a & 0x80 != 0);
                *cycles = 2;
            }
            0x4B => {
                let v = self.fetch_pc(bus);
                self.a &= v;
                let new_carry = self.a & 0x01 != 0;
                self.a >>= 1;
                self.p.set_nz(self.a);
                self.p.set(Status::CARRY, new_carry);
                *cycles = 2;
            }
            0x6B => {
                // ARR: A = (A & operand), rotated right through carry. N/Z/V
                // are read off the ROR result BEFORE any decimal correction;
                // in decimal mode C and the final accumulator get an
                // additional BCD adjustment N/Z/V do NOT see — a famous NMOS
                // undocumented-opcode quirk, reverse-engineered against the
                // SingleStepTests 65x02 corpus
                // (`crates/rusty2600-cpu/tests/singlestep_test.rs`).
                let v = self.fetch_pc(bus);
                let t = self.a & v;
                let carry_in = u8::from(self.p.contains(Status::CARRY));
                let ror = (t >> 1) | (carry_in << 7);
                self.p.set_nz(ror);
                let bit6 = ror & 0x40 != 0;
                let bit5 = ror & 0x20 != 0;
                self.p.set(Status::OVERFLOW, bit6 ^ bit5);

                if self.p.contains(Status::DECIMAL) {
                    let mut result = ror;
                    if (t & 0x0F) + (t & 0x01) > 0x05 {
                        result = (result & 0xF0) | ((result.wrapping_add(6)) & 0x0F);
                    }
                    let carry = (t & 0xF0) + (t & 0x10) > 0x50;
                    if carry {
                        result = result.wrapping_add(0x60);
                    }
                    self.p.set(Status::CARRY, carry);
                    self.a = result;
                } else {
                    self.p.set(Status::CARRY, bit6);
                    self.a = ror;
                }
                *cycles = 2;
            }
            0xCB => {
                let v = self.fetch_pc(bus);
                let ax = self.a & self.x;
                let (res, overflow) = ax.overflowing_sub(v);
                self.x = res;
                self.p.set(Status::CARRY, !overflow);
                self.p.set_nz(res);
                *cycles = 2;
            }

            // === Unstable: XAA, LAS, TAS, SHA, SHX, SHY ===
            0x8B => {
                // XAA / ANE: A = (A | const) & X & operand. The "const" is a
                // chip-batch-dependent floating-bus effect (this instruction
                // is famously unstable); 0xEE is the value that gives a
                // 100%-matching result against the SingleStepTests 65x02
                // corpus (`crates/rusty2600-cpu/tests/singlestep_test.rs`),
                // reverse-engineered by sweeping candidate constants.
                let v = self.fetch_pc(bus);
                self.a = (self.a | 0xEE) & self.x & v;
                self.p.set_nz(self.a);
                *cycles = 2;
            }
            0xBB => {
                let o = self.addr_abs_y(bus);
                let v = self.read1(bus, o.addr);
                let res = self.s & v;
                self.a = res;
                self.x = res;
                self.s = res;
                self.p.set_nz(res);
                *cycles = 4 + u8::from(o.page_crossed);
            }
            0x9B => {
                // TAS / SHS / XAS abs,Y: S = A & X; then SHA-style write
                // using `S` as the value register.
                let base = self.fetch_pc_u16(bus);
                self.s = self.a & self.x;
                self.sh_store(bus, base, self.y, self.s);
                *cycles = 5;
            }
            0x9F => {
                // SHA abs,Y. value_reg = A & X.
                let base = self.fetch_pc_u16(bus);
                self.sh_store(bus, base, self.y, self.a & self.x);
                *cycles = 5;
            }
            0x93 => {
                // SHA (zp),Y. Indirect; base from zp-pointer-resolved
                // low/high bytes.  value_reg = A & X.
                let zp = self.fetch_pc(bus);
                let lo = self.read1(bus, u16::from(zp));
                let hi_byte = self.read1(bus, u16::from(zp.wrapping_add(1)));
                let base = u16::from(lo) | (u16::from(hi_byte) << 8);
                self.sh_store(bus, base, self.y, self.a & self.x);
                *cycles = 6;
            }
            0x9E => {
                // SHX abs,Y. value_reg = X.
                let base = self.fetch_pc_u16(bus);
                self.sh_store(bus, base, self.y, self.x);
                *cycles = 5;
            }
            0x9C => {
                // SHY abs,X. value_reg = Y. Index register is X here.
                let base = self.fetch_pc_u16(bus);
                self.sh_store(bus, base, self.x, self.y);
                *cycles = 5;
            }

            // === JAM / KIL / STP ===
            //
            // Real silicon locks up permanently: the address bus reads the
            // PC+1 byte once (without ever advancing PC past it), then
            // settles into the fixed sequence $FFFF, $FFFE, $FFFE, $FFFF...
            // forever — a decode-PLA artifact identical across all 12 JAM
            // opcodes (verified against the SingleStepTests 65x02 corpus,
            // `crates/rusty2600-cpu/tests/singlestep_test.rs`, which samples
            // an 11-cycle window of that infinite pattern). We reproduce
            // that exact window so `jammed` state and its cycle trace match
            // real hardware for however long a caller keeps stepping; there
            // is no way out of this state short of a reset.
            0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
                let _ = self.read1(bus, self.pc);
                let _ = self.read1(bus, 0xFFFF);
                let _ = self.read1(bus, 0xFFFE);
                let _ = self.read1(bus, 0xFFFE);
                for _ in 0..6 {
                    let _ = self.read1(bus, 0xFFFF);
                }
                self.jammed = true;
                *cycles = 11;
            }
        }
    }

    // ------------------------------------------------------------------
    // Helpers / micro-ops.
    // ------------------------------------------------------------------

    fn lda(&mut self, value: u8) {
        self.a = value;
        self.p.set_nz(value);
    }

    fn lda_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.lda(v);
    }

    fn ldx(&mut self, value: u8) {
        self.x = value;
        self.p.set_nz(value);
    }

    fn ldy(&mut self, value: u8) {
        self.y = value;
        self.p.set_nz(value);
    }

    fn and(&mut self, value: u8) {
        self.a &= value;
        self.p.set_nz(self.a);
    }

    fn ora(&mut self, value: u8) {
        self.a |= value;
        self.p.set_nz(self.a);
    }

    fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.p.set_nz(self.a);
    }

    fn bit(&mut self, value: u8) {
        let result = self.a & value;
        self.p.set(Status::ZERO, result == 0);
        self.p.set(Status::NEGATIVE, value & 0x80 != 0);
        self.p.set(Status::OVERFLOW, value & 0x40 != 0);
    }

    fn adc(&mut self, value: u8) {
        if self.p.contains(Status::DECIMAL) {
            let carry_in = u16::from(self.p.contains(Status::CARRY));

            // Z uses the plain binary sum — a separate, well-documented NMOS
            // decimal-mode quirk from N/V below.
            let bin_sum = u16::from(self.a) + u16::from(value) + carry_in;
            self.p.set(Status::ZERO, (bin_sum as u8) == 0);

            // Low-nibble BCD digit, corrected if it overflowed a decimal digit.
            let mut al = u16::from(self.a & 0x0F) + u16::from(value & 0x0F) + carry_in;
            if al > 0x09 {
                al = ((al + 0x06) & 0x0F) + 0x10;
            }
            // N and V are read off THIS intermediate — low nibble already BCD-
            // corrected, high nibble NOT YET corrected — not the plain binary
            // sum above and not the final BCD-corrected accumulator either.
            // This is the single most-cited NMOS 6502 decimal-mode gotcha
            // (see 6502.org "Decimal Mode"); confirmed here against the
            // SingleStepTests 65x02 cycle-exact corpus
            // (`crates/rusty2600-cpu/tests/singlestep_test.rs`), which is what
            // caught it — the plain-binary-sum version we had before matches
            // real hardware everywhere EXCEPT decimal-mode ADC/SBC.
            let intermediate = (u16::from(self.a & 0xF0) + u16::from(value & 0xF0) + al) as u8;
            self.p.set(Status::NEGATIVE, (intermediate & 0x80) != 0);
            self.p.set(
                Status::OVERFLOW,
                ((self.a ^ intermediate) & (value ^ intermediate) & 0x80) != 0,
            );

            // Final accumulator + carry: apply the high-nibble correction.
            let mut result = u16::from(self.a & 0xF0) + u16::from(value & 0xF0) + al;
            if result >= 0xA0 {
                result += 0x60;
            }
            self.p.set(Status::CARRY, result >= 0x100);
            self.a = result as u8;
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
    fn sbc(&mut self, value: u8) {
        if self.p.contains(Status::DECIMAL) {
            let carry = self.p.contains(Status::CARRY) as u8;
            let bin_sum = (self.a as u16) + ((value ^ 0xFF) as u16) + (carry as u16);
            let bin_result = bin_sum as u8;

            self.p.set(Status::ZERO, bin_result == 0);
            self.p.set(Status::NEGATIVE, (bin_result & 0x80) != 0);
            self.p.set(
                Status::OVERFLOW,
                ((self.a ^ bin_result) & ((value ^ 0xFF) ^ bin_result) & 0x80) != 0,
            );

            let mut al = (self.a & 0x0F)
                .wrapping_sub(value & 0x0F)
                .wrapping_sub(1 - carry);
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
    fn cmp_with(&mut self, lhs: u8, rhs: u8) {
        let (r, borrow) = lhs.overflowing_sub(rhs);
        self.p.set(Status::CARRY, !borrow);
        self.p.set_nz(r);
    }

    fn inc_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let original = self.read1(bus, addr);
        // RMW dummy write: real 6502 writes the original byte back to the
        // same address before writing the modified value (visible at memory-
        // mapped registers like $4014 and $2007). See `docs/cpu-6502.md` and
        // nesdev wiki "Dummy writes".
        self.write1(bus, addr, original);
        let v = original.wrapping_add(1);
        self.write1(bus, addr, v);
        self.p.set_nz(v);
    }

    fn dec_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let original = self.read1(bus, addr);
        self.write1(bus, addr, original);
        let v = original.wrapping_sub(1);
        self.write1(bus, addr, v);
        self.p.set_nz(v);
    }

    fn asl_value(&mut self, value: u8) -> u8 {
        self.p.set(Status::CARRY, value & 0x80 != 0);
        let r = value << 1;
        self.p.set_nz(r);
        r
    }

    fn asl_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        // RMW dummy write — see `inc_addr`.
        self.write1(bus, addr, v);
        let r = self.asl_value(v);
        self.write1(bus, addr, r);
    }

    fn lsr_value(&mut self, value: u8) -> u8 {
        self.p.set(Status::CARRY, value & 0x01 != 0);
        let r = value >> 1;
        self.p.set_nz(r);
        r
    }

    fn lsr_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.lsr_value(v);
        self.write1(bus, addr, r);
    }

    fn rol_value(&mut self, value: u8) -> u8 {
        let carry_in = u8::from(self.p.contains(Status::CARRY));
        self.p.set(Status::CARRY, value & 0x80 != 0);
        let r = (value << 1) | carry_in;
        self.p.set_nz(r);
        r
    }

    fn rol_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.rol_value(v);
        self.write1(bus, addr, r);
    }

    fn ror_value(&mut self, value: u8) -> u8 {
        let carry_in = u8::from(self.p.contains(Status::CARRY)) << 7;
        self.p.set(Status::CARRY, value & 0x01 != 0);
        let r = (value >> 1) | carry_in;
        self.p.set_nz(r);
        r
    }

    fn ror_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.ror_value(v);
        self.write1(bus, addr, r);
    }

    fn branch<B: CpuBus>(&mut self, bus: &mut B, offset: u8, condition: bool) -> u8 {
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
    fn lax(&mut self, value: u8) {
        self.a = value;
        self.x = value;
        self.p.set_nz(value);
    }

    fn dcp_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let original = self.read1(bus, addr);
        // RMW dummy write.
        self.write1(bus, addr, original);
        let v = original.wrapping_sub(1);
        self.write1(bus, addr, v);
        self.cmp_with(self.a, v);
    }

    fn isc_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let original = self.read1(bus, addr);
        self.write1(bus, addr, original);
        let v = original.wrapping_add(1);
        self.write1(bus, addr, v);
        self.sbc(v);
    }

    fn slo_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.asl_value(v);
        self.write1(bus, addr, r);
        self.a |= r;
        self.p.set_nz(self.a);
    }

    fn rla_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.rol_value(v);
        self.write1(bus, addr, r);
        self.a &= r;
        self.p.set_nz(self.a);
    }

    fn sre_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.lsr_value(v);
        self.write1(bus, addr, r);
        self.a ^= r;
        self.p.set_nz(self.a);
    }

    fn rra_addr<B: CpuBus>(&mut self, bus: &mut B, addr: u16) {
        let v = self.read1(bus, addr);
        self.write1(bus, addr, v);
        let r = self.ror_value(v);
        self.write1(bus, addr, r);
        self.adc(r);
    }
}

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
        let mut cpu = Cpu::power_on();
        cpu.reset(&mut bus);
        assert_eq!(cpu.pc, 0x1234);
        assert_eq!(cpu.s, 0xFD);
    }
}
