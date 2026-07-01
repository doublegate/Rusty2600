//! The ARM7TDMI status flags (the APSR subset this interpreter needs).
//!
//! Only N/Z/C/V are modeled — Thumb-1 has no IT-block/EPSR state (that's a
//! Thumb-2 concept) and no saturation (`Q`) instructions, so both are
//! omitted rather than carried as always-false dead weight.

use bitflags::bitflags;

bitflags! {
    /// N/Z/C/V status flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Status: u8 {
        /// Negative (bit 31 of the result).
        const NEGATIVE = 0b0001;
        /// Zero.
        const ZERO = 0b0010;
        /// Carry / not-borrow.
        const CARRY = 0b0100;
        /// Overflow (signed).
        const OVERFLOW = 0b1000;
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::empty()
    }
}

impl Status {
    /// Set N and Z from a computed result, the pattern almost every
    /// data-processing instruction ends with.
    pub fn set_nz(&mut self, result: u32) {
        self.set(Self::NEGATIVE, result & 0x8000_0000 != 0);
        self.set(Self::ZERO, result == 0);
    }

    /// Set carry AND overflow together for a `a + b + carry_in` framing —
    /// ported from Gopher2600's `isCarry`/`isOverflow`, which are always
    /// called as a pair on the identical `(a, b, c)` inputs throughout
    /// `thumb.rs`. Every subtract in the Thumb-1 ISA is expressed as an add
    /// of the bitwise-NOT of the subtrahend with `carry_in = 1` (two's
    /// complement subtraction via `a + !b + 1`), so this single helper
    /// covers both `ADD`/`ADC` and `SUB`/`SBC`/`CMP`/`CMN` call sites — see
    /// `thumb.rs` for exactly which convention each instruction uses.
    ///
    /// The two flags share one intermediate value: the carry propagated INTO
    /// the sign bit from the lower 31 bits (`carry_into_msb`) and the carry
    /// OUT of the sign-bit addition (`carry_out_of_msb`, the real `C` flag)
    /// — overflow is `carry_into_msb XOR carry_out_of_msb`, the textbook
    /// signed-overflow definition.
    pub fn set_add_flags(&mut self, a: u32, b: u32, carry_in: u32) {
        let carry_into_msb = ((a & 0x7fff_ffff)
            .wrapping_add(b & 0x7fff_ffff)
            .wrapping_add(carry_in))
            >> 31;
        let sign_sum = carry_into_msb + (a >> 31) + (b >> 31);
        let carry_out_of_msb = (sign_sum & 0x02) == 0x02;
        self.set(Self::CARRY, carry_out_of_msb);
        self.set(
            Self::OVERFLOW,
            (carry_into_msb ^ u32::from(carry_out_of_msb)) & 1 == 1,
        );
    }

    /// Evaluate one of the 14 real Thumb-1 branch condition codes (`0b1110`
    /// = always, `0b1111` is reserved/unpredictable and asserted against
    /// rather than silently treated as true — an ARM7TDMI would never
    /// legitimately encode it, so treating it as "always branch" would
    /// mask a genuine decode bug in the ROM or this interpreter).
    #[must_use]
    pub fn condition(self, cond: u8) -> bool {
        let n = self.contains(Self::NEGATIVE);
        let z = self.contains(Self::ZERO);
        let c = self.contains(Self::CARRY);
        let v = self.contains(Self::OVERFLOW);
        match cond {
            0b0000 => z,            // BEQ
            0b0001 => !z,           // BNE
            0b0010 => c,            // BCS
            0b0011 => !c,           // BCC
            0b0100 => n,            // BMI
            0b0101 => !n,           // BPL
            0b0110 => v,            // BVS
            0b0111 => !v,           // BVC
            0b1000 => c && !z,      // BHI
            0b1001 => !c || z,      // BLS
            0b1010 => n == v,       // BGE
            0b1011 => n != v,       // BLT
            0b1100 => !z && n == v, // BGT
            0b1101 => z || n != v,  // BLE
            0b1110 => true,         // B (always)
            _ => unreachable!("condition code 0b1111 is reserved/unpredictable on ARM7TDMI"),
        }
    }
}
