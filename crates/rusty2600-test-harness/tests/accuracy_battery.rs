//! The accuracy battery itself (Layer 4, `docs/testing-strategy.md`):
//! aggregates the bundled Layer 2 (CPU-only) oracle results into a real
//! [`AccuracyScore`] and gates the whole workspace on the v1.0 pass-rate
//! target from `docs/STATUS.md`'s "Version policy" (>=90%, 100% the goal).
//!
//! **Honesty note.** Today's battery aggregates exactly the two bundled
//! Klaus suites (functional + decimal) — the only CPU-only oracle ROMs
//! committed to the repo. The `SingleStepTests` cycle-exact audit
//! (`crates/rusty2600-cpu/tests/singlestep_test.rs`, 233/233 opcodes) is its
//! own separately-enforced gate in the `rusty2600-cpu` crate and is not yet
//! folded into this shared score — doing so would need it to report a
//! pass/fail summary this crate can consume across the crate boundary,
//! which is future work, not done here. As more Layer 2/3 oracles are
//! bundled (TIA-timing ROMs, a real golden-log diff), record them into the
//! SAME [`AccuracyScore`] rather than starting a second, competing one.
//!
//! Gated behind `test-roms` for the same reason `klaus_test.rs` is: the
//! decimal sweep alone is hundreds of millions of instructions.
#![cfg(feature = "test-roms")]

use rusty2600_cpu::{Cpu, CpuBus};
use rusty2600_test_harness::{AccuracyScore, RunOutcome, Sentinel, run_cpu_until_sentinel};
use std::fs;

struct FlatRamBus {
    ram: Vec<u8>,
}

impl FlatRamBus {
    // Heap-allocated (not a stack array) -- 64 KiB on the stack trips
    // clippy's `large_stack_arrays`.
    fn zeroed() -> Self {
        Self {
            ram: vec![0; 65536],
        }
    }
}

impl CpuBus for FlatRamBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.ram[usize::from(addr)]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.ram[usize::from(addr)] = val;
    }
}

#[test]
fn accuracy_battery_meets_v1_0_threshold() {
    const V1_0_THRESHOLD: f64 = 0.90;
    let mut score = AccuracyScore::default();

    // Klaus functional test — see klaus_test.rs for the full protocol
    // rationale; same ROM, same sentinel, run again here specifically to
    // feed the shared score (a modest CI-time duplication in exchange for
    // a genuine end-to-end pass-rate gate, not a re-derivation of the
    // protocol itself).
    {
        let rom = fs::read("../../tests/roms/test_suite/6502_functional_test.bin")
            .expect("Failed to load Klaus 6502 functional test binary.");
        let mut bus = FlatRamBus::zeroed();
        assert_eq!(rom.len(), 65536);
        bus.ram.copy_from_slice(&rom);
        let mut cpu = Cpu::power_on();
        cpu.reset(&mut bus);
        cpu.set_pc(0x0400);
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcTrap { success_pc: 0x3469 },
            200_000_000,
        );
        score.record(outcome == RunOutcome::Passed);
    }

    // Klaus decimal test.
    {
        let rom = fs::read("../../tests/roms/test_suite/6502_decimal_test.bin")
            .expect("Failed to load the assembled Klaus decimal test binary.");
        let mut bus = FlatRamBus::zeroed();
        bus.ram[0x0200..0x0200 + rom.len()].copy_from_slice(&rom);
        let mut cpu = Cpu::power_on();
        cpu.reset(&mut bus);
        cpu.set_pc(0x0200);
        let outcome = run_cpu_until_sentinel(
            &mut cpu,
            &mut bus,
            Sentinel::PcWithZeroPageCheck {
                success_pc: 0x024B,
                zp_addr: 0x000B,
                pass_value: 0,
            },
            700_000_000,
        );
        score.record(outcome == RunOutcome::Passed);
    }

    println!(
        "accuracy battery: {}/{} oracle ROMs passed ({:.1}%)",
        score.passed,
        score.total,
        score.pass_rate() * 100.0
    );

    assert!(
        score.pass_rate() >= V1_0_THRESHOLD,
        "accuracy battery pass rate {:.1}% is below the v1.0 threshold ({:.0}%) -- see docs/STATUS.md's Version policy",
        score.pass_rate() * 100.0,
        V1_0_THRESHOLD * 100.0
    );
}
