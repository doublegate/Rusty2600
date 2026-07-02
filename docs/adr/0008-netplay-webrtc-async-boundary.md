# ADR 0008 — Netplay WebRTC: async-runtime containment boundary

## Status

Accepted.

## Context

Rusty2600 is 100% synchronous today, everywhere — the core (`no_std +
alloc`, no async at all), the frontend (`std`-only, no async runtime), and
`rusty2600-netplay` itself: `v2.3.0`'s STUN client deliberately chose
`stun_codec` (a genuinely sans-IO RFC 5389 codec) specifically to keep it
that way, and `rusty2600-netplay`'s module doc explicitly named WebRTC as
"out of scope" at the time for exactly this reason — a naive WebRTC
integration would pull an async runtime (`tokio`, or `wasm-bindgen-futures`
used pervasively) into a crate, and by extension a session-driving hot
path, that has never needed one.

`v2.6.0` closes this gap. Before writing any WebRTC code, this ADR answers
the question `v2.3.0` deferred: can the async surface WebRTC genuinely
needs stay contained to one-time connection setup, or does it leak into the
per-frame session loop (and from there, transitively, toward the
deterministic core)?

Two facts, both verified directly against this codebase and the browser
WebRTC API shape, make the answer here concrete rather than speculative:

1. **`rusty2600-netplay::session::RollbackSession` wraps a concrete
   `ggrs::P2PSession<RustyConfig>`, and `RustyConfig::Address =
   std::net::SocketAddr` is a crate-wide fixed associated type** (not a
   generic parameter `RollbackSession` is parameterized over). Any new
   `ggrs::NonBlockingSocket` implementation — including a WebRTC-backed one
   — must therefore present itself as `NonBlockingSocket<SocketAddr>` to
   slot into the existing `P2PSession<RustyConfig>` unchanged. This project
   is also already deliberately 2-player-only (`session.rs`'s own module
   doc: "real 2600 hardware rarely supported more than 2 controllers" — the
   single biggest intentional scope divergence from RustyNES's N-peer mesh),
   so a WebRTC socket only ever has exactly one peer. A **fixed sentinel
   `SocketAddr`** (never a real IP — WebRTC data channels have no IP
   addressing in the browser sense at all) standing in for "the one WebRTC
   peer" satisfies GGRS's type requirement with zero changes to
   `RustyConfig`, `RollbackSession`, or any other existing netplay code.
2. **A `web_sys::RtcDataChannel`'s hot-path operations are synchronous, not
   async**, once the channel is open: `RtcDataChannel::send_with_u8_array`
   is a plain synchronous call (not a `Promise`), and inbound messages
   arrive via an `onmessage` `Closure` callback that fills a queue —
   `poll()` then synchronously drains that queue. This is the exact shape
   RustyNES's own `WebRtcTransport` already uses (`crates/rustynes-netplay/
   src/webrtc.rs`, read directly as reference): its `Transport::send`/
   `Transport::poll` contain no `await` anywhere. Only the ONE-TIME
   connection-establishment phase (`RTCPeerConnection.createOffer()`/
   `createAnswer()`/`setLocalDescription()`, ICE candidate gathering) is
   inherently promise-driven in the browser's own API — there is no way to
   make that part synchronous, but it also never needs to be, since it
   happens once, before the session starts stepping frames, not on the hot
   path.

There is already a proven, working precedent in this exact codebase for
"one-time async setup, callback-driven event loop, fully synchronous hot
path": `v2.5.0`'s `Gfx::new_async` wasm bring-up, which drives
`wasm_bindgen_futures::spawn_local` to populate a shared
`Rc<RefCell<Option<Active>>>` cell once the (inherently async) GPU
adapter/device request completes, after which every per-frame
`ApplicationHandler` callback runs fully synchronously and treats a
not-yet-ready cell as a no-op. WebRTC's connection setup is architecturally
the same shape: an async one-time bring-up populating a cell/handle the
synchronous per-frame loop then drives directly.

## Decision

**The async surface stays fully contained to WebRTC connection setup
(offer/answer/ICE), using the exact `wasm_bindgen_futures::spawn_local` +
shared-cell pattern `v2.5.0`'s `Gfx::new_async` already established in this
codebase.** It never touches `RollbackSession::advance_frame`/`poll`, never
becomes a dependency of `rusty2600-core` or `rusty2600-cart` (the
`no_std`+`alloc` chip stack), and never becomes a transitive dependency of
the default native build.

Concretely:

- A new `WebRtcSocket` (native-naming-mirrored, `wasm32`-only) implements
  `ggrs::NonBlockingSocket<SocketAddr>` over a `web_sys::RtcDataChannel`,
  constructed from an **already-open** data channel — matching this
  project's existing `PunchedUdpSocket` precedent (`session.rs`), which
  likewise wraps an already-prepared native building block rather than
  owning its own setup. `send_to`/`receive_all_messages` are synchronous,
  following RustyNES's `WebRtcTransport::send`/`poll` shape directly.
- `RollbackSession::with_webrtc_socket(channel: RtcDataChannel, ...)`
  (mirroring the existing `with_socket` constructor for the STUN/hole-punch
  path) is the ONLY new public entry point `rusty2600-netplay` needs — no
  changes to `RustyConfig`, `advance_frame`, or any other session-driving
  code.
- The connection-establishment dance (create `RtcPeerConnection`, create
  the data channel, generate/apply SDP offer-answer, gather ICE
  candidates, wait for the channel's `readyState` to reach `"open"`) is a
  wasm32-only, `rusty2600-frontend`-side concern (this project's existing
  convention: transport-adjacent glue lives in the frontend, not the
  netplay crate itself, matching how `netplay_session.rs` already owns the
  UI-facing Connect dialog). It runs inside a `spawn_local` task exactly
  like `Gfx::new_async`, populating a cell the frontend's synchronous
  per-frame netplay-dispatch code then picks up once ready.
- `rusty2600-netplay`'s `Cargo.toml` gains `web-sys`/`wasm-bindgen`/
  `wasm-bindgen-futures` as **wasm32-target-only** dependencies (the
  existing `[target.'cfg(not(target_arch = "wasm32"))'...]` pattern this
  workspace already uses elsewhere, inverted for this crate's currently
  fully-native dependency list) — never native, never default-build.

A **native peer-to-peer WebRTC path** (outside any browser, e.g. via
`str0m`) stays an explicitly deferred stretch item within `v2.6.0`, per the
plan — browser-to-browser is the primary target since it is also the
**more testable** one (see the "Consequences" section below), and adding a
second, native-only WebRTC stack (with its own async-runtime containment
question to re-answer) is real, avoidable scope growth for a release that
doesn't need it to close the actual gap (`v2.3.0` already covers native
direct-IP/LAN and STUN-assisted UDP; the gap this release closes is
specifically "browser peers can't reach each other at all").

## Consequences

- Rusty2600's "100% synchronous, no async runtime" invariant holds for the
  native build and the deterministic core, unconditionally — `tokio`/any
  async runtime crate never appears in `rusty2600-core`'s or
  `rusty2600-cart`'s dependency tree, and the native default build's
  dependency graph is completely unaffected by this work (all new
  dependencies are wasm32-target-gated).
- Browser-to-browser WebRTC can be verified end-to-end with two browser
  tabs on the SAME machine — ICE candidate gathering, the DTLS handshake,
  and data-channel open all complete correctly on localhost, unlike raw
  NAT hole-punching (`v2.3.0`'s STUN work) which fundamentally needs two
  different networks to test the traversal itself. This gives `v2.6.0` a
  **stronger, same-host-verifiable bar** than `v2.3.0`'s STUN work could
  reach — a real, live, same-host round trip is achievable in this
  sandbox, not just a compile-verified skeleton.
- A minimal, manual/copy-paste SDP offer-answer exchange (no signaling
  server) is sufficient to prove the transport works — matching the
  existing "exchange your STUN-discovered address out-of-band" convention
  `v2.3.0` already established for the plain-UDP path. A turn-key hosted
  signaling server (RustyNES's `deploy/` bundle: Docker + Caddy + coturn)
  is explicitly out of scope — real infrastructure-deployment cost this
  project doesn't need to take on to close the actual capability gap.
- `WebRtcSocket`'s sentinel `SocketAddr` is an internal implementation
  detail, never exposed as a real address anywhere in the UI/API surface —
  a reviewer or future maintainer must not mistake it for a real peer
  address if they encounter it while debugging.


---
*Note: Test ROMs (Klaus2m5, ProcessorTests) and Stella oracles have been seeded in `tests/roms` and `tests/golden`. Commercial ROMs are staged in `tests/roms/external` for mapper validation.*
