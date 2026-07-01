# `screenshots/` — homebrew gameplay showcase

Committed PNG snapshots of the free/homebrew Atari 2600 games in
`tests/roms/homebrew/`, used as a human-readable compatibility/showcase
reference — mirroring how `../RustyNES/screenshots/` documents that sibling
project's commercial-game corpus (see its `README.md` for the pattern this
one follows).

**These are not accuracy regression baselines.** Rusty2600 has no commercial-
ROM corpus to snapshot the way RustyNES does (`tests/roms/homebrew/` is
homebrew/freeware specifically because commercial ROMs can't be committed —
see `docs/STATUS.md`), and a homebrew game rendering a plausible-looking
frame proves nothing about cycle-exact correctness. Machine-readable
correctness comes from `crates/rusty2600-test-harness`'s Klaus/SingleStepTests
suites and, eventually, the accuracy battery (`docs/STATUS.md`, v0.7.0) — this
directory exists purely so a reader can see the emulator actually running
real games.

## Layout

```text
screenshots/
├── README.md         (this file)
└── homebrew/          one PNG per tests/roms/homebrew/*.a26 title
```

## Regenerating

`crates/rusty2600-frontend/examples/dump_frame.rs` runs a ROM headlessly for
N frames and dumps a PPM frame; convert to PNG with ImageMagick:

```bash
cargo build --release --example dump_frame -p rusty2600-frontend
for rom in tests/roms/homebrew/*.a26; do
    base=$(basename "$rom" .a26)
    rm -f /tmp/rusty2600-shots/frame_*.ppm
    target/release/examples/dump_frame "$rom" 300 /tmp/rusty2600-shots 299
    last=$(ls /tmp/rusty2600-shots/frame_*.ppm | sort | tail -1)
    magick "$last" "screenshots/homebrew/$base.png"
done
```

`300` frames (`= 299` as the dump-from index, i.e. dump only the last frame)
is a reasonable default — most 2600 titles reach their attract/title screen
well before then. A handful of titles may need a different frame count to
land on a representative frame instead of a blank/loading one; adjust
per-title if a screenshot looks wrong.

## Known non-rendering / unrepresentative frames (expected)

Titles using cart boards Rusty2600 doesn't implement yet (BestEffort-tier or
entirely unimplemented bankswitch schemes — `docs/cart.md`) will not produce
a meaningful frame. As cart-scheme coverage grows (`to-dos/ROADMAP.md`,
v0.3.0 onward), regenerate and note any title that still doesn't render
correctly.

Current corpus: **109/110 rendered** (2026-07-01, v0.3.0 in progress).

- **`Zippy the Porcupine (NTSC)`** — 64 KiB ROM; needs one of the 64 KiB
  BestEffort schemes (`F0`, `EF`/`EFSC`, or `X07` — `docs/cart.md`), none of
  which are implemented yet. `detect()` correctly returns `None` for it
  (the honesty gate: no scheme claimed, none delivered) rather than
  guessing — `dump_frame` panics on the `Unsupported` `Err`, which is the
  expected/correct failure mode until one of those schemes lands.
