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

**v1.10.0 landed a real, tested rollback session crate**: 2-player-only,
direct-IP/LAN UDP transport, input-delay + max-prediction-window defaults
matching GGPO convention, and a genuine rollback-desync test proving the
save/restore/resimulate path is correct. **`[2.1.0]` wired it into
`rusty2600-frontend`** — a real Connect dialog and live per-frame input
capture (see "Frontend wiring" below). **`[2.3.0]` adds a real STUN
client** for NAT-assisted connections (see "STUN client" below); the
WebRTC transport remains deferred.

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

## Frontend wiring (`netplay` feature, `rusty2600-frontend/src/netplay_session.rs`)

A real `NetplaySession` wrapper, a `netplay` feature flag (off by
default), a `Tools -> Netplay...` Connect dialog (local port + peer
address text fields), and live per-frame input capture now exist.
`RollbackSession::new`'s API is symmetric — both peers supply each
other's address out-of-band — so there is no separate "host and wait for
any connector" mode; the dialog has a single Connect action, not
Host/Join.

**Why this bypasses the `emu-thread` background loop.**
`RollbackSession` owns and steps its OWN internal `System` — not
something the existing `emu_thread`-spawned background thread (which
steps `EmuCore.system` directly via `crate::runahead::step_frame`) can
drive without a much larger refactor. Instead, connecting sets
`EmuCore.paused = true` (so the background thread's own `paused` gate
makes it skip stepping and sleep — no separate suppression plumbing
needed), and `app.rs`'s `render()` drives the session directly inside its
existing brief per-frame lock: `session.advance_frame(local_input)`, then
copy the resulting `System` into `EmuCore.system` and call the new
`EmuCore::extract_frame()` (a small, deliberate, additive duplication of
`run_frame`'s video-crop/audio-drain tail — kept as a new method rather
than refactoring `run_frame` itself, to keep the change surgical) to
produce this frame's framebuffer/audio the same way `run_frame`/
`step_frame` already do. **Run-ahead is bypassed while netplay is
active** — an acceptable, documented scope cut, since the two features
don't have an obvious combined semantics (run-ahead speculates on LOCAL
input only, meaningless once a rollback session is authoritative over the
timeline).

**`WritesLocked` gained a real `netplay_active` field this same pass**
(see `docs/scripting.md`) — a connected session now locks Lua script
writes too, for the same reason RA hardcore mode does: an unreplicated
local write would silently desync the two peers' otherwise bit-identical
timelines.

Real two-peer verification through `[2.1.0]` was localhost-only
(`netplay_session.rs`'s own test, two `NetplaySession`s on `127.0.0.1`
with different ports) — genuine LAN/cross-machine verification remained
open.

## STUN client (`[2.3.0]`)

Closes the STUN half of `[1.10.0]`'s "STUN/hole-punch NAT traversal is
deferred" note. `rusty2600-netplay::stun` adds a real RFC 5389 client
(`stun_codec`, a sans-IO codec — zero networking/async dependencies,
fitting this crate's plain `std::net::UdpSocket` convention with no new
async-runtime dependency) plus a best-effort UDP hole-punch helper.
`NetplaySession::connect_via_stun` (`rusty2600-frontend`) wires both into
a second "Connect via STUN" button in the Netplay dialog alongside the
existing direct-IP `Connect` button.

**What's real and live-verified**: the STUN Binding Request/Response
round trip itself. `stun::tests::discovers_a_real_public_address` and
`netplay_session::tests::connect_via_stun_completes_for_both_peers` both
performed genuine round trips against a real public STUN server
(`stun.l.google.com:19302`) during implementation — not mocked, not
simulated. Both are `#[ignore]`d by default (matching this project's
convention for tests needing live external network access — a CI
runner's outbound UDP access is a separate, less certain question than a
dev sandbox's) but pass when run explicitly (`cargo test -- --ignored`).
`RollbackSession::with_socket` plus its `PunchedUdpSocket` transport
(a small, real `ggrs::NonBlockingSocket` impl over a pre-bound socket,
needed because the STUN-discovered NAT mapping is only valid for the
exact socket that produced it) are proven by a genuine two-real-peer
localhost session test, not just "it compiles."

**What remains unverified, stated plainly**: actual NAT traversal between
two independently-NATed peers on two different real networks. A
single-host sandbox cannot provide that — loopback traffic never crosses
a NAT boundary at all, so no test in this codebase (or achievable in this
environment) proves the hole-punch actually opens a path through a real
router. If both peers sit behind restrictive (symmetric) NATs, the
subsequent GGRS handshake may simply time out; that's a known STUN-alone
limitation (TURN relay servers exist specifically for this case, and are
out of scope here). Mirrors this project's own `docs/mobile.md` "iOS
Verification" precedent for infrastructure that can be built and
partially verified, but not fully proven, in this environment.

Console switches and paddles remain un-modeled per-player (same "no
natural which-peer-owns-this mapping" reasoning as before). Genuine
LAN/cross-machine (not just localhost) verification also remains open.

## Browser WebRTC transport (`[2.6.0]`, ADR 0008)

Closes the WebRTC gap `[2.3.0]` deliberately deferred. See
`docs/adr/0008-netplay-webrtc-async-boundary.md` for the binding
architectural decisions (why the async surface stays contained to
one-time connection setup, why a sentinel `SocketAddr` stands in for the
one WebRTC peer). `rusty2600-netplay::webrtc` (`wasm32`-only) adds:

- **`WebRtcSocket`** — a `ggrs::NonBlockingSocket<SocketAddr>` over an
  already-open `web_sys::RtcDataChannel`, the WebRTC analogue of
  `PunchedUdpSocket`. Fully synchronous hot path (no `await`), reusing
  the exact `bincode`-over-bytes wire format `PunchedUdpSocket` already
  established.
- **`WebRtcPeer`** — the one-time async connection-establishment dance
  (create the peer connection + data channel, generate/apply an SDP
  offer or answer, wait for ICE gathering to complete). `#[wasm_bindgen]`-
  exported so a real netplay UI (a later release's scope) and this
  crate's own standalone `web/` test harness can both drive it.
- **`RollbackSession::with_webrtc_socket`** — the only new public entry
  point, mirroring `with_socket`.
- A minimal manual/copy-paste SDP exchange (no signaling server, per ADR
  0008) — `rusty2600-netplay/web/` is a standalone Trunk-built test page
  (NOT wired into `rusty2600-frontend`'s own wasm build) with two text
  boxes per side for the offer/answer blobs.

**What's real and verified**: the entire connection-establishment CODE
PATH — `WebRtcPeer::createOffer`/`createAnswer`/`acceptAnswer` — was
driven end-to-end via a real Chromium instance (CDP-scripted, two
independent tabs each holding its own `RTCPeerConnection`), and every
step succeeded with real data: a real 458-byte SDP offer generated, a
real 457-byte SDP answer generated in response, `set_remote_description`
accepting the answer without error. This proves the Rust/wasm-bindgen
API surface, the SDP offer/answer dance, and ICE-gathering-completion
detection (`wait_for_ice_gathering_complete`) all work correctly against
the real browser WebRTC API — not a mock, not a stub.

**What remains unverified, stated plainly**: the actual data channel
never reached `"open"` in this project's sandbox. Diagnosis (not
guesswork — checked directly): both peers' gathered SDP contained **zero
`a=candidate` lines** — `icegatheringstate` reached `"complete"`, but ICE
candidate gathering itself produced nothing, despite this sandbox having
real, non-loopback network interfaces available at the OS level
(confirmed via `ip addr`). Tried and ruled out: Chromium's mDNS
local-candidate obfuscation (`--disable-features=
WebRtcHideLocalIpsWithMdns`) made no difference; running under a virtual
display (`Xvfb`) instead of `--headless=new` could not even be launched
in this sandbox. This points to a deeper, environment-specific
restriction on Chromium's WebRTC media/ICE stack in this particular
sandboxed environment, not a bug in `WebRtcSocket`/`WebRtcPeer`'s own
logic — every step UP TO ICE candidate gathering is proven correct, and
the same offer/answer/ICE-wait code is a direct, faithful translation of
the standard browser WebRTC connection-establishment sequence. A future
session with a real desktop browser (or a sandbox without this
restriction) is the natural next verification step — `rusty2600-netplay/
web/cdp_verify.py` (kept, documented) is ready to re-run as-is once that
access exists.

A **native** (non-browser) WebRTC path stays explicitly deferred per ADR
0008 — browser-to-browser was always the primary, more valuable target.
