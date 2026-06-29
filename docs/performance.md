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

## Determinism is not negotiable for performance

No system time, thread scheduling, or OS RNG may enter the core to shave cycles —
the determinism contract (ADR 0004) overrides micro-optimization. Rate control
and run-ahead live in the frontend, off the core's hot path.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
