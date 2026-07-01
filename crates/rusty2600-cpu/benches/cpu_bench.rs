//! Criterion bench for `rusty2600-cpu` (each chip is benchmarkable in isolation —
//! that is what the one-directional crate graph buys us).
#![allow(missing_docs)] // criterion_group!/main! expand into undocumented fns.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rusty2600_cpu::{Cpu, CpuBus};

/// A flat 8 KiB RAM bus with no timing model — isolates the CPU's own
/// decode/execute cost from TIA/RIOT/cart overhead, matching the crate's
/// own `FlatBus` test harness.
struct FlatBus {
    mem: [u8; 0x2000],
}

impl CpuBus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[(addr & 0x1FFF) as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mem[(addr & 0x1FFF) as usize] = val;
    }
}

/// A short, self-looping program exercising a representative mix of
/// addressing modes (immediate, zero-page, zero-page-indexed, absolute,
/// relative branch) rather than just `NOP`, so the bench reflects realistic
/// decode cost.
fn make_bus() -> FlatBus {
    let mut mem = [0u8; 0x2000];
    let prog: &[u8] = &[
        0xA9, 0x01, // LDA #$01
        0x85, 0x10, // STA $10
        0xA6, 0x10, // LDX $10
        0xBD, 0x00, 0x00, // LDA $0000,X
        0x69, 0x01, // ADC #$01
        0xC9, 0x10, // CMP #$10
        0xD0, 0xF0, // BNE -16 (back to LDA #$01)
    ];
    mem[0x1000..0x1000 + prog.len()].copy_from_slice(prog);
    // Reset vector -> $1000 (mirrors into $1FFC/$1FFD within the 8 KiB window).
    mem[0x1FFC] = 0x00;
    mem[0x1FFD] = 0x10;
    FlatBus { mem }
}

fn bench_step(c: &mut Criterion) {
    let mut bus = make_bus();
    let mut cpu = Cpu::power_on();
    cpu.reset(&mut bus);

    c.bench_function("cpu_step_mixed_addressing_modes", |b| {
        b.iter(|| {
            black_box(cpu.step(&mut bus));
        });
    });
}

criterion_group!(benches, bench_step);
criterion_main!(benches);
