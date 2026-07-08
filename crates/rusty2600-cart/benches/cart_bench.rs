//! Criterion bench for `rusty2600-cart` (each chip is benchmarkable in isolation —
//! that is what the one-directional crate graph buys us).
#![allow(missing_docs)] // criterion_group!/main! expand into undocumented fns.

use criterion::{Criterion, criterion_group, criterion_main};
use rusty2600_cart::{Board, detect};
use std::hint::black_box;

/// F8 (the most common classic Curated scheme) read/write throughput,
/// including its hotspot bank-switch check on every access.
fn bench_bankf8(c: &mut Criterion) {
    let rom = vec![0u8; 0x2000];
    let mut board = detect(&rom).expect("8 KiB resolves to BankF8");
    c.bench_function("cart_bankf8_read_write", |b| {
        b.iter(|| {
            black_box(board.cpu_read(0x1000));
            board.cpu_write(black_box(0x1FF9), 0);
        });
    });
}

/// DPC (the most involved Curated board: RNG + 8 data fetchers) read
/// throughput — the ceiling case for per-access cart overhead.
fn bench_dpc(c: &mut Criterion) {
    let rom = vec![0u8; 0x2800];
    let mut board = detect(&rom).expect("10 KiB resolves to BankDpc");
    c.bench_function("cart_dpc_register_read", |b| {
        b.iter(|| {
            black_box(board.cpu_read(0x1008)); // a data-fetcher display read
        });
    });
}

criterion_group!(benches, bench_bankf8, bench_dpc);
criterion_main!(benches);
