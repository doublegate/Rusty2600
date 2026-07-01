# tests/cpu-timing — Rusty2600

Cycle-exact 6502 opcode test vectors, consumed by
`crates/rusty2600-cpu/tests/singlestep_test.rs`.

`singlestep-6502/` is a trimmed subset (20 cases per opcode, out of ~10,000 in
the source) of the [SingleStepTests/65x02](https://github.com/SingleStepTests/65x02)
project (MIT licensed, Thomas Harte et al — see that repo for the full corpus
and license text), covering all 233 opcodes `rusty2600-cpu` implements
(documented + illegal, including the 12 JAM variants). Each `<opcode>.json`
gives, per test case, the CPU's initial register/memory state, the expected
final state, and the EXACT cycle-by-cycle bus sequence (address, value,
read/write) real 6502 silicon produces for that instruction — this is what
catches wrong cycle counts, not just wrong final values (the Klaus functional
test only checks the latter).

Unlike the ROMs in `tests/roms/`, this isn't a "ROM" (no copyright concerns,
MIT redistribution-friendly), just generated conformance data — hence its own
top-level directory rather than living under `tests/roms/`.

`fetch-vectors.py` (stdlib-only) regenerates or extends the trimmed subset by
downloading each opcode's full case list from the upstream repo and keeping the
first N (default 20); see its module docstring for the per-opcode and
whole-corpus invocations.
