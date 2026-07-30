//! [`Overlay`] — the accumulated draw primitives from `emu.drawText`/
//! `drawRect`/`drawPixel`/`drawLine`, for a host to composite over the
//! emulated frame.
//!
//! **Compositing is wired** (`[2.3.0]`, extended `[2.13.0]` with
//! `drawLine`). `ScriptEngine` accumulates primitives into an `Overlay` and
//! hands it to the host via [`crate::ScriptEngine::take_overlay`] every
//! frame; `rusty2600-frontend`'s `app.rs::draw_script_overlay` composites
//! every primitive over the displayed framebuffer via an egui foreground
//! layer painter (a scripting-feature-gated overlay pass, piggybacked on
//! the frontend's existing egui pass rather than a new wgpu blend step).

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

/// One `emu.drawLine(x1, y1, x2, y2, color)` call. `color` is packed
/// `0xRRGGBB`. `[2.13.0]` — the fourth HUD primitive, matching the sibling
/// `RustyNES` project's own `emu.drawLine` at parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePrimitive {
    /// X position of the line's start point.
    pub x1: i32,
    /// Y position of the line's start point.
    pub y1: i32,
    /// X position of the line's end point.
    pub x2: i32,
    /// Y position of the line's end point.
    pub y2: i32,
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
    /// Every `emu.drawLine` call this frame, in order (`[2.13.0]`).
    pub lines: Vec<LinePrimitive>,
}

impl Overlay {
    /// Drops every accumulated primitive (called once the host has consumed
    /// a frame's overlay, so the next frame starts empty).
    pub fn clear(&mut self) {
        self.texts.clear();
        self.rects.clear();
        self.pixels.clear();
        self.lines.clear();
    }

    /// Whether no primitives were drawn this frame.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.texts.is_empty()
            && self.rects.is_empty()
            && self.pixels.is_empty()
            && self.lines.is_empty()
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
    fn clear_empties_all_four_lists() {
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
            lines: vec![LinePrimitive {
                x1: 0,
                y1: 0,
                x2: 1,
                y2: 1,
                color: 0,
            }],
        };
        assert!(!overlay.is_empty());
        overlay.clear();
        assert!(overlay.is_empty());
    }
}
