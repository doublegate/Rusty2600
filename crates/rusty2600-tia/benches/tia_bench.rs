//! Criterion bench for `rusty2600-tia` (each chip is benchmarkable in isolation —
//! that is what the one-directional crate graph buys us).
#![allow(missing_docs)] // criterion_group!/main! expand into undocumented fns.

use criterion::{Criterion, criterion_group, criterion_main};
use rusty2600_tia::Tia;
use std::hint::black_box;

/// One color clock, the TIA's own per-tick unit of work (beam-raced video +
/// audio poly-counter advance).
fn bench_tick_color_clock(c: &mut Criterion) {
    let mut tia = Tia::new();
    c.bench_function("tia_tick_color_clock", |b| {
        b.iter(|| {
            tia.tick_color_clock();
        });
    });
}

/// One full NTSC frame (262 scanlines x 228 color clocks) — the real unit of
/// work the frontend pays per rendered frame.
fn bench_full_ntsc_frame(c: &mut Criterion) {
    c.bench_function("tia_full_ntsc_frame", |b| {
        b.iter(|| {
            let mut tia = Tia::new();
            for _ in 0..262 * 228 {
                tia.tick_color_clock();
            }
            black_box(&tia);
        });
    });
}

criterion_group!(benches, bench_tick_color_clock, bench_full_ntsc_frame);
criterion_main!(benches);
