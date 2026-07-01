# `screenshots/` — gameplay showcase

Committed PNG snapshots of Atari 2600 games Rusty2600 runs — mirroring how
`../RustyNES/screenshots/` documents that sibling project's game corpus (see
its `README.md` for the pattern this one follows).

**These are not accuracy regression baselines.** A game rendering a
plausible-looking frame proves nothing about cycle-exact correctness.
Machine-readable correctness comes from `crates/rusty2600-test-harness`'s
Klaus/SingleStepTests suites and, eventually, the accuracy battery
(`docs/STATUS.md`, v0.7.0) — this directory exists purely so a reader can see
the emulator actually running real games.

## Layout

```text
screenshots/
├── README.md         (this file)
├── homebrew/          one PNG per tests/roms/homebrew/*.a26 title (ROMs committed)
└── commercial/        one PNG per tests/roms/external/commercial/*.a26 title
                        (ROMs gitignored — only the PNGs are committed)
```

The `commercial/` PNGs are safe to commit even though their source ROMs
aren't: a single rendered frame carries no copyrighted game code, same
reasoning RustyNES uses for its own commercial-game screenshot corpus.

## Regenerating

`crates/rusty2600-frontend/examples/dump_frame.rs` runs a ROM headlessly for
N frames and dumps a PPM frame; convert to PNG with ImageMagick:

```bash
cargo build --release --example dump_frame -p rusty2600-frontend
for rom in tests/roms/homebrew/*.a26 tests/roms/external/commercial/*.a26; do
    base=$(basename "$rom" .a26)
    subdir=$(echo "$rom" | grep -q homebrew && echo homebrew || echo commercial)
    rm -f /tmp/rusty2600-shots/frame_*.ppm
    target/release/examples/dump_frame "$rom" 300 /tmp/rusty2600-shots 299
    last=$(ls /tmp/rusty2600-shots/frame_*.ppm | sort | tail -1)
    magick "$last" "screenshots/$subdir/$base.png"
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

### homebrew/

Current corpus: **109/110 rendered** (2026-07-01, v0.3.0 in progress).

- **`Zippy the Porcupine (NTSC)`** — 64 KiB ROM; needs one of the 64 KiB
  BestEffort schemes (`F0`, `EF`/`EFSC`, or `X07` — `docs/cart.md`), none of
  which are implemented yet. `detect()` correctly returns `None` for it
  (the honesty gate: no scheme claimed, none delivered) rather than
  guessing — `dump_frame` panics on the `Unsupported` `Err`, which is the
  expected/correct failure mode until one of those schemes lands.

### commercial/

Current corpus: **15/16 rendered** (2026-07-01, v0.3.0 in progress). See
`tests/roms/README.md` for the local ROM staging convention.

- **`BurgerTime (USA)`** — regenerated after `T-0401-009`'s ROM-DB
  disambiguation landed. Previously misdetected as plain F6 (16 KiB
  defaulted to F6 before E7 detection existed), rendering an all-black
  frame; the hotspot-pattern heuristic (`is_probably_e7`) now correctly
  identifies it as E7 (confirmed against Stella's own properties database,
  which lists "M Network" as the manufacturer), and the screenshot shows
  real gameplay. A concrete example of why same-size misdetection matters,
  not just a theoretical concern.
- **`Pitfall II - Lost Caverns (USA)`** — now boots via the DPC (Display
  Processor Chip) coprocessor scheme (`T-0401-005`, v0.3.0), but its
  screenshot is a blank blue frame at every frame count tried (60/300/900/
  5000): a Gopher2600 differential probe confirmed DPC decode / control-
  flow are bit-exact for the first ~2,000 executed instructions, but the
  CPU then never returns from a boot-time RIOT-timer wait loop. Since
  `dump_frame`'s `run_frame` has a 200,000-instruction safety timeout that
  fires every call while stuck there, higher frame counts just burn more
  instructions in the same loop (5000 frames ≈ 1 billion instructions) —
  not a "needs more frames" case. Tracked as `T-0601-008`; not a DPC decode
  bug (control-flow is independently verified correct), left as-is pending
  that investigation rather than blocking this ticket.
- **`Communist Mutants from Space (USA)`** — 8,448-byte image, not one of
  the size buckets `detect()` currently resolves; needs BestEffort-tier
  breadth work (v0.4.x) once its bankswitch scheme is identified. Expected
  `Unsupported` failure until then.
