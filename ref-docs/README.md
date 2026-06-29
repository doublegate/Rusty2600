# ref-docs — Rusty2600

The IMMUTABLE research corpus for the Atari 2600 / Video Computer System. After
the research step this directory is frozen: new findings go in NEW dated files
(`YYYY-MM-DD-supplemental-<name>.md`), never edits to the existing ones. The
docs under `docs/` are the living spec; this is the frozen source material they
were derived from.

## Contents

- `research-report.md` — the deep research report (frozen 2026-06-24). Its
  sections:
  1. Executive summary
  2. Scope and goals
  3. Background
  4. Technical deep-dive: the 6507 CPU
  5. Technical deep-dive: the TIA video (the beam-racing model)
  6. Technical deep-dive: the TIA audio
  7. Technical deep-dive: the RIOT (MOS 6532)
  8. Technical deep-dive: cartridges and bankswitching (with tiering)
  9. State-of-the-art and prior art
  10. Principal engineering challenges
  11. Standards / compliance: the test-ROM oracle
  12. Architecture options (not pre-committed)
  13. External dependencies (candidate)
  14. Open questions and contested claims
  15. Source manifest (cited references)

## Adding findings later

Do not edit `research-report.md`. New hardware/dev-wiki extracts, corrections,
or contested-claim resolutions go in a dated supplemental file in this directory,
and the relevant `docs/<subsystem>.md` is updated to cite it.
