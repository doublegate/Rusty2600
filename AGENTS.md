<!-- Managed by Master-Claude. Universal rules come from the imported/inlined core.
     Edit only inside the MC-PROJECT block; mc-sync overwrites everything else. -->
<!-- mc-core: 0.1.0 | mode=import | lang=rust -->
# AGENTS.md — Rusty2600

@/home/parobek/.claude/master-core/AGENTS.base.md
@/home/parobek/.claude/master-core/lang/rust.md
@/home/parobek/.claude/master-core/modules/10-commits-and-versioning.md
@/home/parobek/.claude/master-core/modules/20-testing-and-accuracy.md
@/home/parobek/.claude/master-core/modules/30-quality-gates.md
@/home/parobek/.claude/master-core/modules/40-docs-and-adrs.md
@/home/parobek/.claude/master-core/modules/50-architecture-patterns.md
@/home/parobek/.claude/master-core/modules/60-security.md
@/home/parobek/.claude/master-core/modules/70-release-ceremony.md
@/home/parobek/.claude/master-core/modules/80-phase-sprint-workflow.md
@/home/parobek/.claude/master-core/modules/90-multi-language-integration.md
@/home/parobek/.claude/master-core/modules/95-named-pattern-library.md

<<< MC-PROJECT-START >>>
## Project: Rusty2600

Rusty2600 is a cycle-accurate Atari 2600 / Video Computer System emulator in Rust at the Mesen2 / ares / higan bar.
Architecture (the load-bearing facts — read `docs/architecture.md`):

- **The timing master is the TIA color clock** @ 3579545 Hz; the lockstep scheduler advances it
  one color clock per tick, the 6507 runs on every **third** clock, and `WSYNC`/`RDY` freezes the
  CPU mid-cycle (the beam-stall) while the color clock keeps running.
- **The TIA owns BOTH video and audio.** The RIOT is the console's only RAM (128 B) + the I/O
  ports + the interval timer — it has no sound. The AudioBus draws samples FROM the TIA.
- **The Bus owns everything mutable** (`rusty2600-core::Bus`); the CPU borrows `&mut Bus`. The 2600
  has no separate work RAM, so the Bus carries no WRAM field (the only RAM is in the RIOT).
- **The crate graph is one-directional**; no chip crate depends on another except `rusty2600-tia`,
  which reads cart-mediated bus state via `rusty2600-cart`; `rusty2600-core` ties them.
- **Board logic lives in the cart crate** (default-no-op trait hooks); the tiered bankswitch
  catalog is gated by an accuracy-honesty marker (`Tier`; ADR 0003).
- **Determinism is a hard contract** (seed+ROM+input ⇒ bit-identical AV; frontend owns rate control; ADR 0004).
- **Test ROMs are the spec**; pin the failing ROM first, then implement.
- **Additive features are default-off** so shipped/native/no_std/wasm stay byte-identical.
- **Frontend is winit + wgpu + cpal + egui** (native render/audio thread + egui overlay); additive
  feature flags: `debug-hooks`, `hd-pack`, `retroachievements`, `emu-thread`, `help-tui`.

## Where things live

- `crates/rusty2600-cpu/` — MOS 6507 (cpu)
- `crates/rusty2600-tia/` — TIA (Television Interface Adaptor) — **video AND audio**
- `crates/rusty2600-riot/` — MOS 6532 RIOT — the only RAM (128 B) + I/O ports + interval timer
- `crates/rusty2600-cart/` — tiered bankswitch catalog (honesty gate; ADR 0003)
- `crates/rusty2600-core/` — Bus + scheduler · `crates/rusty2600-frontend/` — egui shell (binary `rusty2600`)
- `crates/rusty2600-cheevos/` — RetroAchievements integration (gated by `retroachievements` feature)
- `crates/rusty2600-test-harness/` — the accuracy oracle
- `docs/` — the spec (update in the same PR as code); `docs/STATUS.md` = single source of truth;
  `docs/adr/` — ADRs. `ref-docs/` — immutable research. `ref-proj/` — study clones (gitignored).
- `to-dos/ROADMAP.md` — planning entry point; tickets `T-PS-NNN`.

## Build / test / lint

```bash
cargo check --workspace && cargo test --workspace
cargo test --workspace --features test-roms
cargo clippy --workspace --all-targets -- -D warnings   # + per-feature jobs; NEVER --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build -p rusty2600-core --target thumbv7em-none-eabihf --no-default-features   # no_std gate
```

## Conventions

Chip change touches the chip code AND its `docs/<chip>.md`; `unsafe` only in frontend + FFI
with `// SAFETY:`; never commit commercial ROMs; never `--all-features`. Start clean at
v0.1.0 — RustyNES "v2.0 / engine-lineage" anchors are NOT this project's releases.

<<< MC-PROJECT-END >>>

