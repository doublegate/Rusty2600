# Supplemental: TIA audio's real clocking + waveform model (corrects §6, §14 item 2)

Frozen 2026-07-01. Corrects/extends `research-report.md` §6 (TIA audio deep-dive)
and §14 open question 2 (AUDC 0xA/0xB). Found while pinning AUDC 0xA/0xB
against the current Stella source per that open question — the finding turned
out to be much larger than the two contested modes.

## What was assumed

`crates/rusty2600-tia/src/audio.rs` implements the classic **TIASOUND**
model: a 16-entry lookup keyed directly on the 4-bit `AUDC` value, each entry
hand-describing one "distortion mode" (pure tone, 4/5/9-bit poly noise,
composites), clocked by a single free-running `prescale` counter that fires
every 114 color clocks (342 for AUDC 12-15). This is the model most
2600-emulator write-ups (including the *Stella Programmer's Guide*) describe,
and is what `docs/tia.md`'s existing AUDC table documents.

## What Stella's current source actually does

`ref-proj/stella/src/emucore/tia/AudioChannel.cxx` + `Audio.hxx` (the
Stella Team's own hardware-derived rewrite, not the legacy TIASOUND port)
uses a **fundamentally different architecture**:

1. **`AUDC`'s 4 bits are two independent 2-bit fields**, not one 4-bit mode
   selector. `myAudc & 0x03` selects the *noise*-feedback behavior;
   `myAudc >> 2` (the high 2 bits) selects the *pulse*-feedback behavior,
   evaluated independently each phase. There is no 16-entry mode table at
   all — the 16 "modes" musicians and docs describe are simply the 16
   combinations these two independent 2-bit fields can take, and their
   *interaction* (the noise counter feeds into the pulse counter's hold/
   feedback logic and vice versa) is what produces the audible waveform.
2. **Two internal counters, not one output bit**: `myPulseCounter` (a 4-bit
   counter, read out inverted-and-shifted: `~(myPulseCounter >> 1) & 0x07`)
   and `myNoiseCounter` (a 5-bit LFSR-like counter), each with their own
   feedback network keyed on the AUDC half-field above, `myNoiseCounterBit4`
   (bit 0 of the noise counter, latched), and a `myPulseCounterHold` gate.
3. **Two-phase clocking, not one `tick()`**: `phase0()` updates the feedback
   flags and the frequency-divider comparison against `AUDF`; `phase1()`
   actually shifts both counters and computes `actualVolume()`. A full
   "audio clock" is one `phase0()` + one `phase1()` pair.
4. **Fixed firing positions within the 228-color-clock scanline, not a
   free-running modulo counter**: `Audio::tick()` runs every color clock
   (sampling `actualVolume()` into a running sum every single clock — the
   *volume* output is sampled continuously) but only calls `phase0()` at
   color-clock positions **9 and 81**, and `phase1()` (+ `createSample()`,
   averaging the accumulated per-clock volume sum) at positions **37 and
   149**. That's exactly 2 phase0/phase1 pairs per 228-clock scanline —
   consistent with the already-correct "~31.4 kHz = color/114" rate this
   project's `docs/tia.md`/`docs/scheduler.md` already document — but the
   four firing positions are NOT evenly spaced (9, 37, 81, 149; gaps 28, 44,
   68, and 88 wrapping) and are NOT derived from a simple `clock % 114 == 0`
   test. They are specific real-hardware-derived positions tied to the
   TIA's actual internal clock-generation circuit, not an emulator
   convenience.
5. **Volume is sampled every color clock and averaged**, not read out once
   per phase — `mySumChannel0`/`mySumChannel1` accumulate `actualVolume()`
   every one of the 228 clocks, and `createSample()` (called only at the two
   `phase1()` positions) divides the accumulated sum by the elapsed-clock
   count to get the actual output sample. This is a form of oversampling/
   averaging this project's current single-sample-per-tick model does not do.

## What this means for AUDC 0xA/0xB specifically

The original open question ("0xA/0xB differ from the manual due to
clock/data alignment") is real, but it's a symptom of using the wrong model
entirely, not a two-mode exception to an otherwise-correct 16-mode table.
`0xA` (noise bits `10`, pulse bits `00`) and `0xB` (noise bits `11`, pulse
bits `00`) only make sense as specific noise/pulse-feedback-field
combinations in Stella's model — there is no clean way to "patch" the
existing TIASOUND-style lookup table to make just these two entries correct
without the surrounding two-counter feedback network they depend on.

## Recommendation

Treat full AUDC/audio-clocking accuracy as its own dedicated piece of work,
not a two-mode pin — re-architect `rusty2600-tia::audio` around Stella's
two-counter (`pulse`/`noise`) feedback-field model and its two-phase,
fixed-position-per-scanline clocking, verified in the same differential-
oracle style already used for TIA video/RIOT (build a headless Gopher2600 or
Stella audio-sample probe and diff sample-for-sample against a known ROM).
Logged as a ticket in `to-dos/phase-3-audio/` rather than folded into the
Phase 6 accuracy-battery sprint, since this is closer to "the audio model
needs rebuilding" than "verify an existing model against a reference."
