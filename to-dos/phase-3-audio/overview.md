# Phase 3 — Audio

**Goal:** the TIA two-channel polynomial-counter audio synthesizes the 16 AUDC
distortion modes bit-exactly (against TIASOUND/Stella), with the correct
dual-clock derivation (color/114 and CPU/114) and the non-linear volume mixer.

References: `docs/tia.md` §Audio; `ref-docs/research-report.md` §6, §10.6, §14
(items 1–2); `crates/rusty2600-tia/src/audio.rs`.

## Scope

In: the per-channel clock divider (gated by AUDF+1); the 4/5/9-bit polynomial
shift registers; the 16 AUDC distortion mappings; the color/114 vs CPU/114 source
selection (AUDC 12–15 use CPU/114); the non-linear 16-level volume + 2-channel
mix; `AudioBus::audio_sample` feeding the frontend resampler.

Out: the frontend audio sink / DRC / device picker (Phase 5); any EQ/effects
(Phase 8).

## Exit criteria (verifiable)

- Each of the 16 AUDC modes matches a TIASOUND/Stella reference at the bit/sample
  level (the 0xA/0xB alignment quirk pinned against Stella, not prose — research
  report §14 item 2).
- The audio clock is the precise 3.579545 MHz / 114 (and CPU/114 for 12–15), not
  the rounded "30 kHz."
- A deterministic run produces bit-identical audio samples (ADR 0004).
- `Audio::tick` stays allocation-free (it runs on the hot color-clock path).

## Sprints

- Sprint 1 — poly counters + AUDC modes + mixer (stub — add
  `sprint-1-poly-and-mixer.md` with `T-0301-NNN` when Phase 2 is ~complete).

## Risks

- The exact poly-counter tap config + the 0xA/0xB alignment behaviour are debated
  in prose — the reference is TIASOUND/Stella, verified against captured samples.
- The dual source clock (color/114 vs CPU/114) must switch cleanly when AUDC
  crosses 11→12 mid-note.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
