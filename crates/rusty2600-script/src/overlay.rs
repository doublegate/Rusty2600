//! [`Overlay`] — the accumulated draw primitives from `emu.drawText`/
//! `drawRect`/`drawPixel`, for a host to composite over the emulated frame.
//!
//! **Compositing is not wired in this release.** `ScriptEngine` accumulates
//! primitives into an `Overlay` and hands it to the host via
//! [`crate::ScriptEngine::take_overlay`] every frame, but no
//! `rusty2600-frontend` render-path code consumes it yet — the actual wgpu
//! blend-over-the-emulated-frame step is a real, separate integration task
//! (touching `gfx.rs`/`shader_pass.rs`), deliberately left for a follow-up
//! rather than rushed here, the same honest-partial-landing call this
//! project already made for `[1.4.0]`'s sprite-pack render splice and
//! `[1.7.0]`'s live movie-recording wiring.

/// One `emu.drawText(x, y, text)` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPrimitive {
    /// X position, in emulated-frame pixels (0..=159).
    pub x: i32,
    /// Y position, in emulated-frame pixels.
    pub y: i32,
    /// The text to draw.
    pub text: String,
}

/// One `emu.drawRect(x, y, w, h, color)` call. `color` is packed `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectPrimitive {
    /// X position of the rectangle's top-left corner.
    pub x: i32,
    /// Y position of the rectangle's top-left corner.
    pub y: i32,
    /// Width in pixels.
    pub w: i32,
    /// Height in pixels.
    pub h: i32,
    /// Packed `0xRRGGBB` color.
    pub color: u32,
}

/// One `emu.drawPixel(x, y, color)` call. `color` is packed `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelPrimitive {
    /// X position.
    pub x: i32,
    /// Y position.
    pub y: i32,
    /// Packed `0xRRGGBB` color.
    pub color: u32,
}

/// The primitives a script drew during the current frame, in call order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// Every `emu.drawText` call this frame, in order.
    pub texts: Vec<TextPrimitive>,
    /// Every `emu.drawRect` call this frame, in order.
    pub rects: Vec<RectPrimitive>,
    /// Every `emu.drawPixel` call this frame, in order.
    pub pixels: Vec<PixelPrimitive>,
}

impl Overlay {
    /// Drops every accumulated primitive (called once the host has consumed
    /// a frame's overlay, so the next frame starts empty).
    pub fn clear(&mut self) {
        self.texts.clear();
        self.rects.clear();
        self.pixels.clear();
    }

    /// Whether no primitives were drawn this frame.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.texts.is_empty() && self.rects.is_empty() && self.pixels.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        assert!(Overlay::default().is_empty());
    }

    #[test]
    fn clear_empties_all_three_lists() {
        let mut overlay = Overlay {
            texts: vec![TextPrimitive {
                x: 0,
                y: 0,
                text: "hi".to_string(),
            }],
            rects: vec![RectPrimitive {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
                color: 0,
            }],
            pixels: vec![PixelPrimitive {
                x: 0,
                y: 0,
                color: 0,
            }],
        };
        assert!(!overlay.is_empty());
        overlay.clear();
        assert!(overlay.is_empty());
    }
}
