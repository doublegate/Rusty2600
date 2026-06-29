//! The 2600 colour palettes — NTSC, PAL, and SECAM.
//!
//! The TIA emits a colour as a 7-bit value packed into the high nibble + bit 1
//! of `COLUxx`: 4 bits of hue (the chroma phase) and 3 bits of luminance, giving
//! a 128-entry table per region. The three regions decode the same register
//! bits to DIFFERENT RGB:
//!
//! - **NTSC** — 16 hues x 8 lumas (128 colours).
//! - **PAL** — a different hue spread (and the missing/duplicated hues at the
//!   ends of the wheel); also 128 entries.
//! - **SECAM** — only 8 colours total (luminance selects from a fixed 8-colour
//!   set; chroma is ignored), broadcast back into the 128-entry layout.
//!
//! v0.1 ships these as ZEROED stub tables with the right SHAPE. The real RGB
//! values come from the measured Stella / TIA palettes — see
//! `ref-docs/` (the immutable hardware reference) and `docs/tia.md`. Pin the
//! visual-baseline screenshots against a real palette before blessing them.

/// A 0xRRGGBB colour, the form the present path uploads.
pub type Rgb = u32;

/// Number of entries in a region palette (the TIA's 7-bit colour space).
pub const PALETTE_LEN: usize = 128;

/// The TV broadcast region — selects which palette + the line count
/// (NTSC 262 / PAL 312 / SECAM 312) the scheduler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Region {
    /// NTSC: 262 lines, 60 Hz, the 128-colour NTSC table.
    #[default]
    Ntsc,
    /// PAL: 312 lines, 50 Hz, the 128-colour PAL table.
    Pal,
    /// SECAM: 312 lines, 50 Hz, the 8-colour SECAM set.
    Secam,
}

impl Region {
    /// Human-readable label for menus / the status bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ntsc => "NTSC",
            Self::Pal => "PAL",
            Self::Secam => "SECAM",
        }
    }

    /// Total scanlines per frame for this region (the scheduler's frame length).
    #[must_use]
    pub const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal | Self::Secam => 312,
        }
    }

    /// The visible framebuffer height for this region (the active window, after
    /// the top/bottom VBLANK lines are excluded). The present path sizes its
    /// upload to `(VISIBLE_WIDTH, active_height())`.
    #[must_use]
    // The visible-line constants are small (192 / 228), far below `u32::MAX`, so the
    // `usize -> u32` cast cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "visible-line constants are < 256"
    )]
    pub const fn active_height(self) -> u32 {
        match self {
            // NTSC: ~192 visible lines of the 262 total (the common Stella crop).
            Self::Ntsc => crate::present_buffer::VISIBLE_HEIGHT_NTSC as u32,
            // PAL/SECAM: ~228 visible lines of the 312 total.
            Self::Pal | Self::Secam => crate::present_buffer::VISIBLE_HEIGHT_PAL as u32,
        }
    }

    /// The wall-clock frame-rate target for this region (the pacer's authoritative
    /// cadence — the frontend owns pacing, never the core).
    #[must_use]
    pub const fn frame_rate(self) -> f64 {
        match self {
            Self::Ntsc => crate::FRAME_RATE_NTSC,
            Self::Pal | Self::Secam => crate::FRAME_RATE_PAL,
        }
    }

    /// The 128-entry RGB lookup table for this region.
    ///
    /// TODO(T-PS-053): replace the zeroed stubs with the measured Stella / TIA
    /// palette values (cite `ref-docs/`). SECAM is an 8-colour set splayed
    /// across the 128-entry layout.
    #[must_use]
    pub const fn table(self) -> &'static [Rgb; PALETTE_LEN] {
        match self {
            Self::Ntsc => &NTSC,
            Self::Pal => &PAL,
            Self::Secam => &SECAM,
        }
    }

    /// Look up a TIA 7-bit colour value in this region's palette. The TIA packs
    /// the colour in the high 7 bits of `COLUxx`, so callers pass `colu >> 1`.
    #[must_use]
    pub const fn lookup(self, colour7: u8) -> Rgb {
        self.table()[(colour7 as usize) & (PALETTE_LEN - 1)]
    }
}

// TODO(T-PS-053): real RGB values. Stubs so the present path has the right
// shape (and so a forgotten palette fills the screen with an obvious black,
// not a panic).
const NTSC: [Rgb; PALETTE_LEN] = [
    0x1F1F1F, 0x1F1F1F, 0x3F3F3F, 0x3F3F3F, 0x5F5F5F, 0x5F5F5F, 0x7F7F7F, 0x7F7F7F, 0x9F9F9F,
    0x9F9F9F, 0xBFBFBF, 0xBFBFBF, 0xDFDFDF, 0xDFDFDF, 0xFFFFFF, 0xFFFFFF, 0x251E18, 0x251E18,
    0x4B3C31, 0x4B3C31, 0x715A4A, 0x715A4A, 0x977863, 0x977863, 0xBD967C, 0xBD967C, 0xE3B494,
    0xE3B494, 0xFFD2AD, 0xFFD2AD, 0xFFF1C6, 0xFFF1C6, 0x271C1E, 0x271C1E, 0x4E393C, 0x4E393C,
    0x75555A, 0x75555A, 0x9C7278, 0x9C7278, 0xC38E97, 0xC38E97, 0xEAABB5, 0xEAABB5, 0xFFC7D3,
    0xFFC7D3, 0xFFE4F1, 0xFFE4F1, 0x261B23, 0x261B23, 0x4D3747, 0x4D3747, 0x74526B, 0x74526B,
    0x9B6E8F, 0x9B6E8F, 0xC189B3, 0xC189B3, 0xE8A5D7, 0xE8A5D7, 0xFFC0FB, 0xFFC0FB, 0xFFDCFF,
    0xFFDCFF, 0x251B28, 0x251B28, 0x4A3651, 0x4A3651, 0x6F527A, 0x6F527A, 0x946DA3, 0x946DA3,
    0xB989CC, 0xB989CC, 0xDEA4F5, 0xDEA4F5, 0xFFC0FF, 0xFFC0FF, 0xFFDBFF, 0xFFDBFF, 0x221C2C,
    0x221C2C, 0x443858, 0x443858, 0x675484, 0x675484, 0x8970B0, 0x8970B0, 0xAB8DDC, 0xAB8DDC,
    0xCEA9FF, 0xCEA9FF, 0xF0C5FF, 0xF0C5FF, 0xFFE1FF, 0xFFE1FF, 0x1F1D2C, 0x1F1D2C, 0x3E3B59,
    0x3E3B59, 0x5D5986, 0x5D5986, 0x7C76B3, 0x7C76B3, 0x9B94DF, 0x9B94DF, 0xBBB2FF, 0xBBB2FF,
    0xDAD0FF, 0xDAD0FF, 0xF9EDFF, 0xF9EDFF, 0x1C1F2A, 0x1C1F2A, 0x383F55, 0x383F55, 0x545E80,
    0x545E80, 0x707EAB, 0x707EAB, 0x8C9ED6, 0x8C9ED6, 0xA8BDFF, 0xA8BDFF, 0xC4DDFF, 0xC4DDFF,
    0xE0FDFF, 0xE0FDFF,
];
const PAL: [Rgb; PALETTE_LEN] = [
    0x1F1F1F, 0x1F1F1F, 0x3F3F3F, 0x3F3F3F, 0x5F5F5F, 0x5F5F5F, 0x7F7F7F, 0x7F7F7F, 0x9F9F9F,
    0x9F9F9F, 0xBFBFBF, 0xBFBFBF, 0xDFDFDF, 0xDFDFDF, 0xFFFFFF, 0xFFFFFF, 0x1F1F1F, 0x1F1F1F,
    0x3F3F3F, 0x3F3F3F, 0x5F5F5F, 0x5F5F5F, 0x7F7F7F, 0x7F7F7F, 0x9F9F9F, 0x9F9F9F, 0xBFBFBF,
    0xBFBFBF, 0xDFDFDF, 0xDFDFDF, 0xFFFFFF, 0xFFFFFF, 0x1F1D2C, 0x1F1D2C, 0x3F3A59, 0x3F3A59,
    0x5F5886, 0x5F5886, 0x7F75B3, 0x7F75B3, 0x9F92E0, 0x9F92E0, 0xBFB0FF, 0xBFB0FF, 0xDFCDFF,
    0xDFCDFF, 0xFFEAFF, 0xFFEAFF, 0x231B2B, 0x231B2B, 0x473756, 0x473756, 0x6A5381, 0x6A5381,
    0x8E6FAC, 0x8E6FAC, 0xB18BD7, 0xB18BD7, 0xD5A7FF, 0xD5A7FF, 0xF8C2FF, 0xF8C2FF, 0xFFDEFF,
    0xFFDEFF, 0x261B26, 0x261B26, 0x4C364C, 0x4C364C, 0x725273, 0x725273, 0x986D99, 0x986D99,
    0xBE89BF, 0xBE89BF, 0xE5A4E6, 0xE5A4E6, 0xFFBFFF, 0xFFBFFF, 0xFFDBFF, 0xFFDBFF, 0x271C1F,
    0x271C1F, 0x4E383F, 0x4E383F, 0x75545F, 0x75545F, 0x9C707F, 0x9C707F, 0xC38C9F, 0xC38C9F,
    0xEAA9BF, 0xEAA9BF, 0xFFC5DF, 0xFFC5DF, 0xFFE1FF, 0xFFE1FF, 0x261D19, 0x261D19, 0x4C3B32,
    0x4C3B32, 0x72594C, 0x72594C, 0x987765, 0x987765, 0xBE957E, 0xBE957E, 0xE5B398, 0xE5B398,
    0xFFD1B1, 0xFFD1B1, 0xFFEFCB, 0xFFEFCB, 0x232014, 0x232014, 0x474029, 0x474029, 0x6A603D,
    0x6A603D, 0x8E8052, 0x8E8052, 0xB1A167, 0xB1A167, 0xD5C17B, 0xD5C17B, 0xF8E190, 0xF8E190,
    0xFFFFA5, 0xFFFFA5,
];
const SECAM: [Rgb; PALETTE_LEN] = [
    0x000000, 0x000000, 0x0000FF, 0x0000FF, 0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00,
    0x00FF00, 0x00FFFF, 0x00FFFF, 0xFFFF00, 0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000, 0x000000,
    0x0000FF, 0x0000FF, 0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00, 0x00FFFF,
    0x00FFFF, 0xFFFF00, 0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000, 0x000000, 0x0000FF, 0x0000FF,
    0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00, 0x00FFFF, 0x00FFFF, 0xFFFF00,
    0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000, 0x000000, 0x0000FF, 0x0000FF, 0xFF0000, 0xFF0000,
    0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00, 0x00FFFF, 0x00FFFF, 0xFFFF00, 0xFFFF00, 0xFFFFFF,
    0xFFFFFF, 0x000000, 0x000000, 0x0000FF, 0x0000FF, 0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF,
    0x00FF00, 0x00FF00, 0x00FFFF, 0x00FFFF, 0xFFFF00, 0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000,
    0x000000, 0x0000FF, 0x0000FF, 0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00,
    0x00FFFF, 0x00FFFF, 0xFFFF00, 0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000, 0x000000, 0x0000FF,
    0x0000FF, 0xFF0000, 0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00, 0x00FFFF, 0x00FFFF,
    0xFFFF00, 0xFFFF00, 0xFFFFFF, 0xFFFFFF, 0x000000, 0x000000, 0x0000FF, 0x0000FF, 0xFF0000,
    0xFF0000, 0xFF00FF, 0xFF00FF, 0x00FF00, 0x00FF00, 0x00FFFF, 0x00FFFF, 0xFFFF00, 0xFFFF00,
    0xFFFFFF, 0xFFFFFF,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_line_counts() {
        assert_eq!(Region::Ntsc.lines_per_frame(), 262);
        assert_eq!(Region::Pal.lines_per_frame(), 312);
        assert_eq!(Region::Secam.lines_per_frame(), 312);
    }

    #[test]
    fn lookup_masks_into_range() {
        // Any 8-bit input stays in range (no panic on the top bit).
        let _ = Region::Ntsc.lookup(0xFF);
        assert_eq!(Region::Ntsc.table().len(), PALETTE_LEN);
    }
}
