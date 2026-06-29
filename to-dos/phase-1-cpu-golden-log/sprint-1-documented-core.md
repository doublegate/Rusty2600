# Phase 1 · Sprint 1 — Documented core + addressing + decimal

Goal: the 6507 executes every documented NMOS opcode across all addressing modes
with correct flags and cycle counts, passing the Klaus functional + decimal
tests. References: `docs/cpu.md`, `docs/testing-strategy.md`,
`ref-docs/research-report.md` §4, §11.

## Tickets

### T-11-001 — Instruction-decode sequencer

- **Description:** replace the `Cpu::tick` stub with fetch/decode/execute; address
  masking to 13 bits before every bus access; the `cycles` counter advances per
  real CPU cycle. (Closes `T-PS-001`.)
- **Acceptance:** a hand-written program (LDA/STA/branch) executes correctly on
  the flat-memory test bus with the right cycle count.
- **Dependencies:** T-01-003. **Complexity:** L.

### T-11-002 — All addressing modes + dummy reads

- **Description:** implement immediate/zp/zp,X/abs/abs,X/abs,Y/(ind,X)/(ind),Y/
  relative/accumulator, including the page-cross dummy read and the RMW dummy
  read-then-write, on the exact cycles.
- **Acceptance:** the addressing-mode subtests of the Klaus functional test pass.
- **Dependencies:** T-11-001. **Complexity:** L.

### T-11-003 — Flags + stack + RESET timing

- **Description:** `P` updates (`C Z I D B U V N`), `PHP`/`PLP` push/pull
  round-trip (bit 5 reads 1, `B` only in the pushed copy), the stack page mapped
  into RIOT RAM, and the 7-cycle RESET sequence. (Refines `Cpu::reset`.)
- **Acceptance:** stack + flag subtests of the Klaus functional test pass;
  `reset_loads_vector` still green.
- **Dependencies:** T-11-001. **Complexity:** M.

### T-11-004 — Decimal mode (BCD ADC/SBC)

- **Description:** the `DECIMAL` flag gates BCD arithmetic with exact `N`/`V`/`Z`
  behaviour. (The 2600's 6507 has working decimal, unlike the NES 2A03.)
- **Acceptance:** Klaus `6502_decimal_test` (Bruce Clark) passes 0-diff.
- **Dependencies:** T-11-001. **Complexity:** M.

### T-11-005 — Klaus functional 0-diff via the GoldenLogDiffer

- **Description:** bundle the Klaus functional-test golden trace, wire
  `GoldenLogDiffer::first_divergence` to diff the live `(PC,A,X,Y,SP,P,cycle)`
  trace, and run the ROM to its success sentinel. (Closes `T-PS-070/071`.)
- **Acceptance:** `first_divergence()` returns `None` for the full functional-test
  run; the harness reports `Passed`.
- **Dependencies:** T-11-002, T-11-003, T-11-004. **Complexity:** M.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
