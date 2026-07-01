# Phase 0 · Sprint 1 — Workspace, substrate, CI

Goal: the scaffold compiles and CI is green, with the lockstep loop, the
determinism seed, the Core boards, and the honesty gate in place. Most of this is
DONE in the scaffold; the tickets below pin the remaining gaps and the
acceptance bars. References: `docs/architecture.md`, `docs/scheduler.md`,
`docs/adr/0001/0003/0004`.

## Tickets

### T-0001-001 — Crate split compiles clean

- **Description:** the seven crates exist with the one-directional dependency
  graph (`docs/architecture.md`): cpu (no console dep), tia → cart, riot
  (independent), cart (independent), core ties them, frontend → core, harness →
  core.
- **Acceptance:** `cargo check --workspace` + `cargo clippy --workspace
  --all-targets -- -D warnings` + `cargo fmt --all --check` all green.
- **Dependencies:** none. **Complexity:** S (scaffolded).

### T-0001-002 — `no_std` cross-compile gate for the chip stack

- **Description:** cpu/tia/riot/cart build against `core` + `alloc` only.
- **Acceptance:** `cargo build -p rusty2600-cpu --target thumbv7em-none-eabihf
  --no-default-features` succeeds, and likewise for `-tia`, `-riot`, `-cart`; the
  command is wired into CI.
- **Dependencies:** T-0001-001. **Complexity:** S.

### T-0001-003 — Lockstep scheduler + seeded determinism phase

- **Description:** `System::tick_one_color_clock` advances the TIA master, steps
  the CPU every third color clock offset by the seeded `phase`, ticks the RIOT +
  cart on the CPU cycle, and respects `Tia::rdy_stall` (the WSYNC freeze).
- **Acceptance:** `seeded_phase_is_deterministic` and `color_clock_advances`
  green; `wsync_sets_and_hblank_clears_rdy` (in tia) green.
- **Dependencies:** T-0001-001. **Complexity:** M (loop scaffolded; the exact
  mid-instruction freeze point is `T-0201-006`, deferred to Phase 2/6).

### T-0001-004 — `Bus` decode skeleton (no WRAM field)

- **Description:** `Bus` owns tia/riot/board/open_bus and exposes `cpu_read/
  cpu_write` masking to 13 bits. The real sparse/mirrored decode is `T-0201-001`
  (Phase 2); this ticket only fixes the shape — crucially, **no `wram` field**
  (the 2600's RAM is in the RIOT).
- **Acceptance:** `Bus::new()` compiles; `audio_sample()` routes to
  `tia.audio.sample()`; no WRAM field present.
- **Dependencies:** T-0001-001. **Complexity:** S.

### T-0001-005 — Core cart boards + `detect()` + honesty gate

- **Description:** `Rom2K`, `Rom4K`, `BankF8` implement `Board`; `detect()` picks
  them by length; `Tier::is_accuracy_gated` is the load-bearing predicate.
- **Acceptance:** `rom4k_reads_window`, `rom2k_mirrors`, `f8_switches_on_hotspot`,
  `detect_picks_sized_boards`, `besteffort_is_never_accuracy_gated`, and both
  tests in `mapper_tier_honesty.rs` green.
- **Dependencies:** T-0001-001. **Complexity:** S (scaffolded).

### T-0001-006 — Harness shapes present

- **Description:** `GoldenLogDiffer` (capture/`first_divergence`),
  `run_until_complete`, `AccuracyScore` (pass-rate), `SnapComparator` (scanline-
  buffer diff) exist as the API the later phases fill.
- **Acceptance:** `run_until_complete_times_out_on_stub`, `score_pass_rate`,
  `snap_identical_is_zero` green.
- **Dependencies:** T-0001-001. **Complexity:** S (scaffolded).

### T-0001-007 — STATUS + CI report v0.1.0

- **Description:** `docs/STATUS.md` shows v0.1.0, all suite counts 0 / TBD, and
  the board tier matrix; CI workflow runs check/test/clippy/fmt/no_std + the
  honesty gate.
- **Acceptance:** CI green on a clean checkout; STATUS matches the scaffold state.
- **Dependencies:** T-0001-001..006. **Complexity:** S.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
