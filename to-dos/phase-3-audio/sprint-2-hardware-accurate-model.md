# Sprint 3.2 — Hardware-Accurate Audio Model

**Context:** Part of Phase 3 — Audio. Supersedes `sprint-1-tia-audio.md`'s
original scope for bit-exactness (that sprint's "get a working synthesizer
in place" goal is done; this one is "make it bit-exact").

Found while pinning AUDC `0xA`/`0xB` against Stella for v0.2.0 (research
report §14 open question 2) — see
`ref-docs/2026-07-01-supplemental-audio-hardware-model.md` for the full
writeup and `docs/tia.md` §Audio for the summary. The current
`crates/rusty2600-tia/src/audio.rs` implements the classic TIASOUND 16-mode
lookup table; Stella's current source (`ref-proj/stella/src/emucore/tia/
AudioChannel.cxx` + `Audio.hxx`) uses a materially different, hardware-
derived model. Getting `0xA`/`0xB` right requires adopting that model
wholesale, not patching two table entries.

## Tickets (`T-0302-NNN`)

- [ ] `T-0302-001`: Re-architect `Channel` around Stella's two-counter model:
  a 4-bit `pulse_counter` (read out as `~(pulse_counter >> 1) & 0x07`) and a
  5-bit `noise_counter`, replacing the current `poly4`/`poly5`/`output`
  fields. `AUDC`'s 4 bits split into two independent 2-bit fields (`audc &
  0x03` for noise-feedback behavior, `audc >> 2` for pulse-feedback
  behavior) — there is no 16-entry match on the full nibble in the target
  model.
- [ ] `T-0302-002`: Implement the two-phase clock (`phase0`/`phase1`
  equivalents) replacing the current single `tick()`: `phase0` updates the
  feedback flags (`noise_feedback`, `noise_counter_bit4`,
  `pulse_counter_hold`) and compares the frequency divider against `AUDF`;
  `phase1` shifts both counters using those flags.
- [ ] `T-0302-003`: Fire the two phases at Stella's fixed color-clock
  positions per scanline (phase0 at 9 and 81; phase1 at 37 and 149 —
  `createSample()`-equivalent runs alongside phase1), not a free-running
  `prescale >= 114` modulo counter. Confirm this is genuinely
  position-fixed and not itself a Stella implementation detail worth
  cross-checking against Gopher2600's audio model too before committing to
  it as ground truth.
- [ ] `T-0302-004`: Sample and average `actualVolume()`-equivalent every
  color clock (not just once per phase1), matching Stella's
  `mySumChannel0`/`mySumChannel1` running-sum-then-divide pattern.
- [ ] `T-0302-005`: Resolve the linear-vs-non-linear volume-mixing
  discrepancy flagged in `docs/tia.md` (this doc says non-linear/16-impedance-
  level; `audio.rs`'s own module doc claims a simple linear sum) — determine
  which is actually correct against Stella/hardware notes and fix whichever
  side is wrong.
- [ ] `T-0302-006`: Differential-oracle validation: build a headless
  Gopher2600 or Stella audio-sample probe (extending the existing
  `rusty2600-gopher-differential-oracle` workflow) and diff sample-for-sample
  against a known test ROM exercising all 16 AUDC values, both channels,
  and both source clocks (color/114 and CPU/114).

## Exit criteria

- All 16 AUDC values produce sample-identical output against the
  differential oracle, for both channels and both source-clock modes.
- `Audio::tick` (or its two-phase replacement) stays allocation-free — it
  runs on the hot color-clock path (ADR 0004 determinism + `docs/
  performance.md`).
- `docs/tia.md`'s Audio section and `ref-docs/2026-07-01-supplemental-audio-
  hardware-model.md`'s "what this means" section are reconciled with the
  final implementation.

## Risks

- This is a genuine rearchitecture of a subsystem with existing (if
  simplified) working behavior — must not regress `audio::tests::
  silent_by_default` or any perceptual expectations already validated
  informally.
- The four fixed clock positions (9/37/81/149) are Stella-specific; confirm
  they're real-hardware-derived (not a Stella-internal implementation
  artifact) before treating them as ground truth — cross-check against
  Gopher2600 and, if possible, real-hardware capture data cited in
  ref-docs/research-report.md §6.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
