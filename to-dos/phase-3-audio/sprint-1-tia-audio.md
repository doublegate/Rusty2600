# Sprint 3.1 — TIA Audio Channels

**Context:** Part of Phase 3 — Audio.

## Tickets (`T-0301-NNN`)

- [x] `T-0301-001`: Implement the classic TIASOUND-style two-channel
  poly-counter model (`crates/rusty2600-tia/src/audio.rs`): per-channel
  AUDF+1 clock divider, 4/5/9-bit LFSR poly counters, the 16 AUDC mode
  match, color/114 vs CPU/114 (AUDC 12-15) source selection, linear 2-channel
  mix. Done pre-v0.2.0.
- [x] `T-0301-002`: Add tests for silent-by-default construction. Done
  pre-v0.2.0 (`audio::tests::silent_by_default`).

Superseded by `sprint-2-hardware-accurate-model.md`: investigating AUDC
0xA/0xB (v0.2.0) found this sprint's model is a confirmed simplification of
what Stella's current source actually implements — see
`ref-docs/2026-07-01-supplemental-audio-hardware-model.md`. This sprint's
scope (get *a* working two-channel synthesizer in place) is genuinely done;
getting it *bit-exact* is the new sprint.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
