# Phase 1 — CPU golden log

**Goal:** the 6507 core executes the full NMOS 6502 instruction set — documented
and undocumented, with working decimal mode and exact per-cycle bus access — to
**0-diff** against the Klaus Dormann and SingleStepTests/ProcessorTests golden
logs.

References: `docs/cpu.md`; `docs/testing-strategy.md`;
`ref-docs/research-report.md` §4 (the 6507), §11 (the test oracle);
`crates/rusty2600-cpu/src/lib.rs`.

## Scope

In: the instruction-decode sequencer; all addressing modes; the full documented
opcode set; the NMOS undocumented opcodes (stable + unstable); decimal-mode
`ADC`/`SBC`; per-cycle bus reads/writes including the dummy reads on page-cross
and RMW; the RESET sequence timing; the RDY/WSYNC stall latch the scheduler
asserts. The `GoldenLogDiffer` wired to the Klaus golden trace.

Out: anything TIA/RIOT/cart-specific beyond the flat-memory test bus (those are
Phase 2+). No interrupts (the 6507 has none).

## Exit criteria (verifiable)

- Klaus `6502_functional_test` runs to its success sentinel with **0-diff** on the
  `GoldenLogDiffer`.
- Klaus `6502_decimal_test` (Bruce Clark) passes (BCD `ADC`/`SBC` exact).
- The SingleStepTests/ProcessorTests `6502` set passes per-opcode, **per-cycle**
  (bus read/write activity matches), including undocumented opcodes. Use the
  `6502` set, not `nes6502`.
- `Cpu::tick` stays allocation-free (no per-instruction `Box`/dyn).

## Sprints

- Sprint 1 — documented core + addressing + decimal → `sprint-1-documented-core.md`
  (`T-0101-NNN`).
- Sprint 2 — undocumented opcodes + per-cycle bus parity (stub — add
  `sprint-2-undocumented-and-cycles.md` when Sprint 1 is ~complete).

## Risks

- Decimal-mode flag behaviour (`N`/`V`/`Z` after BCD) is subtle — pin against
  Bruce Clark's test, not prose.
- The unstable opcodes (`XAA`/`AHX`/`TAS`/`LAS`/`SHX`/`SHY`) have analog/magic-
  constant behaviour — match the ProcessorTests vectors' chosen constants.
- Per-cycle bus ordering (dummy reads/writes) must land exactly, since Phase 2's
  TIA timing depends on it (`docs/cpu.md` §Cycle accuracy).


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
