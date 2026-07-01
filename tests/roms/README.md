# tests/roms — Rusty2600

Committed CC0 / public-domain test ROMs only. **`tests/roms/external/` is gitignored** — stage
your commercial dumps there locally to boot/verify/screenshot every board; commit ONLY the
screenshots + `.snap` golden baselines they produce, NEVER the ROMs (copyright).

## Layout

- **`test_suite/`** — genuine CPU-behavior test binaries only:
  `6502_functional_test.bin` / `6502_decimal_test.bin` (Klaus Dormann / Bruce
  Clark, public domain; wired into `crates/rusty2600-test-harness/tests/klaus_test.rs`).
  A `65C02_extended_opcodes_test.bin` that shipped alongside these was removed
  2026-07-01 — the 2600's 6507 is an NMOS 6502 derivative, not a 65C02, so it
  was never applicable and nothing used it. Researched (2026-07-01) whether
  any other freely-redistributable 2600-specific test ROMs exist beyond what's
  here + `tests/cpu-timing/` (Tom Harte SingleStepTests): none found — no
  TIA/RIOT-specific test-ROM corpus is publicly redistributable (Gopher2600's
  own README says the same: it has never obtained permission to redistribute
  its TIA/RIOT test ROMs either), and the Atari Diagnostic Test Cartridge 2.0
  is official service-center software, NOT freely redistributable — do not
  add it. Further TIA/RIOT accuracy work continues via the differential-
  oracle method (`rusty2600-gopher-differential-oracle` project memory)
  against specific known-hard titles, not a canned test suite.
- **`homebrew/`** — ~110 free/homebrew Atari 2600 game ROMs (moved out of
  `test_suite/` 2026-07-01 — they were never test ROMs, and their presence
  there was misleading). See `homebrew/README.md`: these exist **only** to
  generate gameplay screenshots (`screenshots/homebrew/`), never for
  correctness testing — a game booting to a plausible-looking frame proves
  nothing about cycle accuracy.
- **`klaus/`**, **`Klaus2m5/`** — historical/duplicate staging locations for
  the same Klaus test binaries; `test_suite/` is the one actually read by
  the test harness.
- **`external/`** (gitignored) — local commercial-ROM staging, as above.

Commercial ROMs were found and scrubbed from `test_suite/`'s history once
already — see the `rusty2600-commercial-rom-scrub-2026-07-01` project memory
before adding anything new here.
