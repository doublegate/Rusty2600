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
const NTSC: [Rgb; PALETTE_LEN] = [0; PALETTE_LEN];
const PAL: [Rgb; PALETTE_LEN] = [0; PALETTE_LEN];
const SECAM: [Rgb; PALETTE_LEN] = [0; PALETTE_LEN];

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
