# CLAUDE.md

Guidance for Claude Code working in Rusty2600.

## What this is

Rusty2600 is a cycle-accurate Atari 2600 / Video Computer System emulator in Rust at the Mesen2 / ares / higan bar.
Architecture (the load-bearing facts — read `docs/architecture.md`):

- **The timing master is the TIA color clock** @ 3579545 Hz; the lockstep scheduler advances it
  one color clock per tick, the 6507 runs on every **third** clock, and `WSYNC`/`RDY` freezes the
  CPU mid-cycle (the beam-stall) while the color clock keeps running.
- **The TIA owns BOTH video and audio.** The RIOT is the console's only RAM (128 B) + the I/O
  ports + the interval timer — it has no sound. The AudioBus draws samples FROM the TIA.
- **The Bus owns everything mutable** (`rusty2600-core::Bus`); the CPU borrows `&mut Bus`. The 2600
  has no separate work RAM, so the Bus carries no WRAM field (the only RAM is in the RIOT).
- **The crate graph is one-directional**; no chip crate depends on another; `rusty2600-core` ties them.
- **Board logic lives in the cart crate** (default-no-op trait hooks); the tiered bankswitch
  catalog is gated by an accuracy-honesty marker (`Tier`; ADR 0003).
- **Determinism is a hard contract** (seed+ROM+input ⇒ bit-identical AV; frontend owns rate control).
- **Test ROMs are the spec**; pin the failing ROM first, then implement.
- **Additive features are default-off** so shipped/native/no_std/wasm stay byte-identical.

## Where things live

- `crates/rusty2600-cpu/` — MOS 6507 (cpu)
- `crates/rusty2600-tia/` — TIA (Television Interface Adaptor) — **video AND audio**
- `crates/rusty2600-riot/` — MOS 6532 RIOT — the only RAM (128 B) + I/O ports + interval timer
- `crates/rusty2600-cart/` — tiered bankswitch catalog (honesty gate; ADR 0003)
- `crates/rusty2600-core/` — Bus + scheduler · `crates/rusty2600-frontend/` — egui shell (binary `rusty2600`)
- `crates/rusty2600-test-harness/` — the accuracy oracle
- `docs/` — the spec (update in the same PR as code); `docs/STATUS.md` = single source of truth;
  `docs/adr/` — ADRs. `ref-docs/` — immutable research. `ref-proj/` — study clones (gitignored).
- `to-dos/ROADMAP.md` — planning entry point; tickets `T-PS-NNN`.

## Build / test / lint

```bash
cargo check --workspace && cargo test --workspace
cargo test --workspace --features test-roms
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # + per-feature jobs; NEVER --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features   # no_std gate
```

## Conventions

Conventional Commits; chip change touches the chip code AND its `docs/<chip>.md`; user-visible
changes go in `CHANGELOG [Unreleased]`; hot paths allocation-free; `unsafe` only in frontend +
FFI with `// SAFETY:`; never commit commercial ROMs; never `--all-features`. Start clean at
v0.1.0 — RustyNES "v2.0 / engine-lineage" anchors are NOT this project's releases.
