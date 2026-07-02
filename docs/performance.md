# Performance — Rusty2600

References: `ref-docs/research-report.md` §5 (the beam-racing render model), §10
(engineering challenges); `docs/scheduler.md`. Profile (`cargo bench` + `perf
record`) before adding any abstraction — measure first.

## The hottest loop is the color-clock tick

`System::tick_one_color_clock()` runs at the **TIA color clock — 3.579545 MHz**,
i.e. **3× the CPU rate** (the CPU steps only every third call). It is the single
hottest path in the system: at 262 lines × 228 color clocks × ~60 fps that is
~3.58 million ticks per emulated second. Per ref-docs/research-report.md §1, §5.

Rules for the hot path:

- **Allocation-free.** No `Box`/dyn/`Vec` churn inside `tick_one_color_clock`,
  `Tia::tick_color_clock`, `Cpu::tick`, `Riot::tick`, or any `Board::cpu_read/
  write`. Prefer fixed arrays (`Objects.pos: [u8; 5]`, `Riot.ram: [u8; 128]`).
- **No framebuffer churn.** The TIA emits one `(luma, chroma)` dot per visible
  color clock into a per-scanline buffer; there is no full-frame allocation per
  pixel.
- **Branch-lean register decode.** The sparse/mirrored bus decode and the cart
  hotspot match run on every CPU access — keep them table/match-driven, not
  allocating.
- **The coprocessor hooks are default no-op** so the overwhelming majority of
  plain-ROM boards pay nothing for the DPC path.

## Targets

- Headless core: target **≤ 2 ms/frame** (the budget RustyNES holds; the 2600's
  workload is far lighter, so this should be comfortable once the renderer
  lands).
- The benches live per chip crate (`cargo bench -p rusty2600-cpu`,
  `-p rusty2600-tia`, etc.) so each chip is profilable in isolation (the
  one-directional crate graph, `docs/architecture.md`).

## Measured baselines (v0.5.0, populated Criterion benches)

Per-chip Criterion benches now exist (`crates/*/benches/*_bench.rs`, run via
`cargo bench -p <crate>`); numbers below are from a single unoptimized
development-machine run and are a baseline, not a performance guarantee —
re-measure before trusting them for a real regression comparison.

| Bench | What it measures | Measured |
|---|---|---|
| `rusty2600-cpu::cpu_step_mixed_addressing_modes` | one `Cpu::step()` against a flat-RAM bus, mixed addressing modes (immediate/zero-page/indexed/absolute/branch) | ~6.0 ns/call |
| `rusty2600-tia::tia_tick_color_clock` | one `Tia::tick_color_clock()` | ~4.8 ns/call |
| `rusty2600-tia::tia_full_ntsc_frame` | one full NTSC frame, 262×228 color clocks, TIA-only (no CPU/RIOT/cart) | ~899 µs/frame |
| `rusty2600-riot::riot_tick` | one `Riot::tick()` (interval-timer prescale) | ~433 ps/call |
| `rusty2600-riot::riot_ram_write_then_read` | one RIOT RAM write + read round-trip | ~418 ps/call |
| `rusty2600-cart::cart_bankf8_read_write` | one `BankF8` read + write (hotspot check on every access) | ~2.5 ns/call |
| `rusty2600-cart::cart_dpc_register_read` | one `BankDpc` data-fetcher display read (RNG clock + register decode) | ~3.3 ns/call |
| `rusty2600-core::system_full_ntsc_frame` | one full NTSC frame (262×228 color clocks) driven through the WHOLE `System` — real CPU decode/execute, TIA register writes, RIOT, and cart, via `System::step_instruction` in a loop, not a per-chip proxy | ~1.25 ms/frame |

The TIA-only full-frame figure (~899 µs) is already well under the ≤ 2 ms/frame
target with the full CPU+RIOT+cart overhead not yet included — comfortable
headroom, consistent with the "far lighter than RustyNES's workload"
expectation above. The end-to-end `system_full_ntsc_frame` bench above
(`crates/rusty2600-core/benches/frame_bench.rs`) confirms this holds with the
full stack included: ~1.25 ms/frame, still comfortably under the ≤ 2 ms
target.

That bench drives a small hand-assembled synthetic 4 KiB `Rom4K` cartridge
image (a real NTSC kernel: 3 lines VSYNC, 37 lines VBLANK, 192 visible lines,
30 lines overscan, looping forever via `WSYNC` beam-stalls) rather than a
committed game ROM — this project never commits commercial ROMs, and the only
2600-cartridge-shaped images actually committed under `tests/roms/test_suite/`
are two generic 6502 CPU conformance binaries, not a TIA-driving kernel. The
synthetic ROM exercises the same code paths a real game would (CPU decode,
TIA register writes, the scheduler's `WSYNC`/`RDY` stall), just without real
graphics content, which the frame-timing bench doesn't need.

## CI-gated performance-regression check

`scripts/bench_regression_check.sh` runs `system_full_ntsc_frame` and fails if
its measured mean exceeds a fixed **absolute** ceiling (currently
**3,750,000 ns / 3.75 ms**, roughly 3x the ~1.25 ms measured baseline above —
deliberately NOT a relative/percentage-based check, since CI runners have
enough run-to-run timing variance that a relative comparison would flap on
noise; an absolute ceiling with real headroom above the measured baseline
still catches any regression severe enough to matter). Wired as the `perf` job
in `.github/workflows/ci.yml`, `ubuntu-latest` only. Re-baseline the ceiling
(with a comment explaining why) only after a deliberate, reviewed change
legitimately moves the measured mean — never to silence a real regression.

## Determinism is not negotiable for performance

No system time, thread scheduling, or OS RNG may enter the core to shave cycles —
the determinism contract (ADR 0004) overrides micro-optimization. Rate control
and run-ahead live in the frontend, off the core's hot path.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
