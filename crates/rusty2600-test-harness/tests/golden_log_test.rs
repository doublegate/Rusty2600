//! `T-0602-007`: closes `GoldenLogDiffer`'s honesty gap with a genuine
//! externally-oracled golden CPU trace.
//!
//! `tests/golden/klaus_functional_test_gopher2600.trace` is a real
//! per-instruction `(PC, A, X, Y, SP, P, cumulative cycle)` trace, captured
//! by running `tests/roms/test_suite/6502_functional_test.bin` (the same
//! CC0 Klaus functional test `klaus_test.rs` already exercises) through
//! Gopher2600's `hardware/cpu` package directly (not the full VCS —
//! `cpu.NewCPU(mem)` against a flat 64 KiB `Memory` implementation, exactly
//! the pattern Gopher2600's own `cpu_test.go` uses to unit-test the CPU in
//! isolation). The probe lived at
//! `ref-proj/Gopher2600/cmd_klaustrace/main.go` (that directory is
//! gitignored — a throwaway differential-oracle probe, not a permanent
//! fixture, per this project's established `rusty2600-gopher-differential-
//! oracle` workflow).
//!
//! Deliberately bounded to the first 20,000 retired instructions, not the
//! full multi-million-instruction run — a full trace would be an
//! impractically large committed fixture, and 20,000 instructions is
//! already a real, meaningful independent confirmation: two
//! *independently implemented* 6502 cores (Rusty2600's and Gopher2600's)
//! agreeing register-for-register and cycle-for-cycle over a real
//! self-testing program's early control flow is genuine external
//! validation, distinct from Klaus's own internal pass/fail trap (which
//! only proves the ROM's self-check passed, not that either emulator
//! matches an independent reference at the per-instruction level).
//!
//! Both sides start from an explicit, documented common baseline
//! (`A=X=Y=0, SP=0xFF, P=0x34, PC=0x0400`) rather than relying on each
//! emulator's own reset-vector-fetch ceremony to happen to consume the same
//! idle cycles — that's an implementation-specific detail neither side
//! claims to model identically, and it's not a real accuracy question the
//! Klaus test exercises (it only depends on a clean register state and
//! `PC=$0400`). `P=0x34` (not the more obvious `0x24`) specifically because
//! Gopher2600's own `Status.Load()` unconditionally forces its internal
//! `Break` bit true regardless of the loaded byte (a quirk in its source,
//! not a bug found here) — `0x34` is the value whose *effective* state
//! matches on both sides once that quirk is accounted for.
#![cfg(feature = "test-roms")]
#![allow(warnings)]
use rusty2600_cpu::{Cpu, CpuBus, Status};
use rusty2600_test_harness::{GoldenLogDiffer, TraceRecord};
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

/// Parses the golden trace's `pc,a,x,y,sp,p,cycle` CSV lines (hex fields,
/// decimal cycle count), skipping `#`-prefixed comment/header lines. No
/// `serde`/CSV crate dependency added for this — the format is small and
/// fixed enough that a hand-rolled split is simpler than a new dependency.
fn parse_golden_trace(text: &str) -> Vec<TraceRecord> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split(',');
            let pc = u16::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let a = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let x = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let y = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let sp = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let p = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let cycle = fields.next().unwrap().parse::<u64>().unwrap();
            TraceRecord {
                pc,
                a,
                x,
                y,
                sp,
                p,
                cycle,
            }
        })
        .collect()
}

#[test]
fn matches_gopher2600_golden_trace_over_first_20000_instructions() {
    let golden_text =
        fs::read_to_string("../../tests/golden/klaus_functional_test_gopher2600.trace")
            .expect("Failed to load the golden CPU trace fixture.");
    let golden = parse_golden_trace(&golden_text);
    assert_eq!(
        golden.len(),
        20_000,
        "the bundled golden trace's own length"
    );

    let mut bus = KlausBus { ram: [0; 65536] };
    let rom = fs::read("../../tests/roms/test_suite/6502_functional_test.bin").expect(
        "Failed to load Klaus 6502 test binary. Make sure to download it to tests/roms/test_suite/",
    );
    assert_eq!(rom.len(), 65536);
    bus.ram.copy_from_slice(&rom);

    // The explicit common baseline (see module doc) rather than
    // `Cpu::power_on()` + `reset()` — the reset-vector-fetch ceremony's
    // idle-cycle count is an implementation detail this cross-check
    // deliberately does not depend on.
    let mut cpu = Cpu::power_on();
    cpu.a = 0;
    cpu.x = 0;
    cpu.y = 0;
    cpu.s = 0xFF;
    cpu.p = Status::from_bits_truncate(0x34);
    cpu.cycles = 0;
    cpu.set_pc(0x0400);

    let mut differ = GoldenLogDiffer::new();
    differ.load_golden(golden);
    assert!(differ.bundled(), "a golden log was just loaded");

    for _ in 0..20_000 {
        let pc_at_fetch = cpu.pc;
        cpu.step(&mut bus);
        differ.capture(TraceRecord {
            pc: pc_at_fetch,
            a: cpu.a,
            x: cpu.x,
            y: cpu.y,
            sp: cpu.s,
            p: cpu.p.bits(),
            cycle: cpu.cycles,
        });
    }

    assert_eq!(
        differ.first_divergence(),
        None,
        "Rusty2600's CPU diverged from Gopher2600's independent implementation"
    );
}
