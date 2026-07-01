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
//! The RGB values are the measured Stella / TIA palettes (see
//! `ref-proj/stella/src/common/PaletteHandler.cxx`, `ref-docs/` for the immutable
//! hardware reference, and `docs/tia.md`).

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

    /// The 128-entry RGB lookup table for this region. SECAM is an 8-colour set
    /// splayed across the 128-entry layout (chroma ignored, luma selects).
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

// Measured Stella reference palettes (`ref-proj/stella/src/common/PaletteHandler.cxx`,
// `ourNTSCPalette` / `ourPALPalette` / SECAM's fixed 8-colour set), Stella being this
// project's behavioural oracle (`docs/architecture.md` "Test ROMs are the spec",
// `docs/testing-strategy.md`).
// Indexed `hue * 8 + luma` (16 hues x 8 lumas = 128), matching `colour7 = colu >> 1`: COLUxx
// bits 7..4 are hue, bits 3..1 are luma, bit 0 is unused, so `colu >> 1` packs to exactly
// `hue << 3 | luma`. Earlier stub data wrongly duplicated each entry into 16-wide hue blocks
// (8 real hues instead of 16, every odd hue aliasing into a neighbouring hue's colour) — this
// was the root cause of the reported severe palette corruption / banding on real ROMs (e.g.
// Frogger's road/median/water/grass bands collapsing into one or two flat colours).
//
// Left as plain 0xRRGGBB literals (not grouped `0xRRGG_BBxx`) to stay byte-comparable against
// the Stella source they were transcribed from.
#[allow(clippy::unreadable_literal)]
const NTSC: [Rgb; PALETTE_LEN] = [
    0x000000, 0x4A4A4A, 0x6F6F6F, 0x8E8E8E, 0xAAAAAA, 0xC0C0C0, 0xD6D6D6, 0xECECEC, 0x484800,
    0x69690F, 0x86861D, 0xA2A22A, 0xBBBB35, 0xD2D240, 0xE8E84A, 0xFCFC54, 0x7C2C00, 0x904811,
    0xA26221, 0xB47A30, 0xC3903D, 0xD2A44A, 0xDFB755, 0xECC860, 0x901C00, 0xA33915, 0xB55328,
    0xC66C3A, 0xD5824A, 0xE39759, 0xF0AA67, 0xFCBC74, 0x940000, 0xA71A1A, 0xB83232, 0xC84848,
    0xD65C5C, 0xE46F6F, 0xF08080, 0xFC9090, 0x840064, 0x97197A, 0xA8308F, 0xB846A2, 0xC659B3,
    0xD46CC3, 0xE07CD2, 0xEC8CE0, 0x500084, 0x68199A, 0x7D30AD, 0x9246C0, 0xA459D0, 0xB56CE0,
    0xC57CEE, 0xD48CFC, 0x140090, 0x331AA3, 0x4E32B5, 0x6848C6, 0x7F5CD5, 0x956FE3, 0xA980F0,
    0xBC90FC, 0x000094, 0x181AA7, 0x2D32B8, 0x4248C8, 0x545CD6, 0x656FE4, 0x7580F0, 0x8490FC,
    0x001C88, 0x183B9D, 0x2D57B0, 0x4272C2, 0x548AD2, 0x65A0E1, 0x75B5EF, 0x84C8FC, 0x003064,
    0x185080, 0x2D6D98, 0x4288B0, 0x54A0C5, 0x65B7D9, 0x75CCEB, 0x84E0FC, 0x004030, 0x18624E,
    0x2D8169, 0x429E82, 0x54B899, 0x65D1AE, 0x75E7C2, 0x84FCD4, 0x004400, 0x1A661A, 0x328432,
    0x48A048, 0x5CBA5C, 0x6FD26F, 0x80E880, 0x90FC90, 0x143C00, 0x355F18, 0x527E2D, 0x6E9C42,
    0x87B754, 0x9ED065, 0xB4E775, 0xC8FC84, 0x303800, 0x505916, 0x6D762B, 0x88923E, 0xA0AB4F,
    0xB7C25F, 0xCCD86E, 0xE0EC7C, 0x482C00, 0x694D14, 0x866A26, 0xA28638, 0xBB9F47, 0xD2B656,
    0xE8CC63, 0xFCE070,
];
#[allow(clippy::unreadable_literal)]
const PAL: [Rgb; PALETTE_LEN] = [
    0x0B0B0B, 0x333333, 0x595959, 0x7B7B7B, 0x999999, 0xB6B6B6, 0xCFCFCF, 0xE6E6E6, 0x0B0B0B,
    0x333333, 0x595959, 0x7B7B7B, 0x999999, 0xB6B6B6, 0xCFCFCF, 0xE6E6E6, 0x3B2400, 0x664700,
    0x8B7000, 0xAC9200, 0xC5AE36, 0xDEC85E, 0xF7E27F, 0xFFF19E, 0x004500, 0x006F00, 0x3B9200,
    0x65B009, 0x85CA3D, 0xA3E364, 0xBFFC84, 0xD5FFA5, 0x590000, 0x802700, 0xA15700, 0xBC7937,
    0xD6985F, 0xEEB381, 0xFFCE9E, 0xFFDCBD, 0x004900, 0x007200, 0x169216, 0x45AF45, 0x6BC96B,
    0x8BE38B, 0xA9FBA9, 0xC5FFC5, 0x640012, 0x890821, 0xA73D4D, 0xC26472, 0xDC8491, 0xF4A3AE,
    0xFFBECA, 0xFFDAE0, 0x003D29, 0x006A48, 0x048E63, 0x3CAA84, 0x62C5A2, 0x83DFBE, 0xA1F8D9,
    0xBEFFE9, 0x550046, 0x88006E, 0xA5318D, 0xC159AA, 0xDA7CC5, 0xF39ADF, 0xFFB9F3, 0xFFD4F6,
    0x003651, 0x005A7D, 0x117E9C, 0x429CB8, 0x68B7D2, 0x88D2EB, 0xA6EBFF, 0xC3FFFF, 0x4C007C,
    0x75009D, 0x932EB8, 0xAF57D2, 0xCA7AEB, 0xE499FF, 0xECB7FF, 0xF3D4FF, 0x002D83, 0x003EA4,
    0x2D65BF, 0x5685DA, 0x79A2F2, 0x99BFFF, 0xB7DBFF, 0xD3F5FF, 0x220096, 0x5200B6, 0x7538CF,
    0x945FE8, 0xB181FF, 0xC5A0FF, 0xD6BDFF, 0xE8DAFF, 0x00009A, 0x241DB6, 0x504AD0, 0x746FE9,
    0x928EFF, 0xB1ADFF, 0xCECAFF, 0xE9E5FF, 0x0B0B0B, 0x333333, 0x595959, 0x7B7B7B, 0x999999,
    0xB6B6B6, 0xCFCFCF, 0xE6E6E6, 0x0B0B0B, 0x333333, 0x595959, 0x7B7B7B, 0x999999, 0xB6B6B6,
    0xCFCFCF, 0xE6E6E6,
];
// SECAM ignores chroma entirely: luminance (the low 3 bits) selects one of 8 fixed colours,
// broadcast unchanged across all 16 hue groups (Stella's `ourSECAMPalette`).
#[allow(clippy::unreadable_literal)]
const SECAM: [Rgb; PALETTE_LEN] = {
    const ROW: [Rgb; 8] = [
        0x000000, 0x2121FF, 0xF03C79, 0xFF50FF, 0x7FFF00, 0x7FFFFF, 0xFFFF3F, 0xFFFFFF,
    ];
    let mut table = [0u32; PALETTE_LEN];
    let mut hue = 0;
    while hue < 16 {
        let mut luma = 0;
        while luma < 8 {
            table[hue * 8 + luma] = ROW[luma];
            luma += 1;
        }
        hue += 1;
    }
    table
};

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
    fn ntsc_hues_are_8_wide_not_16_wide() {
        // Regression for the "every odd hue aliases into its neighbour" bug:
        // index = hue*8 + luma, so hue 0 and hue 1 must NOT share entries.
        // (The old stub duplicated every entry into 16-wide blocks, so what
        // should have been hue=1,luma=0 at index 8 actually returned hue=0's
        // luma=4 colour.)
        let hue0_luma0 = Region::Ntsc.lookup(0); // hue 0, luma 0
        let hue1_luma0 = Region::Ntsc.lookup(8); // hue 1, luma 0
        assert_ne!(hue0_luma0, hue1_luma0);
        // Hue 0 is the grey ramp: luma must strictly increase in brightness.
        let l0 = Region::Ntsc.lookup(0);
        let l4 = Region::Ntsc.lookup(4);
        let l7 = Region::Ntsc.lookup(7);
        assert!(l0 < l4 && l4 < l7);
    }

    #[test]
    fn lookup_masks_into_range() {
        // Any 8-bit input stays in range (no panic on the top bit).
        let _ = Region::Ntsc.lookup(0xFF);
        assert_eq!(Region::Ntsc.table().len(), PALETTE_LEN);
    }
}
