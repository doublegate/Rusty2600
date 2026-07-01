//! Gated behind `test-roms`: both tests here load a committed ROM binary and
//! the decimal test in particular runs long enough (hundreds of millions of
//! instructions, exhaustively sweeping every `ADC`/`SBC` decimal-mode input x
//! carry-in combination) that it does not belong in the fast default
//! `cargo test --workspace` path — `ci.yml` already runs a separate
//! `--features test-roms` step every push, so no coverage is lost.
#![cfg(feature = "test-roms")]
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

/// Bruce Clark's decimal-mode test (public domain,
/// <http://www.6502.org/tutorials/decimal_mode.html>), assembled from
/// `tests/roms/Klaus2m5/6502_decimal_test.a65` (the bundled `as65` 1.42
/// assembler, `cputype = 0` i.e. plain NMOS 6502). Exhaustively checks every
/// `ADC`/`SBC` decimal-mode input/carry-in combination against the predicted
/// binary-arithmetic result, leaving a pass/fail byte at zero-page `$0B`
/// (`ERROR` — "ERROR = 0 if the test passed, ERROR = 1 if the test failed"
/// per the source's own header).
///
/// The `DONE` label's `end_of_test` macro emits raw `$DB`, documented in the
/// same macro as "execute 65C02 stop instruction" — true on a 65C02, but on
/// the `cputype = 0` (plain NMOS 6502) build this test is actually
/// configured for, `$DB` is the real illegal opcode **DCP absolute,Y**
/// (confirmed correct here — it's one of the 233 opcodes the SingleStepTests
/// audit already validates), which does NOT halt anything: it just executes
/// as a normal 7-cycle instruction and falls through into whatever follows.
/// The source's own top-of-file comment even says "modify the code at the
/// DONE label for desired program end" — i.e. the raw `$DB` byte is a
/// placeholder the integrator is expected to replace or trap around, not a
/// real halt condition for this CPU type. So this traps on **reaching the
/// `DONE` address ($024B) directly**, the same PC-based pattern
/// `klaus_functional_test` above uses, checked BEFORE that instruction gets
/// a chance to execute (executing the DCP would otherwise run PC past DONE
/// into unprogrammed memory and off into a spurious BRK loop — exactly what
/// happens if this check is missing, as found while wiring this test).
#[test]
fn klaus_decimal_test() {
    const DONE_ADDR: u16 = 0x024B;

    let mut bus = KlausBus { ram: [0; 65536] };
    let rom = fs::read("../../tests/roms/test_suite/6502_decimal_test.bin")
        .expect("Failed to load the assembled Klaus decimal test binary.");
    bus.ram[0x0200..0x0200 + rom.len()].copy_from_slice(&rom);

    let mut cpu = Cpu::power_on();
    cpu.reset(&mut bus);
    cpu.set_pc(0x0200); // the source's `org $200` code entry point

    let mut clocks = 0;
    // The exhaustive 256 x 256 x 2-carry-in sweep needs far more instructions
    // than the functional test's budget above.
    while clocks < 700_000_000 {
        if cpu.pc == DONE_ADDR {
            let error = bus.read(0x000B);
            assert_eq!(
                error, 0,
                "Klaus decimal test failed: ERROR=${error:02X} at zero-page $0B (0 = pass, 1 = fail)"
            );
            return;
        }
        cpu.step(&mut bus);
        clocks += 1;
    }
    panic!("Klaus decimal test timed out!");
}
