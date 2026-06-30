//! Lock-free SPSC audio ring buffer.
#![allow(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

const DEFAULT_CAPACITY: usize = 16_384;

/// One audio sample (mono f32 at v0.1; the TIA mixes 2 channels to mono, then
/// the frontend may widen to stereo — see RustyNES's stereo-panning stage).
pub type Sample = f32;

/// Lock-free single-producer/single-consumer ring of f32 samples stored as
/// atomic bit patterns. Capacity is a power of two.
struct Ring {
    slots: Box<[AtomicU32]>,
    mask: usize,
}

impl Ring {
    fn new(capacity_pow2: usize) -> Self {
        debug_assert!(capacity_pow2.is_power_of_two());
        let slots = (0..capacity_pow2)
            .map(|_| AtomicU32::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            slots,
            mask: capacity_pow2 - 1,
        }
    }

    const fn capacity(&self) -> usize {
        self.mask + 1
    }
}

struct QueueInner {
    ring: Ring,
    head: AtomicUsize,
    tail: AtomicUsize,
    started: AtomicBool,
    playing: AtomicBool,
    start_threshold: AtomicUsize,
    paused: AtomicBool,
}

#[derive(Clone)]
pub struct SampleQueue {
    inner: Arc<QueueInner>,
}

impl SampleQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        Self {
            inner: Arc::new(QueueInner {
                ring: Ring::new(cap),
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                started: AtomicBool::new(false),
                playing: AtomicBool::new(false),
                start_threshold: AtomicUsize::new(0),
                paused: AtomicBool::new(false),
            }),
        }
    }

    pub fn set_start_threshold(&self, samples: usize) {
        let cap = self.inner.ring.capacity();
        self.inner
            .start_threshold
            .store(samples.min(cap / 2), Ordering::Relaxed);
    }

    pub fn push(&self, samples: &[Sample]) {
        self.inner.started.store(true, Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Relaxed);
        let head = self.inner.head.load(Ordering::Acquire);
        let free = self.inner.ring.capacity() - tail.wrapping_sub(head);
        let n = samples.len().min(free);
        for (i, &s) in samples[..n].iter().enumerate() {
            self.inner.ring.slots[tail.wrapping_add(i) & self.inner.ring.mask]
                .store(s.to_bits(), Ordering::Relaxed);
        }
        self.inner
            .tail
            .store(tail.wrapping_add(n), Ordering::Release);
    }

    pub fn pop_or_silence(&self, out: &mut [Sample]) -> usize {
        let head = self.inner.head.load(Ordering::Relaxed);
        let tail = self.inner.tail.load(Ordering::Acquire);
        let avail = tail.wrapping_sub(head);

        if self.inner.paused.load(Ordering::Relaxed) {
            out.fill(0.0);
            return 0;
        }

        if !self.inner.playing.load(Ordering::Relaxed) {
            let threshold = self.inner.start_threshold.load(Ordering::Relaxed);
            if avail >= threshold && (avail > 0 || threshold == 0) {
                self.inner.playing.store(true, Ordering::Relaxed);
            } else {
                out.fill(0.0);
                return 0;
            }
        }

        let n = out.len().min(avail);
        for (i, o) in out[..n].iter_mut().enumerate() {
            let bits = self.inner.ring.slots[head.wrapping_add(i) & self.inner.ring.mask]
                .load(Ordering::Relaxed);
            *o = f32::from_bits(bits);
        }
        self.inner
            .head
            .store(head.wrapping_add(n), Ordering::Release);

        if n < out.len() && !out.is_empty() {
            self.inner.playing.store(false, Ordering::Relaxed);
        }
        for s in out.iter_mut().skip(n) {
            *s = 0.0;
        }
        n
    }

    pub fn len(&self) -> usize {
        let tail = self.inner.tail.load(Ordering::Acquire);
        let head = self.inner.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SampleQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The dynamic-rate-control servo. Given the ring fill ratio, returns the
/// resampler ratio multiplier to apply this block (a tiny nudge around 1.0).
#[must_use]
pub fn rate_control_ratio(fill_ratio: f32) -> f32 {
    // Drift toward half-full. A +-0.5% maximum nudge keeps pitch shift inaudible.
    const MAX_NUDGE: f32 = 0.005;
    let error = 0.5 - fill_ratio; // positive => ring low => slow consumption
    error.clamp(-1.0, 1.0).mul_add(-MAX_NUDGE, 1.0)
}

/// The producer end (emu-thread).
#[derive(Clone)]
pub struct Producer(SampleQueue);

/// The consumer end (cpal callback).
#[derive(Clone)]
pub struct Consumer(SampleQueue);

#[must_use]
pub fn channel(capacity: usize) -> (Producer, Consumer) {
    let q = SampleQueue::with_capacity(capacity);
    (Producer(q.clone()), Consumer(q))
}

impl Producer {
    pub fn push(&self, s: Sample) {
        self.0.push(&[s]);
    }

    pub fn push_slice(&self, s: &[Sample]) {
        self.0.push(s);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn fill_ratio(&self) -> f32 {
        let cap = self.0.inner.ring.capacity();
        if cap == 0 {
            0.0
        } else {
            self.0.len() as f32 / cap as f32
        }
    }
}

impl Consumer {
    pub fn pull(&self) -> Sample {
        let mut buf = [0.0; 1];
        self.0.pop_or_silence(&mut buf);
        buf[0]
    }

    pub fn pop_or_silence(&self, out: &mut [Sample]) -> usize {
        self.0.pop_or_silence(out)
    }
}
