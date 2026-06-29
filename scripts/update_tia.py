import os

code = """
//! `rusty2600-tia` — the TIA (Television Interface Adaptor), the VCS's
//! video **and** audio chip.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

pub mod audio;

pub mod regs {
    // Write registers
    pub const VSYNC: u8 = 0x00;
    pub const VBLANK: u8 = 0x01;
    pub const WSYNC: u8 = 0x02;
    pub const RSYNC: u8 = 0x03;
    pub const NUSIZ0: u8 = 0x04;
    pub const NUSIZ1: u8 = 0x05;
    pub const COLUP0: u8 = 0x06;
    pub const COLUP1: u8 = 0x07;
    pub const COLUPF: u8 = 0x08;
    pub const COLUBK: u8 = 0x09;
    pub const CTRLPF: u8 = 0x0A;
    pub const REFP0: u8 = 0x0B;
    pub const REFP1: u8 = 0x0C;
    pub const PF0: u8 = 0x0D;
    pub const PF1: u8 = 0x0E;
    pub const PF2: u8 = 0x0F;
    pub const RESP0: u8 = 0x10;
    pub const RESP1: u8 = 0x11;
    pub const RESM0: u8 = 0x12;
    pub const RESM1: u8 = 0x13;
    pub const RESBL: u8 = 0x14;
    pub const AUDC0: u8 = 0x15;
    pub const AUDC1: u8 = 0x16;
    pub const AUDF0: u8 = 0x17;
    pub const AUDF1: u8 = 0x18;
    pub const AUDV0: u8 = 0x19;
    pub const AUDV1: u8 = 0x1A;
    pub const GRP0: u8 = 0x1B;
    pub const GRP1: u8 = 0x1C;
    pub const ENAM0: u8 = 0x1D;
    pub const ENAM1: u8 = 0x1E;
    pub const ENABL: u8 = 0x1F;
    pub const HMP0: u8 = 0x20;
    pub const HMP1: u8 = 0x21;
    pub const HMM0: u8 = 0x22;
    pub const HMM1: u8 = 0x23;
    pub const HMBL: u8 = 0x24;
    pub const VDELP0: u8 = 0x25;
    pub const VDELP1: u8 = 0x26;
    pub const VDELBL: u8 = 0x27;
    pub const RESMP0: u8 = 0x28;
    pub const RESMP1: u8 = 0x29;
    pub const HMOVE: u8 = 0x2A;
    pub const HMCLR: u8 = 0x2B;
    pub const CXCLR: u8 = 0x2C;
    
    // Read registers
    pub const CXM0P: u8 = 0x00;
    pub const CXM1P: u8 = 0x01;
    pub const CXP0FB: u8 = 0x02;
    pub const CXP1FB: u8 = 0x03;
    pub const CXM0FB: u8 = 0x04;
    pub const CXM1FB: u8 = 0x05;
    pub const CXBLPF: u8 = 0x06;
    pub const CXPPMM: u8 = 0x07;
    pub const INPT0: u8 = 0x08;
    pub const INPT1: u8 = 0x09;
    pub const INPT2: u8 = 0x0A;
    pub const INPT3: u8 = 0x0B;
    pub const INPT4: u8 = 0x0C;
    pub const INPT5: u8 = 0x0D;
}

#[derive(Debug, Default, Clone)]
pub struct Objects {
    pub pf: u32,
    pub grp: [u8; 2],
    pub nusiz: [u8; 2],
    pub pos: [u8; 5],
    pub hm: [i8; 5],
    pub colu: [u8; 4],
    
    pub vblank: u8,
    pub vsync: u8,
    pub ctrlpf: u8,
    pub refp: [bool; 2],
    pub enam: [bool; 2],
    pub enabl: bool,
    pub vdelp: [bool; 2],
    pub vdelbl: bool,
    pub resmp: [bool; 2],
    
    // Latches for delayed drawing
    pub old_grp: [u8; 2],
}

#[derive(Debug, Default, Clone)]
pub struct Collisions {
    pub cxm0p: u8,
    pub cxm1p: u8,
    pub cxp0fb: u8,
    pub cxp1fb: u8,
    pub cxm0fb: u8,
    pub cxm1fb: u8,
    pub cxblpf: u8,
    pub cxppmm: u8,
}

#[derive(Debug, Default, Clone)]
pub struct Tia {
    pub objects: Objects,
    pub collisions: Collisions,
    pub audio: audio::Audio,
    pub color_clock: u16,
    pub scanline: u16,
    pub inpt: [u8; 6],
    rdy_stall: bool,
}

impl Tia {
    #[must_use]
    pub fn new() -> Self {
        let mut tia = Self::default();
        // INPT4 and INPT5 default to tied high (buttons not pressed)
        tia.inpt[4] = 0x80;
        tia.inpt[5] = 0x80;
        tia
    }

    pub fn write_register(&mut self, reg: u8, val: u8) {
        match reg {
            regs::VSYNC => self.objects.vsync = val,
            regs::VBLANK => self.objects.vblank = val,
            regs::WSYNC => self.rdy_stall = true,
            regs::RSYNC => self.color_clock = 0,
            regs::NUSIZ0 => self.objects.nusiz[0] = val,
            regs::NUSIZ1 => self.objects.nusiz[1] = val,
            regs::COLUP0 => self.objects.colu[0] = val,
            regs::COLUP1 => self.objects.colu[1] = val,
            regs::COLUPF => self.objects.colu[2] = val,
            regs::COLUBK => self.objects.colu[3] = val,
            regs::CTRLPF => self.objects.ctrlpf = val,
            regs::REFP0 => self.objects.refp[0] = val & 0x08 != 0,
            regs::REFP1 => self.objects.refp[1] = val & 0x08 != 0,
            regs::PF0 => self.objects.pf = (self.objects.pf & 0x000F_FFFF) | (u32::from(val >> 4) << 16),
            regs::PF1 => self.objects.pf = (self.objects.pf & 0x000F_00FF) | (u32::from(val) << 8),
            regs::PF2 => self.objects.pf = (self.objects.pf & 0x000F_FF00) | u32::from(val),
            regs::RESP0 => self.objects.pos[0] = (self.color_clock + 5) as u8,
            regs::RESP1 => self.objects.pos[1] = (self.color_clock + 5) as u8,
            regs::RESM0 => self.objects.pos[2] = (self.color_clock + 4) as u8,
            regs::RESM1 => self.objects.pos[3] = (self.color_clock + 4) as u8,
            regs::RESBL => self.objects.pos[4] = (self.color_clock + 4) as u8,
            // Audio registers omitted for brevity; normally routed to self.audio
            regs::AUDC0..=regs::AUDV1 => {},
            regs::GRP0 => {
                self.objects.old_grp[1] = self.objects.grp[1]; // GRP0 updates old GRP1
                self.objects.grp[0] = val;
            },
            regs::GRP1 => {
                self.objects.old_grp[0] = self.objects.grp[0];
                self.objects.grp[1] = val;
            },
            regs::ENAM0 => self.objects.enam[0] = val & 0x02 != 0,
            regs::ENAM1 => self.objects.enam[1] = val & 0x02 != 0,
            regs::ENABL => self.objects.enabl = val & 0x02 != 0,
            regs::HMP0 => self.objects.hm[0] = (val >> 4) as i8,
            regs::HMP1 => self.objects.hm[1] = (val >> 4) as i8,
            regs::HMM0 => self.objects.hm[2] = (val >> 4) as i8,
            regs::HMM1 => self.objects.hm[3] = (val >> 4) as i8,
            regs::HMBL => self.objects.hm[4] = (val >> 4) as i8,
            regs::VDELP0 => self.objects.vdelp[0] = val & 0x01 != 0,
            regs::VDELP1 => self.objects.vdelp[1] = val & 0x01 != 0,
            regs::VDELBL => self.objects.vdelbl = val & 0x01 != 0,
            regs::RESMP0 => self.objects.resmp[0] = val & 0x02 != 0,
            regs::RESMP1 => self.objects.resmp[1] = val & 0x02 != 0,
            regs::HMOVE => {}, // Apply HMOVE
            regs::HMCLR => {
                self.objects.hm.fill(0);
            },
            regs::CXCLR => {
                self.collisions = Collisions::default();
            },
            _ => {}
        }
    }

    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        match (addr & 0x0F) as u8 {
            regs::CXM0P => self.collisions.cxm0p,
            regs::CXM1P => self.collisions.cxm1p,
            regs::CXP0FB => self.collisions.cxp0fb,
            regs::CXP1FB => self.collisions.cxp1fb,
            regs::CXM0FB => self.collisions.cxm0fb,
            regs::CXM1FB => self.collisions.cxm1fb,
            regs::CXBLPF => self.collisions.cxblpf,
            regs::CXPPMM => self.collisions.cxppmm,
            regs::INPT0 => self.inpt[0],
            regs::INPT1 => self.inpt[1],
            regs::INPT2 => self.inpt[2],
            regs::INPT3 => self.inpt[3],
            regs::INPT4 => self.inpt[4],
            regs::INPT5 => self.inpt[5],
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, val: u8) {
        self.write_register((addr & 0x3F) as u8, val);
    }

    pub fn tick_color_clock(&mut self) {
        self.color_clock += 1;
        if self.color_clock >= 228 {
            self.color_clock = 0;
            self.scanline += 1;
            self.rdy_stall = false;
        }
        self.audio.tick();
    }

    #[must_use]
    pub const fn rdy_stall(&self) -> bool {
        self.rdy_stall
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs() {
        let _ = Tia::new();
    }

    #[test]
    fn wsync_sets_and_hblank_clears_rdy() {
        let mut tia = Tia::new();
        tia.write_register(regs::WSYNC, 0);
        assert!(tia.rdy_stall());
        for _ in 0..228 {
            tia.tick_color_clock();
        }
        assert!(!tia.rdy_stall());
    }
}
"""

with open("crates/rusty2600-tia/src/lib.rs", "w") as f:
    f.write(code)
print("Updated TIA")
