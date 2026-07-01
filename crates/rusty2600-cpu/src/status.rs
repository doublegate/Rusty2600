//! The 6502/6507 processor status register `P`.

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
