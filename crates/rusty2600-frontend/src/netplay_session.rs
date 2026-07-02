//! Rollback netplay frontend wiring (`netplay` feature, off by default).
//!
//! Wires `rusty2600-netplay`'s `RollbackSession` into the frontend per
//! `docs/netplay.md`'s "What's next": a real Connect dialog (see
//! `shell.rs`'s `render_netplay_dialog`) and live per-frame input capture
//! driving a running session. `[1.10.0]` shipped direct-IP/LAN only;
//! `[2.3.0]` adds [`NetplaySession::connect_via_stun`] — a real,
//! live-verified STUN client (`rusty2600_netplay::discover_public_address`)
//! plus a best-effort UDP hole-punch, letting two peers behind NAT discover
//! and exchange public addresses instead of requiring a shared LAN. Real
//! cross-NAT traversal between two independently-NATed peers on different
//! networks remains unverified in any sandbox this project has access to
//! (see `rusty2600-netplay::stun`'s module doc for the exact verification
//! boundary) — the STUN round trip itself IS live-tested. The WebRTC
//! browser transport remains explicitly deferred to its own future,
//! separately-scoped release (it would pull an async runtime into an
//! otherwise 100%-synchronous codebase).
//!
//! ## Why this bypasses the `emu-thread` background loop
//!
//! `RollbackSession` owns and steps its OWN internal `System` (see
//! `rusty2600-netplay::session`'s module doc) — it is not something the
//! existing `emu_thread`-spawned background thread (which calls
//! `crate::runahead::step_frame` on `EmuCore.system` directly) can drive
//! without a much larger refactor. Instead, while a [`NetplaySession`] is
//! active, `EmuCore.paused` is set `true` (so the background emu-thread's
//! own `paused` gate makes it skip stepping and just sleep — no separate
//! plumbing needed to suppress it), and `app.rs`'s `render()` drives the
//! session directly inside its existing brief per-frame lock, writing the
//! session's resulting `System` into `EmuCore.system` and calling
//! [`crate::emu_thread::EmuCore::extract_frame`] to produce this frame's
//! framebuffer/audio the same way `run_frame`/`step_frame` already do. This
//! keeps the change additive (no edits to the emu-thread closure itself)
//! at the cost of bypassing run-ahead while netplay is active — an
//! acceptable, documented scope cut: the two features already don't have
//! an obvious combined semantics (run-ahead speculates on LOCAL input only,
//! which is meaningless once a rollback session is authoritative over the
//! timeline).

use std::net::{SocketAddr, UdpSocket};

use rusty2600_core::System;
use rusty2600_netplay::{
    DEFAULT_INPUT_DELAY, DEFAULT_MAX_PREDICTION_WINDOW, DEFAULT_STUN_SERVER, NetplayError,
    PortInput, RollbackSession, StunError, discover_public_address, hole_punch,
};

use crate::input::InputState;

/// Everything that can go wrong in the STUN-assisted connect path.
///
/// [`NetplaySession::connect_via_stun`]'s error surface is a superset of
/// [`NetplayError`], since this path fails in real network-discovery
/// ways `connect`'s direct-IP path never has to.
#[derive(Debug)]
pub enum StunConnectError {
    /// Binding the local socket, or the STUN round trip itself, failed.
    Stun(StunError),
    /// The STUN handshake succeeded, but starting the rollback session over
    /// the resulting socket failed.
    Netplay(NetplayError),
}

impl core::fmt::Display for StunConnectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Stun(e) => write!(f, "STUN discovery failed: {e}"),
            Self::Netplay(e) => write!(f, "starting the rollback session failed: {e}"),
        }
    }
}

/// A live rollback netplay session, wrapping [`RollbackSession`].
pub struct NetplaySession {
    session: RollbackSession,
}

impl NetplaySession {
    /// Connect to `remote_addr` from `local_port`, using `system` (the
    /// currently-loaded game, already reset to power-on) as the session's
    /// starting state. `rom_tag` matches the tag this frontend's other
    /// `SaveState` consumers already use (currently `0`, an existing
    /// project-wide placeholder — see `emu_thread.rs`'s own
    /// `SaveState::capture(&self.system, 0)` calls).
    ///
    /// Direct-IP/LAN only — `remote_addr` must already be reachable from
    /// this machine (e.g. a LAN address, or a manually port-forwarded
    /// public one). See [`Self::connect_via_stun`] for the NAT-assisted
    /// path.
    ///
    /// # Errors
    ///
    /// See [`RollbackSession::new`].
    pub fn connect(
        local_port: u16,
        remote_addr: SocketAddr,
        system: System,
        rom_tag: u64,
    ) -> Result<Self, NetplayError> {
        let session = RollbackSession::new(local_port, remote_addr, system, rom_tag)?;
        Ok(Self { session })
    }

    /// Discover this machine's own public (STUN-mapped) address, send a
    /// best-effort hole-punch toward `remote_public_addr` (the OTHER
    /// peer's own already-discovered public address, exchanged out-of-band
    /// the same way [`Self::connect`]'s LAN address is), and start a
    /// rollback session over that same punched socket.
    ///
    /// Returns the locally-discovered public address alongside the session
    /// so the caller can show it to the user (e.g. "tell your friend to
    /// connect to `203.0.113.7:54321`").
    ///
    /// **What this does and doesn't prove**: the STUN round trip is real
    /// and live-verified (`rusty2600_netplay::stun`'s own tests actually
    /// hit a public STUN server). Whether the hole-punch actually opens a
    /// path through BOTH peers' real NATs is not something this method (or
    /// any test in this codebase) can verify without two machines on two
    /// different real networks — if the peers are both behind restrictive
    /// (symmetric) NATs, the subsequent GGRS handshake may simply time out,
    /// the same honest limitation `docs/netplay.md` already documents.
    ///
    /// # Errors
    ///
    /// Returns [`StunConnectError::Stun`] if the local socket can't be
    /// bound or the STUN server doesn't respond, or
    /// [`StunConnectError::Netplay`] if GGRS rejects the resulting session.
    pub fn connect_via_stun(
        local_port: u16,
        remote_public_addr: SocketAddr,
        system: System,
        rom_tag: u64,
    ) -> Result<(Self, SocketAddr), StunConnectError> {
        let socket = UdpSocket::bind(("0.0.0.0", local_port))
            .map_err(StunError::Io)
            .map_err(StunConnectError::Stun)?;

        let stun_server: SocketAddr = std::net::ToSocketAddrs::to_socket_addrs(DEFAULT_STUN_SERVER)
            .ok()
            .and_then(|mut addrs| addrs.next())
            .ok_or(StunConnectError::Stun(StunError::UnexpectedResponse))?;
        let public_addr =
            discover_public_address(&socket, stun_server).map_err(StunConnectError::Stun)?;

        hole_punch(&socket, remote_public_addr);

        let session = RollbackSession::with_socket(
            socket,
            remote_public_addr,
            system,
            rom_tag,
            DEFAULT_INPUT_DELAY,
            DEFAULT_MAX_PREDICTION_WINDOW,
        )
        .map_err(StunConnectError::Netplay)?;

        Ok((Self { session }, public_addr))
    }

    /// Whether the session has finished synchronizing with the remote peer.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.session.is_running()
    }

    /// Poll the socket for incoming packets (call every tick while waiting
    /// for the initial sync — `advance_frame` also polls internally once
    /// running).
    pub fn poll(&mut self) {
        self.session.poll();
    }

    /// Advance the session by one frame using this machine's local
    /// joystick input (always port 0's [`crate::input::Joystick`] — a
    /// 2-player-only session has exactly one local player).
    ///
    /// Returns `Ok(true)` if a real game frame was simulated (the caller
    /// should then copy [`Self::system`] into `EmuCore.system` and call
    /// `EmuCore::extract_frame`), `Ok(false)` if the session is still
    /// waiting on the remote peer this tick (not an error).
    ///
    /// # Errors
    ///
    /// See [`RollbackSession::advance_frame`].
    pub fn advance_frame(&mut self, host_input: &InputState) -> Result<bool, NetplayError> {
        let joy = host_input.joysticks[0];
        let local_input = PortInput {
            up: joy.up,
            down: joy.down,
            left: joy.left,
            right: joy.right,
            fire: joy.fire,
        };
        self.session.advance_frame(local_input)
    }

    /// The session's own, internally-driven [`System`] — read-only; every
    /// mutation must go through [`Self::advance_frame`] so rollback stays
    /// correct.
    #[must_use]
    pub const fn system(&self) -> &System {
        self.session.system()
    }
}

#[cfg(test)]
mod tests {
    use std::net::ToSocketAddrs;

    use super::*;

    /// Two local sessions on localhost, both driving the same synthetic
    /// input-reactive ROM `rusty2600-netplay`'s own rollback-desync test
    /// already validated — this proves the FRONTEND wiring (port binding,
    /// `PortInput` extraction from `InputState`, `advance_frame` dispatch)
    /// correctly drives a real session; it deliberately does not re-prove
    /// the rollback/resimulate math itself (that's `rusty2600-netplay`'s
    /// own job, already covered there).
    fn synthetic_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x1000];
        rom[0x000] = 0xAD; // LDA $0280
        rom[0x001] = 0x80;
        rom[0x002] = 0x02;
        rom[0x003] = 0x85; // STA $80
        rom[0x004] = 0x80;
        rom[0x005] = 0x4C; // JMP $1000
        rom[0x006] = 0x00;
        rom[0x007] = 0x10;
        rom[0xFFC] = 0x00;
        rom[0xFFD] = 0x10;
        rom
    }

    fn loaded_system() -> System {
        let board = rusty2600_cart::detect(&synthetic_rom()).expect("synthetic ROM must detect");
        let mut system = System::new(0);
        system.bus.board = Some(board);
        system.reset();
        system
    }

    /// `connect_via_stun` end to end: both "peers" run on this same
    /// machine, each doing a REAL STUN round trip (`discover_public_address`,
    /// live-tested in `rusty2600-netplay::stun` too) before hole-punching
    /// and starting a session. Since both processes sit behind the same
    /// NAT/router in this environment, whether the resulting session
    /// actually reaches `SessionState::Running` depends on this network's
    /// NAT hairpinning support — genuinely outside this codebase's control
    /// (see `rusty2600-netplay::stun`'s module doc on the verification
    /// boundary). This test asserts the STUN+hole-punch+session-construction
    /// PLUMBING completes without error for both peers (a real, meaningful
    /// check — a mistake in address handling or socket reuse would fail
    /// here), not that the two peers necessarily finish synchronizing.
    /// `#[ignore]`d for the same reason `rusty2600-netplay`'s own live STUN
    /// test is: requires outbound UDP network access.
    #[test]
    #[ignore = "requires live outbound UDP access to a public STUN server; run explicitly with --ignored"]
    fn connect_via_stun_completes_for_both_peers() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                // `connect_via_stun` needs the PEER's real discovered
                // address up front (matching the out-of-band exchange the
                // real UI requires) — a same-process test has no peer to
                // ask, so do a bare STUN discovery for each side first
                // (on throwaway sockets, since `connect_via_stun` binds its
                // own), then connect for real using each other's result.
                let socket_a = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
                let socket_b = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
                let local_port_a = socket_a.local_addr().unwrap().port();
                let local_port_b = socket_b.local_addr().unwrap().port();
                let stun_server: SocketAddr = "stun.l.google.com:19302"
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap();
                let public_a =
                    rusty2600_netplay::discover_public_address(&socket_a, stun_server).unwrap();
                let public_b =
                    rusty2600_netplay::discover_public_address(&socket_b, stun_server).unwrap();
                drop(socket_a);
                drop(socket_b);

                let (peer_a, discovered_a) =
                    NetplaySession::connect_via_stun(local_port_a, public_b, loaded_system(), 1)
                        .expect(
                            "peer A's STUN+hole-punch+session-construction plumbing should succeed",
                        );
                let (peer_b, discovered_b) =
                    NetplaySession::connect_via_stun(local_port_b, public_a, loaded_system(), 1)
                        .expect(
                            "peer B's STUN+hole-punch+session-construction plumbing should succeed",
                        );

                assert_eq!(discovered_a, public_a);
                assert_eq!(discovered_b, public_b);
                drop(peer_a);
                drop(peer_b);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn two_local_peers_synchronize_and_advance() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let addr_a: SocketAddr = "127.0.0.1:19100".parse().unwrap();
                let addr_b: SocketAddr = "127.0.0.1:19101".parse().unwrap();

                let mut peer_a =
                    NetplaySession::connect(19100, addr_b, loaded_system(), 1).unwrap();
                let mut peer_b =
                    NetplaySession::connect(19101, addr_a, loaded_system(), 1).unwrap();

                let mut advanced_a = 0;
                let mut advanced_b = 0;
                // Enough ticks for GGRS's UDP handshake to complete and several
                // real frames to simulate on localhost.
                for _ in 0..500 {
                    peer_a.poll();
                    peer_b.poll();
                    let input = InputState::default();
                    if matches!(peer_a.advance_frame(&input), Ok(true)) {
                        advanced_a += 1;
                    }
                    if matches!(peer_b.advance_frame(&input), Ok(true)) {
                        advanced_b += 1;
                    }
                    if advanced_a > 10 && advanced_b > 10 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }

                assert!(
                    advanced_a > 0 && advanced_b > 0,
                    "both peers should have advanced at least one real frame \
                     (a={advanced_a}, b={advanced_b})"
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
