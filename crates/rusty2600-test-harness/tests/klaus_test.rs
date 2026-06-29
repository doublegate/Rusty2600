#![allow(warnings)]
use rusty2600_cpu::{Cpu, CpuBus};
use std::fs;

struct KlausBus {
    ram: [u8; 65536],
}

impl CpuBus for KlausBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.ram[addr as usize] = val;
    }
}

#[test]
fn klaus_functional_test() {
    let mut bus = KlausBus { ram: [0; 65536] };
    // Klaus 6502 test is loaded in the root at tests/roms/test_suite/6502_functional_test.bin
    // We are running this test from `crates/rusty2600-test-harness` directory
    let rom = fs::read("../../tests/roms/test_suite/6502_functional_test.bin").expect(
        "Failed to load Klaus 6502 test binary. Make sure to download it to tests/roms/test_suite/",
    );

    assert_eq!(rom.len(), 65536);
    bus.ram.copy_from_slice(&rom);

    let mut cpu = Cpu::power_on();
    cpu.reset(&mut bus);
    cpu.set_pc(0x0400); // Klaus functional test entry point

    let mut clocks = 0;
    while clocks < 200_000_000 {
        let prev_pc = cpu.pc;
        cpu.step(&mut bus);

        if cpu.pc == 0x3469 {
            // Success trap!
            return;
        } else if cpu.pc == prev_pc {
            // Infinite loop -> failure trap
            panic!("Klaus test failed at PC={:04X} (trapped)", cpu.pc);
        }
        clocks += 1;
    }
    panic!("Klaus test timed out!");
}
