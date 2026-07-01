//! Criterion bench for `rusty2600-riot` (each chip is benchmarkable in isolation —
//! that is what the one-directional crate graph buys us).
#![allow(missing_docs)] // criterion_group!/main! expand into undocumented fns.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rusty2600_riot::Riot;

/// One CPU cycle's worth of RIOT work (the interval timer's prescale
/// countdown, called once per CPU cycle from the scheduler).
fn bench_tick(c: &mut Criterion) {
    let mut riot = Riot::new();
    riot.cpu_write(0x0296, 0xFF); // TIM64T, a representative prescale
    c.bench_function("riot_tick", |b| {
        b.iter(|| {
            riot.tick();
        });
    });
}

/// RAM read/write throughput (the 2600's only general-purpose RAM).
fn bench_ram_access(c: &mut Criterion) {
    let mut riot = Riot::new();
    c.bench_function("riot_ram_write_then_read", |b| {
        b.iter(|| {
            riot.cpu_write(0x0080, black_box(0x42));
            black_box(riot.cpu_read(0x0080));
        });
    });
}

criterion_group!(benches, bench_tick, bench_ram_access);
criterion_main!(benches);
