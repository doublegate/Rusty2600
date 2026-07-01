//! Cycle-exact opcode audit against the Tom Harte / `SingleStepTests` 65x02
//! corpus (<https://github.com/SingleStepTests/65x02>, `6502/v1/<opcode>.json>`,
//! see `tests/cpu-timing/README.md`).
//!
//! Unlike the Klaus functional test (which only checks final register/memory
//! state), this validates the EXACT cycle-by-cycle bus sequence — address,
//! value, and read/write — for every implemented opcode, INCLUDING the
//! "unstable" illegal ones (ANE/XAA, LXA, ARR, JAM). This is the gap that let
//! several real bugs ship undetected: a systematic missing dummy-read cycle
//! on every zero-page-indexed / indexed-indirect addressing mode, wrong N/V
//! flags on decimal-mode `ADC`, wrong ANE/LXA "magic constant," a missing
//! decimal-mode correction on `ARR`, and JAM's bus-lockup pattern not being
//! modeled — all found and fixed via this exact test. Every implemented
//! opcode passes 100% now, including the illegal ones (their "unstable" real-
//! silicon reputation just means the floating-bus constant varies by chip
//! batch — the `SingleStepTests` corpus captures ONE specific, internally
//! consistent reference behavior, which is what we match).
//!
//! Reads the committed, trimmed subset of the corpus (20 cases/opcode — the
//! full corpus is ~10,000 cases/opcode) from `tests/cpu-timing/singlestep-6502/`
//! at the workspace root. Override with the `SINGLESTEP_VECTORS_DIR` env var
//! to point at a larger local copy of the full corpus for a deeper sweep.

use rusty2600_cpu::{Cpu, CpuBus, Status};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: CpuState,
    #[serde(rename = "final")]
    end: CpuState,
    cycles: Vec<(u16, u8, String)>,
}

#[derive(Deserialize)]
struct CpuState {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

struct RecordingBus {
    mem: HashMap<u16, u8>,
    trace: Vec<(u16, u8, bool)>, // (addr, val, is_write)
}

impl CpuBus for RecordingBus {
    fn read(&mut self, addr: u16) -> u8 {
        let v = self.mem.get(&addr).copied().unwrap_or(0);
        self.trace.push((addr, v, false));
        v
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mem.insert(addr, val);
        self.trace.push((addr, val, true));
    }
}

/// Returns `(mismatches, total)` for one opcode file.
fn run_opcode_file(path: &std::path::Path) -> (Vec<String>, usize) {
    let text = fs::read_to_string(path).expect("read vector file");
    let cases: Vec<TestCase> = serde_json::from_str(&text).expect("parse vector file");
    let mut failures = Vec::new();

    for case in &cases {
        let mut bus = RecordingBus {
            mem: case.initial.ram.iter().copied().collect(),
            trace: Vec::new(),
        };
        let mut cpu = Cpu::new();
        cpu.pc = case.initial.pc;
        cpu.s = case.initial.s;
        cpu.a = case.initial.a;
        cpu.x = case.initial.x;
        cpu.y = case.initial.y;
        cpu.p = Status::from_bits_truncate(case.initial.p);

        cpu.step(&mut bus);

        let mut mismatch = Vec::new();
        if cpu.pc != case.end.pc {
            mismatch.push(format!("pc: got {:04X} want {:04X}", cpu.pc, case.end.pc));
        }
        if cpu.s != case.end.s {
            mismatch.push(format!("s: got {:02X} want {:02X}", cpu.s, case.end.s));
        }
        if cpu.a != case.end.a {
            mismatch.push(format!("a: got {:02X} want {:02X}", cpu.a, case.end.a));
        }
        if cpu.x != case.end.x {
            mismatch.push(format!("x: got {:02X} want {:02X}", cpu.x, case.end.x));
        }
        if cpu.y != case.end.y {
            mismatch.push(format!("y: got {:02X} want {:02X}", cpu.y, case.end.y));
        }
        if cpu.p.bits() != case.end.p {
            mismatch.push(format!(
                "p: got {:08b} want {:08b}",
                cpu.p.bits(),
                case.end.p
            ));
        }
        for &(addr, want) in &case.end.ram {
            let got = bus.mem.get(&addr).copied().unwrap_or(0);
            if got != want {
                mismatch.push(format!("ram[{addr:04X}]: got {got:02X} want {want:02X}"));
            }
        }
        if bus.trace.len() == case.cycles.len() {
            for (i, (got, want)) in bus.trace.iter().zip(case.cycles.iter()).enumerate() {
                let want_write = want.2 == "write";
                if got.0 != want.0 || got.1 != want.1 || got.2 != want_write {
                    mismatch.push(format!(
                        "cycle {i}: got ({:04X},{:02X},{}) want ({:04X},{:02X},{})",
                        got.0,
                        got.1,
                        if got.2 { "write" } else { "read" },
                        want.0,
                        want.1,
                        want.2
                    ));
                }
            }
        } else {
            mismatch.push(format!(
                "cycle count: got {} want {}",
                bus.trace.len(),
                case.cycles.len()
            ));
        }

        if !mismatch.is_empty() {
            failures.push(format!("{}: {}", case.name, mismatch.join("; ")));
        }
    }

    (failures, cases.len())
}

#[test]
fn singlestep_cycle_exact_audit() {
    let dir = std::env::var("SINGLESTEP_VECTORS_DIR").map_or_else(
        |_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/cpu-timing/singlestep-6502")
        },
        std::path::PathBuf::from,
    );
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read vectors dir {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();

    let mut total_cases = 0;
    let mut total_failures = 0;
    let mut failing_opcodes = Vec::new();

    for path in &entries {
        let opcode = path.file_stem().unwrap().to_string_lossy().to_uppercase();
        let (failures, count) = run_opcode_file(path);
        total_cases += count;
        if !failures.is_empty() {
            total_failures += failures.len();
            println!("=== opcode {opcode}: {}/{count} FAILED ===", failures.len());
            for f in failures.iter().take(2) {
                println!("    {f}");
            }
            failing_opcodes.push(opcode);
        }
    }

    println!(
        "\nsinglestep audit: {total_failures} failing cases / {total_cases} total across {} opcodes",
        entries.len()
    );
    println!("failing opcodes: {failing_opcodes:?}");

    assert!(
        failing_opcodes.is_empty(),
        "{} opcode(s) failed the cycle-exact audit: {failing_opcodes:?}",
        failing_opcodes.len()
    );
}
