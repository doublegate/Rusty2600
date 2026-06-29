//! The audio ring + dynamic rate control — and where the determinism contract
//! is enforced.
//!
//! The core draws samples FROM the TIA ([`rusty2600_core::AudioBus`]). The
//! frontend pulls them through a lock-free single-producer/single-consumer ring
//! into the cpal output callback. The producer is the emu-thread; the consumer
//! is the cpal audio thread.
//!
//! ## The determinism contract lives HERE, never in the core
//!
//! The core's synthesis is fixed-rate and bit-exact (same seed + ROM + input =>
//! bit-identical samples). To match the host's actual output clock without
//! perturbing that synthesis, the frontend runs **dynamic rate control**: a
//! resampler stage nudges the effective sample rate by a tiny ratio derived from
//! the ring's fill level (drifting toward "half full"). This is a FRONTEND
//! resampler stage — the core never sees it. Likewise **run-ahead** (snapshot ->
//! advance N frames -> restore -> re-advance with the latched input) is
//! frontend snapshot/restore orchestration, never a core concern. Keeping both
//! out of the core is exactly what preserves the determinism contract.
//!
//! Lifted in SHAPE from RustyNES `resampler.rs` + the audio ring. v0.1 keeps the
//! ring as a `Vec`-backed mutex queue with the rate-control hook present (a
//! `// TODO` to swap in the lock-free SPSC + the cubic resampler).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// One audio sample (mono f32 at v0.1; the TIA mixes 2 channels to mono, then
/// the frontend may widen to stereo — see RustyNES's stereo-panning stage).
pub type Sample = f32;

/// The lock-free SPSC ring (scaffold: mutex-guarded `VecDeque`).
///
/// TODO(T-PS-059): replace with RustyNES's lock-free SPSC (a fixed power-of-two
/// buffer + atomic head/tail). The audio callback MUST be allocation-free and
/// wait-free; the mutex here is a v0.1 placeholder.
#[derive(Debug, Default)]
struct Ring {
    queue: VecDeque<Sample>,
    capacity: usize,
}

/// The producer end (emu-thread): pushes synthesized samples.
#[derive(Debug, Clone)]
pub struct Producer(Arc<Mutex<Ring>>);

/// The consumer end (cpal callback): drains samples to the device.
#[derive(Debug, Clone)]
pub struct Consumer(Arc<Mutex<Ring>>);

/// Create a connected producer/consumer pair sized for `capacity` samples.
#[must_use]
pub fn channel(capacity: usize) -> (Producer, Consumer) {
    let ring = Arc::new(Mutex::new(Ring {
        queue: VecDeque::with_capacity(capacity),
        capacity,
    }));
    (Producer(Arc::clone(&ring)), Consumer(ring))
}

impl Producer {
    /// Push one sample. Drops the oldest sample on overflow (better a tiny
    /// glitch than unbounded latency) — the rate-control servo keeps the ring
    /// near half-full so overflow is rare.
    pub fn push(&self, s: Sample) {
        if let Ok(mut ring) = self.0.lock() {
            if ring.queue.len() == ring.capacity {
                ring.queue.pop_front();
            }
            ring.queue.push_back(s);
        }
    }

    /// The current fill ratio (`0.0`..=`1.0`), the input to the rate-control
    /// servo. The servo targets `0.5`.
    #[must_use]
    pub fn fill_ratio(&self) -> f32 {
        self.0.lock().map_or(0.0, |ring| {
            if ring.capacity == 0 {
                0.0
            } else {
                // The queue length + capacity are small ring indices (far below f32's 2^23
                // mantissa limit), so the cast is exact at the sizes this ring ever holds.
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "ring sizes are far below the f32 mantissa limit"
                )]
                let ratio = ring.queue.len() as f32 / ring.capacity as f32;
                ratio
            }
        })
    }
}

impl Consumer {
    /// Pull the next sample, or `0.0` (silence) on underrun.
    #[must_use]
    pub fn pull(&self) -> Sample {
        self.0
            .lock()
            .ok()
            .and_then(|mut ring| ring.queue.pop_front())
            .unwrap_or(0.0)
    }
}

/// The dynamic-rate-control servo. Given the ring fill ratio, returns the
/// resampler ratio multiplier to apply this block (a tiny nudge around 1.0).
///
/// This is the frontend stage that reconciles the core's fixed synthesis rate
/// with the host output clock WITHOUT touching the core. TODO(T-PS-060): replace
/// the proportional nudge with RustyNES's measured DRC + the cubic resampler.
#[must_use]
pub fn rate_control_ratio(fill_ratio: f32) -> f32 {
    // Drift toward half-full. A +-0.5% maximum nudge keeps pitch shift inaudible.
    const MAX_NUDGE: f32 = 0.005;
    let error = 0.5 - fill_ratio; // positive => ring low => slow consumption
    error.clamp(-1.0, 1.0).mul_add(-MAX_NUDGE, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pull_roundtrips() {
        let (tx, rx) = channel(4);
        tx.push(0.25);
        tx.push(-0.5);
        assert!((rx.pull() - 0.25).abs() < f32::EPSILON);
        assert!((rx.pull() + 0.5).abs() < f32::EPSILON);
        // Underrun => silence. Exact-sentinel compare: `pull` returns a literal `0.0`.
        #[allow(clippy::float_cmp, reason = "exact silence sentinel from pull()")]
        let silent = rx.pull() == 0.0;
        assert!(silent);
    }

    #[test]
    fn overflow_drops_oldest() {
        let (tx, rx) = channel(2);
        tx.push(1.0);
        tx.push(2.0);
        tx.push(3.0); // evicts 1.0
        assert!((rx.pull() - 2.0).abs() < f32::EPSILON);
        assert!((rx.pull() - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rate_control_nudges_toward_half_full() {
        // Empty ring => slow the consumer (ratio < 1).
        assert!(rate_control_ratio(0.0) < 1.0);
        // Full ring => speed up (ratio > 1).
        assert!(rate_control_ratio(1.0) > 1.0);
        // Balanced => no change.
        assert!((rate_control_ratio(0.5) - 1.0).abs() < f32::EPSILON);
    }
}
