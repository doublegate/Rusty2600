# Rollback netplay (`rusty2600-netplay`) — Rusty2600

References: `docs/adr/0004-determinism-contract.md`; `docs/adr/0007-save-state-versioning.md`;
`to-dos/ROADMAP.md` (v1.10.0 "Rollback"); `crates/rusty2600-netplay/src/`.
This doc is the SPEC, not history — update it in the same PR as the code.

## What this crate is

`rusty2600-netplay` adds 2-player rollback netplay, built on the [ggrs](https://crates.io/crates/ggrs)
crate — a mature, independently-maintained GGPO-style rollback engine for
Rust — rather than a from-scratch reimplementation. Rollback's `resync()`
loop is fundamentally "restore a recent snapshot, replay forward" — the
exact primitive `rusty2600_core::SaveState` (`[1.1.0]`, ADR 0007) already
provides, and ADR 0004's determinism contract is explicitly cited as "the
basis for rewind, run-ahead, **and netplay rollback**." No new
determinism infrastructure was needed to build this crate — it's a thin,
real integration layer.

**v1.10.0 lands a real, tested rollback session crate**: 2-player-only,
direct-IP/LAN UDP transport, input-delay + max-prediction-window defaults
matching GGPO convention, and a genuine rollback-desync test proving the
save/restore/resimulate path is correct. **It is not yet wired into
`rusty2600-frontend`** — no host/join-game menu, no live per-frame input
capture feeding a running session. See "What's deferred" below.

## Why 2-player-only

Deliberate scope cut vs. RustyNES's own 2-4-player mesh: real 2600
hardware rarely supported more than 2 controllers, so a fully-connected
N-peer mesh, roster protocol, and 3-4-player UI would be pure
RustyNES-parity cost with little real payoff here — the single biggest
intentional scope divergence in this crate, decided by the project's own
plan before implementation began.

## Architecture (matches the crate)

- **`config.rs`** — `RustyConfig` (the `ggrs::Config` binding) and
  `PortInput` (one player's joystick contribution: four directions + fire).
  **Why not `rusty2600_core::MovieFrame` directly as `Config::Input`**:
  `MovieFrame` packs BOTH joystick ports' bits into one `swcha` byte plus
  shared console-switch state, because a `.r26m` movie records the WHOLE
  machine's input for one frame. GGRS's `Config::Input` is fundamentally
  per-player — each peer contributes, transmits, and confirms only its own
  input. Using `MovieFrame` as-is would mean each player's "input" secretly
  smuggled the other player's port bits too, a real bug waiting to happen.
  `PortInput` is the correct, minimal per-player type; `frame_advance::combine`
  recombines both players' confirmed inputs into a real `MovieFrame`
  immediately before advancing the frame.
- **`checksum.rs`** — a cheap 128-bit hash over save-state blobs, feeding
  GGRS's own built-in desync-detection.
- **`frame_advance.rs`** — `advance_one_frame` (a documented, deliberate
  duplication of `emu_thread::run_frame`'s core loop, since this crate
  can't depend on the frontend crate) and `combine` (recombines two
  `PortInput`s into one `MovieFrame`).
- **`session.rs`** — `RollbackSession`, wrapping `ggrs::P2PSession`, using
  GGRS's own built-in `UdpNonBlockingSocket` (no custom transport
  implementation needed — GGRS already ships a real, tested UDP
  transport). `DEFAULT_INPUT_DELAY = 2` and
  `DEFAULT_MAX_PREDICTION_WINDOW = 8` match GGPO convention exactly (GGRS's
  own docs cite 2-4 frames as typical input delay; 8 frames was GGPO's
  original default prediction window).
- **`lib.rs`** — crate docs plus the rollback-desync test (see below).

## What's deliberately out of scope this release

- **Console switches and paddles are not modeled per-player.** There's no
  natural "which peer owns this" mapping for shared machine-level switches
  (Select/Reset/Color/Difficulty) in a 2-player session, and paddle-based
  head-to-head titles are a small slice of the library. A session runs with
  console switches permanently idle and paddles centered until this is
  revisited — documented, not silently dropped.
- **STUN/hole-punch NAT traversal is deferred to `v1.10.x`.** This release
  is direct-IP/LAN connection only (the peer's `SocketAddr` is supplied
  directly, e.g. exchanged out-of-band by the two players). Implementing
  and — crucially — *verifying* real NAT traversal needs a genuine
  external peer no sandbox can provide, and is substantial, separable work
  from the rollback session logic itself.
- **WebRTC browser transport is deferred to `v1.10.x`** per the original
  plan — this crate is native-only.
- **Frontend wiring is deferred to `v1.10.x`.** No host/join-game menu, no
  live `ScriptBus`-style capture of per-frame input into a running
  session. This release lands a real, tested standalone crate; wiring it
  into `rusty2600-frontend` is the same honest-partial-landing pattern
  this project already used for `rusty2600-thumb` (`[1.6.0]`) and
  `rusty2600-script` (`[1.9.0]`).

## The rollback-desync test

The actual proof the rollback logic is correct, not just that the crate
compiles: uses `ggrs::SyncTestSession` (GGRS's own built-in
determinism-testing session type) driving a REAL `rusty2600_core::System`
loaded with a tiny synthetic 4K ROM (`LDA $0280` / `STA $80` / `JMP
$1000` — continuously copies `SWCHA` into RIOT RAM) across 12 frames of
varied two-player input. `SyncTestSession::advance_frame()` internally
saves state, advances, rewinds `check_distance` frames, re-simulates
forward, and panics on any checksum mismatch — reaching the end of the
loop without a panic IS the test's pass condition.

This test was validated for real, not just written and trusted: an
initial version using a bare `System::new()` with no cartridge attached
passed vacuously (no board means CPU state barely depends on input, and a
`None` checksum trivially equals `None`). Fixed by adding the
input-reactive synthetic ROM and wiring a real 128-bit checksum, then
re-confirmed by deliberately reintroducing the same bug and watching the
test correctly fail with a `MismatchedChecksum` panic.

A related real fix: `rusty2600_cart::Cartridge`'s enum is sized to its
largest variant (`BankF4`'s inline 32 KiB ROM array), so every
`System::clone()` inside `SaveState::capture` copies a several-tens-of-KB
struct — several such clones nested a few stack frames deep (inside
GGRS's own internal rollback recursion) overflowed the default ~2 MiB test
thread stack. The test now runs on an explicit 32 MiB-stack thread rather
than relying on an external `RUST_MIN_STACK` a CI runner might not set.

## What's next

Per `to-dos/ROADMAP.md`: a `v1.10.x` follow-up wires `RollbackSession`
into `rusty2600-frontend` (a real host/join-game menu, live per-frame
input capture), adds STUN/hole-punch NAT traversal for real internet
play, and adds the WebRTC browser transport (reusing the existing wasm
build). Only once frontend wiring lands does rollback netplay become
something a user can actually play.
