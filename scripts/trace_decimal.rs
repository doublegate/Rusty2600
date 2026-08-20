use rusty2600_cpu::{Cpu, CpuBus};
use std::fs;
use std::collections::HashMap;

struct TraceBus { ram: [u8; 65536] }
impl CpuBus for TraceBus {
    fn read(&mut self, addr: u16) -> u8 { self.ram[addr as usize] }
    fn write(&mut self, addr: u16, val: u8) { self.ram[addr as usize] = val; }
}

fn main() {
    let mut bus = TraceBus { ram: [0; 65536] };
    let rom = fs::read("tests/roms/test_suite/6502_decimal_test.bin").unwrap();
    bus.ram[0x0200..0x0200 + rom.len()].copy_from_slice(&rom);
    let mut cpu = Cpu::power_on();
    cpu.reset(&mut bus);
    cpu.set_pc(0x0200);

    let mut pc_counts: HashMap<u16, u32> = HashMap::new();
    for i in 0..2_000_000u32 {
        let pc = cpu.pc;
        cpu.step(&mut bus);
        if cpu.jammed {
            println!("JAMMED at step {i}, PC was {pc:04X}, ERROR={:02X}", bus.read(0x0B));
            return;
        }
        *pc_counts.entry(pc).or_insert(0) += 1;
        if i % 200_000 == 0 {
            println!("step {i}: pc={:04X} a={:02X} x={:02X} y={:02X}", cpu.pc, cpu.a, cpu.x, cpu.y);
        }
    }
    let mut top: Vec<_> = pc_counts.into_iter().collect();
    top.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
    println!("Top PCs by visit count:");
    for (pc, c) in top.iter().take(10) {
        println!("  {pc:04X}: {c} times");
    }
}
