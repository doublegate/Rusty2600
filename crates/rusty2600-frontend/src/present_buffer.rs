//! The beam-raced display buffer + the triple-buffer producer/consumer handoff.
//!
//! The TIA has no framebuffer (it races the beam), so the FRONTEND owns the
//! accumulation: as the emu-thread advances color clocks it writes the emitted
//! `(luma, chroma)` of each visible dot into a display buffer; once a full frame
//! lands, the buffer is published to the winit thread for upload + present.
//!
//! Lifted in SHAPE from RustyNES `present_buffer.rs`: a triple-buffer so the
//! producer (emu-thread) and consumer (winit present) never block each other and
//! never tear — the producer writes the back buffer, atomically swaps it into a
//! "ready" slot, and the consumer takes the most-recent ready slot. The RustyNES
//! version is a lock-free 3-slot ring; this v0.1 scaffold keeps the shape with a
//! mutex-guarded most-recent slot (a `// TODO` to go lock-free) so it compiles
//! without the heavier atomics plumbing.
//!
//! 2600 display geometry: ~160 visible color clocks wide x ~192 visible lines
//! (NTSC; the visible window excludes the 68-clock HBLANK + the top/bottom
//! VBLANK lines). 2600 pixels are tall — the natural display stretches the
//! 160-wide buffer ~1.8x horizontally to a 4:3 frame. v0.1 records the geometry;
//! it does not perfect the aspect.

use std::sync::{Arc, Mutex};

/// Visible width in TIA color clocks (one dot per clock across the active line).
pub const VISIBLE_WIDTH: usize = 160;

/// Visible scanlines (NTSC active region). PAL/SECAM expose more; the buffer is
/// sized for the larger case at runtime — this is the NTSC default.
pub const VISIBLE_HEIGHT_NTSC: usize = 192;

/// Visible scanlines (PAL / SECAM active region — the taller of the two cases the
/// gfx texture is sized for).
pub const VISIBLE_HEIGHT_PAL: usize = 228;

/// One published frame: `width * height` RGBA8 pixels (the form wgpu uploads).
#[derive(Debug, Clone)]
pub struct Frame {
    /// Pixel width.
    pub width: usize,
    /// Pixel height.
    pub height: usize,
    /// `width * height` RGBA8 pixels, row-major.
    pub pixels: Vec<u8>,
}

impl Frame {
    /// Allocate a black frame of the given dimensions.
    #[must_use]
    pub fn black(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }

    /// Write one beam dot as an RGB colour (alpha forced opaque). Out-of-range
    /// coordinates are ignored (the beam can run past the visible window).
    ///
    /// TODO(T-0501-008): this is the per-dot accumulation the emu-thread calls;
    /// wire it to the TIA's emitted `(luma, chroma)` -> [`crate::palette`] RGB.
    pub fn put_dot(&mut self, x: usize, y: usize, rgb: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = (y * self.width + x) * 4;
        self.pixels[i] = ((rgb >> 16) & 0xFF) as u8;
        self.pixels[i + 1] = ((rgb >> 8) & 0xFF) as u8;
        self.pixels[i + 2] = (rgb & 0xFF) as u8;
        self.pixels[i + 3] = 0xFF;
    }
}

/// The producer/consumer handoff. The emu-thread holds the [`Producer`]; the
/// winit present holds the [`Consumer`]. Both share one most-recent slot.
///
/// TODO(T-0501-009): replace the mutex slot with RustyNES's lock-free 3-slot ring
/// (back / ready / front + atomic index swap) so the present path never blocks
/// the emu-thread.
#[derive(Debug, Default)]
struct Shared {
    ready: Mutex<Option<Frame>>,
}

/// The producer end (emu-thread side): publishes completed frames.
#[derive(Debug, Clone)]
pub struct Producer(Arc<Shared>);

/// The consumer end (winit side): takes the most-recent completed frame.
#[derive(Debug, Clone)]
pub struct Consumer(Arc<Shared>);

/// Create a connected producer/consumer pair.
#[must_use]
pub fn channel() -> (Producer, Consumer) {
    let shared = Arc::new(Shared::default());
    (Producer(Arc::clone(&shared)), Consumer(shared))
}

impl Producer {
    /// Publish a completed frame (replacing any not-yet-consumed one — the
    /// consumer always wants the freshest frame, never a backlog).
    pub fn publish(&self, frame: Frame) {
        if let Ok(mut slot) = self.0.ready.lock() {
            *slot = Some(frame);
        }
    }
}

impl Consumer {
    /// Take the most-recent published frame, if any has landed since the last
    /// take. Returns `None` when no new frame is ready.
    #[must_use]
    pub fn take(&self) -> Option<Frame> {
        self.0.ready.lock().ok().and_then(|mut slot| slot.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_then_take_roundtrips() {
        let (tx, rx) = channel();
        assert!(rx.take().is_none());
        tx.publish(Frame::black(VISIBLE_WIDTH, VISIBLE_HEIGHT_NTSC));
        let f = rx.take().expect("a frame was published");
        assert_eq!(f.width, VISIBLE_WIDTH);
        assert_eq!(f.height, VISIBLE_HEIGHT_NTSC);
        // Consumed; the next take is empty until a new publish.
        assert!(rx.take().is_none());
    }

    #[test]
    fn put_dot_writes_rgba() {
        let mut f = Frame::black(2, 2);
        f.put_dot(1, 0, 0x00FF_8040);
        assert_eq!(&f.pixels[4..8], &[0xFF, 0x80, 0x40, 0xFF]);
    }
}
