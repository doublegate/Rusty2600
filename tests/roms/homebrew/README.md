# tests/roms/homebrew — Rusty2600

~110 free/homebrew Atari 2600 game ROMs. **These are not test ROMs** — a
homebrew game booting to a plausible-looking frame demonstrates nothing about
cycle accuracy (no golden log, no exact expected output, no pass/fail
criterion). Their only purpose in this repository is generating gameplay
screenshots for `screenshots/homebrew/` (a human-readable compatibility/
showcase reference, mirroring how `../RustyNES/screenshots/` documents that
sibling project's commercial-game corpus).

For actual correctness testing, see `../test_suite/` (the Klaus CPU test
binaries) and `tests/cpu-timing/` (the SingleStepTests corpus) instead.

## Provenance

Originally added wholesale to `tests/roms/test_suite/`, this collection
turned out to mix legitimate homebrew/freeware with 3 actual commercial
dumps (`Pac-Man 4K`, `Crazy Balloon`, `Lady Bug`) — see the
`rusty2600-commercial-rom-scrub-2026-07-01` project memory for the full
incident and how it was remediated (those 3 were scrubbed from git history
entirely; they are not present here). The remaining files were individually
checked against known official 2600 releases before being kept: several
(e.g. `Mappy`, `Scramble`, `Super Cobra Arcade`, `Juno First`) correspond to
arcade titles that were **never officially licensed for the 2600 at all**, so
any 2600 version is necessarily homebrew; others (`Donkey Kong VCS`,
`Wizard of Wor Arcade`) are 32 KiB modern homebrew recreations, distinctly
larger than the real 1980s official cartridges (4 KiB) they share a name
with; `Juno First` carries an explicit embedded string confirming this
directly: `"Copyright (C) 2008 Chris Walton... Distributed exclusively by
AtariAge"`.

Before adding anything new to this directory, verify provenance the same
way: check whether an official 2600 release of that title ever existed, and
`strings -n 4 <file> | grep -iE "copyright|atari|activision|coleco|imagic"`
to look for embedded attribution.

## Regenerating screenshots

`crates/rusty2600-frontend/examples/dump_frame.rs` runs a ROM headlessly for
N frames and dumps a PPM frame; convert to PNG and sort into
`screenshots/homebrew/`:

```bash
cargo run --release --example dump_frame -p rusty2600-frontend -- \
    "tests/roms/homebrew/<Game>.a26" 600 /tmp/rusty2600-shots
magick /tmp/rusty2600-shots/frame_0599.ppm "screenshots/homebrew/<Game>.png"
```

See `screenshots/README.md` for the full layout and regeneration convention.
