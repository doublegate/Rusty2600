//! `rusty2600-tia` — the TIA (Television Interface Adaptor), the VCS's
//! video **and** audio chip.

#![no_std]
#![forbid(unsafe_code)]
#![allow(warnings)]
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

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
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

fn sign_extend_4bit(val: u8) -> i8 {
    let mut v = val >> 4;
    if (v & 0x08) != 0 {
        v |= 0xF0;
    }
    v as i8
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tia {
    pub objects: Objects,
    pub collisions: Collisions,
    pub audio: audio::Audio,
    pub color_clock: u16,
    pub scanline: u16,
    pub inpt: [u8; 6],
    pub current_color: u8,
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
            regs::PF0 => {
                self.objects.pf = (self.objects.pf & 0x000F_FFFF) | (u32::from(val >> 4) << 16)
            }
            regs::PF1 => self.objects.pf = (self.objects.pf & 0x000F_00FF) | (u32::from(val) << 8),
            regs::PF2 => self.objects.pf = (self.objects.pf & 0x000F_FF00) | u32::from(val),
            regs::RESP0 => self.objects.pos[0] = ((self.color_clock + 5) % 228) as u8,
            regs::RESP1 => self.objects.pos[1] = ((self.color_clock + 5) % 228) as u8,
            regs::RESM0 => self.objects.pos[2] = ((self.color_clock + 4) % 228) as u8,
            regs::RESM1 => self.objects.pos[3] = ((self.color_clock + 4) % 228) as u8,
            regs::RESBL => self.objects.pos[4] = ((self.color_clock + 4) % 228) as u8,
            regs::AUDC0 => self.audio.channels[0].control = val,
            regs::AUDC1 => self.audio.channels[1].control = val,
            regs::AUDF0 => self.audio.channels[0].freq = val & 0x1F,
            regs::AUDF1 => self.audio.channels[1].freq = val & 0x1F,
            regs::AUDV0 => self.audio.channels[0].volume = val & 0x0F,
            regs::AUDV1 => self.audio.channels[1].volume = val & 0x0F,
            regs::GRP0 => {
                self.objects.old_grp[1] = self.objects.grp[1]; // GRP0 updates old GRP1
                self.objects.grp[0] = val;
            }
            regs::GRP1 => {
                self.objects.old_grp[0] = self.objects.grp[0];
                self.objects.grp[1] = val;
            }
            regs::ENAM0 => self.objects.enam[0] = val & 0x02 != 0,
            regs::ENAM1 => self.objects.enam[1] = val & 0x02 != 0,
            regs::ENABL => self.objects.enabl = val & 0x02 != 0,
            regs::HMP0 => self.objects.hm[0] = sign_extend_4bit(val),
            regs::HMP1 => self.objects.hm[1] = sign_extend_4bit(val),
            regs::HMM0 => self.objects.hm[2] = sign_extend_4bit(val),
            regs::HMM1 => self.objects.hm[3] = sign_extend_4bit(val),
            regs::HMBL => self.objects.hm[4] = sign_extend_4bit(val),
            regs::VDELP0 => self.objects.vdelp[0] = val & 0x01 != 0,
            regs::VDELP1 => self.objects.vdelp[1] = val & 0x01 != 0,
            regs::VDELBL => self.objects.vdelbl = val & 0x01 != 0,
            regs::RESMP0 => self.objects.resmp[0] = val & 0x02 != 0,
            regs::RESMP1 => self.objects.resmp[1] = val & 0x02 != 0,
            regs::HMOVE => {
                for i in 0..5 {
                    let mut p = self.objects.pos[i] as i16;
                    p -= self.objects.hm[i] as i16;
                    if p < 0 {
                        p += 228;
                    }
                    p %= 228;
                    self.objects.pos[i] = p as u8;
                }
            }
            regs::HMCLR => {
                self.objects.hm.fill(0);
            }
            regs::CXCLR => {
                self.collisions = Collisions::default();
            }
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
        self.render_pixel();
        self.audio.tick();
    }

    fn render_pixel(&mut self) {
        if self.color_clock < 68 || self.scanline >= 300 {
            self.current_color = 0;
            return;
        }
        let x = self.color_clock - 68;

        // 1. Playfield
        let mut pf_pixel = false;
        let mut pf_idx = x / 4;
        let reflect = (self.objects.ctrlpf & 0x01) != 0;
        if pf_idx >= 20 {
            if reflect {
                pf_idx = 39 - pf_idx;
            } else {
                pf_idx -= 20;
            }
        }
        let pf_val = self.objects.pf;
        let is_pf = match pf_idx {
            0 => (pf_val & (1 << 16)) != 0,
            1 => (pf_val & (1 << 17)) != 0,
            2 => (pf_val & (1 << 18)) != 0,
            3 => (pf_val & (1 << 19)) != 0,
            4 => (pf_val & (1 << 15)) != 0,
            5 => (pf_val & (1 << 14)) != 0,
            6 => (pf_val & (1 << 13)) != 0,
            7 => (pf_val & (1 << 12)) != 0,
            8 => (pf_val & (1 << 11)) != 0,
            9 => (pf_val & (1 << 10)) != 0,
            10 => (pf_val & (1 << 9)) != 0,
            11 => (pf_val & (1 << 8)) != 0,
            12 => (pf_val & (1 << 0)) != 0,
            13 => (pf_val & (1 << 1)) != 0,
            14 => (pf_val & (1 << 2)) != 0,
            15 => (pf_val & (1 << 3)) != 0,
            16 => (pf_val & (1 << 4)) != 0,
            17 => (pf_val & (1 << 5)) != 0,
            18 => (pf_val & (1 << 6)) != 0,
            19 => (pf_val & (1 << 7)) != 0,
            _ => false,
        };
        if is_pf {
            pf_pixel = true;
        }

        // 2. Ball
        let mut bl_pixel = false;
        if self.objects.enabl {
            let diff = (self.color_clock + 228 - self.objects.pos[4] as u16) % 228;
            let size_bl = 1 << ((self.objects.ctrlpf >> 4) & 0x03);
            if diff < size_bl {
                bl_pixel = true;
            }
        }

        // 3. Players and Missiles
        let mut p0_pixel = false;
        let mut p1_pixel = false;
        let mut m0_pixel = false;
        let mut m1_pixel = false;

        let check_pm = |cc: u16, pos: u16, nusiz: u8, is_missile: bool| -> Option<u16> {
            let diff = (cc + 228 - pos) % 228;
            let number_size = nusiz & 0x07;
            let copies: &[u16] = match number_size {
                0 | 5 | 7 => &[0],
                1 => &[0, 16],
                2 => &[0, 32],
                3 => &[0, 16, 32],
                4 => &[0, 64],
                6 => &[0, 32, 64],
                _ => &[0],
            };
            for &start in copies {
                if diff >= start {
                    let offset = diff - start;
                    if is_missile {
                        let m_size = 1 << ((nusiz >> 4) & 0x03);
                        if offset < m_size {
                            return Some(0);
                        }
                    } else {
                        let scale = match number_size {
                            5 => 2,
                            7 => 4,
                            _ => 1,
                        };
                        if offset < 8 * scale {
                            return Some(offset / scale);
                        }
                    }
                }
            }
            None
        };

        if let Some(offset) = check_pm(
            self.color_clock,
            self.objects.pos[0] as u16,
            self.objects.nusiz[0],
            false,
        ) {
            let grp = if self.objects.vdelp[0] {
                self.objects.old_grp[0]
            } else {
                self.objects.grp[0]
            };
            let bit = if self.objects.refp[0] {
                offset
            } else {
                7 - offset
            };
            if (grp & (1 << bit)) != 0 {
                p0_pixel = true;
            }
        }
        if let Some(offset) = check_pm(
            self.color_clock,
            self.objects.pos[1] as u16,
            self.objects.nusiz[1],
            false,
        ) {
            let grp = if self.objects.vdelp[1] {
                self.objects.old_grp[1]
            } else {
                self.objects.grp[1]
            };
            let bit = if self.objects.refp[1] {
                offset
            } else {
                7 - offset
            };
            if (grp & (1 << bit)) != 0 {
                p1_pixel = true;
            }
        }
        if self.objects.enam[0] && !self.objects.resmp[0] {
            if let Some(_) = check_pm(
                self.color_clock,
                self.objects.pos[2] as u16,
                self.objects.nusiz[0],
                true,
            ) {
                m0_pixel = true;
            }
        }
        if self.objects.enam[1] && !self.objects.resmp[1] {
            if let Some(_) = check_pm(
                self.color_clock,
                self.objects.pos[3] as u16,
                self.objects.nusiz[1],
                true,
            ) {
                m1_pixel = true;
            }
        }

        // Evaluate collisions
        if m0_pixel && p1_pixel {
            self.collisions.cxm0p |= 0x80;
        }
        if m0_pixel && p0_pixel {
            self.collisions.cxm0p |= 0x40;
        }
        if m1_pixel && p0_pixel {
            self.collisions.cxm1p |= 0x80;
        }
        if m1_pixel && p1_pixel {
            self.collisions.cxm1p |= 0x40;
        }

        if p0_pixel && pf_pixel {
            self.collisions.cxp0fb |= 0x80;
        }
        if p0_pixel && bl_pixel {
            self.collisions.cxp0fb |= 0x40;
        }
        if p1_pixel && pf_pixel {
            self.collisions.cxp1fb |= 0x80;
        }
        if p1_pixel && bl_pixel {
            self.collisions.cxp1fb |= 0x40;
        }

        if m0_pixel && pf_pixel {
            self.collisions.cxm0fb |= 0x80;
        }
        if m0_pixel && bl_pixel {
            self.collisions.cxm0fb |= 0x40;
        }
        if m1_pixel && pf_pixel {
            self.collisions.cxm1fb |= 0x80;
        }
        if m1_pixel && bl_pixel {
            self.collisions.cxm1fb |= 0x40;
        }

        if bl_pixel && pf_pixel {
            self.collisions.cxblpf |= 0x80;
        }
        if p0_pixel && p1_pixel {
            self.collisions.cxppmm |= 0x80;
        }
        if m0_pixel && m1_pixel {
            self.collisions.cxppmm |= 0x40;
        }

        // Color Selection
        let pf_priority = (self.objects.ctrlpf & 0x04) != 0;
        let score_mode = (self.objects.ctrlpf & 0x02) != 0;
        let pf_color = if score_mode {
            if x < 80 {
                self.objects.colu[0]
            } else {
                self.objects.colu[1]
            }
        } else {
            self.objects.colu[2]
        };

        let mut current_color = self.objects.colu[3]; // BK

        if pf_priority {
            if pf_pixel || bl_pixel {
                current_color = pf_color;
                if bl_pixel && !pf_pixel {
                    current_color = self.objects.colu[2];
                }
            } else if p0_pixel || m0_pixel {
                current_color = self.objects.colu[0];
            } else if p1_pixel || m1_pixel {
                current_color = self.objects.colu[1];
            }
        } else {
            if p0_pixel || m0_pixel {
                current_color = self.objects.colu[0];
            } else if p1_pixel || m1_pixel {
                current_color = self.objects.colu[1];
            } else if pf_pixel || bl_pixel {
                current_color = pf_color;
                if bl_pixel && !pf_pixel {
                    current_color = self.objects.colu[2];
                }
            }
        }
        self.current_color = current_color;
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
