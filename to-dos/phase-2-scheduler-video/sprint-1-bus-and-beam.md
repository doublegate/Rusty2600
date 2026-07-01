# Phase 2 · Sprint 1 — Bus decode + the beam renderer core

Goal: the bus decodes the sparse/mirrored VCS map, and the TIA emits a correct
dot per visible color clock for the playfield + players, producing a stable
frame for a simple test ROM. RESPx/HMOVE/collisions are Sprint 2. References:
`docs/scheduler.md`, `docs/tia.md`, `ref-docs/research-report.md` §5.

## Tickets

### T-0201-001 — Sparse / mirrored bus decode

- **Description:** implement `Bus::cpu_read/cpu_write`: TIA write regs ($00–$2C
  with mirrors), TIA read regs (collisions/inputs), RIOT RAM ($80–$FF), RIOT I/O +
  timer ($280–$297), the cart window ($1000–$1FFF via the board). Open-bus latch
  on unmapped/write-only reads. (Closes `T-0201-001`.)
- **Acceptance:** RIOT RAM round-trips through a mirror; a TIA register write lands
  in `Tia`; a cart read runs the board hotspot.
- **Dependencies:** Phase 1 complete. **Complexity:** L.

### T-0201-002 — Beam counters + HBLANK/visible + WSYNC release

- **Description:** `Tia::tick_color_clock` advances `color_clock` 0..228 and
  `scanline`, distinguishes the 68-clock HBLANK from the 160 visible, and releases
  RDY at HBLANK end. (Refines the scaffold; closes part of `T-0201-002`.)
- **Acceptance:** `wsync_sets_and_hblank_clears_rdy` green; a frame is exactly
  228 color clocks × the region line count.
- **Dependencies:** T-0201-001. **Complexity:** M.

### T-0201-003 — TIA write-register decode

- **Description:** decode the full write map into `Objects` + colours + strobes +
  the audio regs (audio synthesis is Phase 3, but the writes land now). (Closes
  `T-0201-003`.)
- **Acceptance:** writing PF0/PF1/PF2/GRPx/NUSIZx/COLUPx/COLUBK updates the
  modelled state; strobes act on write regardless of data.
- **Dependencies:** T-0201-001. **Complexity:** M.

### T-0201-004 — Playfield + player dot emission

- **Description:** emit one `(luma, chroma)` per visible color clock from the
  playfield (20-bit PF, CTRLPF reflect/score/priority) and the two players
  (GRPx + NUSIZ copies + REFPx mirror + VDELPx). (Closes part of `T-0201-004`.)
- **Acceptance:** a static-playfield test ROM produces a scanline buffer matching
  a committed golden (`SnapComparator::diff_pixels == 0`).
- **Dependencies:** T-0201-002, T-0201-003. **Complexity:** L.

### T-0201-005 — Scanline-buffer golden harness

- **Description:** capture the composed scanline buffer per frame and wire
  `SnapComparator` + a `tests/golden/` corpus + a bless flow. (Closes
  `T-0601-004`.)
- **Acceptance:** the golden harness compares a deterministic run (seeded) against
  a committed buffer and a re-run is byte-identical (ADR 0004).
- **Dependencies:** T-0201-004. **Complexity:** M.

### T-0201-006 — WSYNC mid-instruction freeze point

- **Description:** model the exact cycle on which the CPU freezes when RDY is
  asserted mid-instruction, without breaking Phase 1 per-cycle bus ordering.
  (Closes `T-0201-006`.)
- **Acceptance:** a WSYNC-timing test ROM lands the post-stall instruction on the
  correct scanline start.
- **Dependencies:** T-0201-002. **Complexity:** M.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
