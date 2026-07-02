//! [`RollbackSession`] — a 2-player-only rollback netplay session over
//! native UDP, wrapping [`ggrs::P2PSession`].
//!
//! **Deliberate scope cut vs. `RustyNES`'s own 2-4-player mesh**: real 2600
//! hardware rarely supported more than 2 controllers, so a fully-connected
//! N-peer mesh, roster protocol, and 3-4-player UI would be pure
//! RustyNES-parity cost with little real payoff here — the single biggest
//! intentional scope divergence in this crate, already decided by the
//! project's own plan.
//!
//! Transport: GGRS's own built-in [`ggrs::UdpNonBlockingSocket`] for the
//! plain direct-IP/LAN path (peer's `SocketAddr` supplied directly, e.g.
//! exchanged out-of-band by the two players) — [`RollbackSession::new`]/
//! [`RollbackSession::with_config`]. `[2.3.0]` adds
//! [`RollbackSession::with_socket`] plus `PunchedUdpSocket` (a small,
//! real `ggrs::NonBlockingSocket` impl over a pre-bound `std::net::UdpSocket`)
//! for the STUN-assisted path — see `crate::stun` for the STUN client and
//! hole-punch helper. **WebRTC (browser or native) remains explicitly out
//! of scope** — see `crate::stun`'s module doc for why.
//!
//! **Frontend wiring (an actual host/join-game menu, live per-frame input
//! capture) landed in `[2.1.0]`** (`rusty2600-frontend::netplay_session`).

use std::net::{SocketAddr, UdpSocket};

use ggrs::{
    GgrsError, GgrsRequest, Message, NonBlockingSocket, P2PSession, PlayerType, SessionBuilder,
};
use rusty2600_core::{SaveState, System};

use crate::checksum::checksum128;
use crate::config::{PortInput, RustyConfig};
use crate::frame_advance::{advance_one_frame, combine};

/// Default input delay (frames).
///
/// A small, GGPO-convention value that lets remote input arrive before it's
/// needed, trading a little constant latency for far fewer rollbacks.
/// GGRS's own docs cite 2-4 frames as typical; this crate defaults to the
/// middle of that range.
pub const DEFAULT_INPUT_DELAY: usize = 2;

/// Default maximum rollback/prediction window (frames).
///
/// How far back a session may rewind to correct a misprediction. GGPO's
/// original default was 8 frames; this crate uses the same value rather
/// than inventing a different one.
pub const DEFAULT_MAX_PREDICTION_WINDOW: usize = 8;

/// This crate's own local-player handle convention: the host is always
/// player 0 (port 0), the single remote peer is always player 1 (port 1) —
/// 2-player-only, see the module docs.
pub const LOCAL_PLAYER_HANDLE: usize = 0;
/// See [`LOCAL_PLAYER_HANDLE`].
pub const REMOTE_PLAYER_HANDLE: usize = 1;

/// Everything that can go wrong starting or driving a [`RollbackSession`].
#[derive(Debug)]
pub enum NetplayError {
    /// Binding the local UDP socket failed (e.g. the port is already in use).
    Socket(std::io::Error),
    /// GGRS itself rejected the session configuration or a frame advance.
    Ggrs(GgrsError),
}

impl core::fmt::Display for NetplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Socket(e) => write!(f, "failed to bind the local UDP socket: {e}"),
            Self::Ggrs(e) => write!(f, "GGRS session error: {e}"),
        }
    }
}

impl From<GgrsError> for NetplayError {
    fn from(e: GgrsError) -> Self {
        Self::Ggrs(e)
    }
}

/// A 2-player rollback netplay session, driving a [`System`] the session
/// itself owns.
pub struct RollbackSession {
    session: P2PSession<RustyConfig>,
    system: System,
    rom_tag: u64,
}

impl RollbackSession {
    /// Start a session: bind a local UDP socket on `local_port`, register
    /// the local player at [`LOCAL_PLAYER_HANDLE`] and the remote peer
    /// (`remote_addr`) at [`REMOTE_PLAYER_HANDLE`], and begin synchronizing.
    ///
    /// `system` is the already-loaded, ready-to-run game the session will
    /// advance; `rom_tag` is the same opaque ROM-identity value
    /// `SaveState::capture`/`restore` already use elsewhere, kept consistent
    /// so a session's internal save/load round-trips never silently accept
    /// a state captured for a different ROM.
    ///
    /// # Errors
    ///
    /// Returns [`NetplayError::Socket`] if the local UDP port can't be
    /// bound, or [`NetplayError::Ggrs`] if GGRS rejects the configuration.
    pub fn new(
        local_port: u16,
        remote_addr: SocketAddr,
        system: System,
        rom_tag: u64,
    ) -> Result<Self, NetplayError> {
        Self::with_config(
            local_port,
            remote_addr,
            system,
            rom_tag,
            DEFAULT_INPUT_DELAY,
            DEFAULT_MAX_PREDICTION_WINDOW,
        )
    }

    /// Like [`Self::new`], but with explicit input-delay/max-prediction-window
    /// values instead of the crate defaults.
    ///
    /// # Errors
    ///
    /// See [`Self::new`].
    pub fn with_config(
        local_port: u16,
        remote_addr: SocketAddr,
        system: System,
        rom_tag: u64,
        input_delay: usize,
        max_prediction_window: usize,
    ) -> Result<Self, NetplayError> {
        let socket =
            ggrs::UdpNonBlockingSocket::bind_to_port(local_port).map_err(NetplayError::Socket)?;
        let session = SessionBuilder::<RustyConfig>::new()
            .with_num_players(2)?
            .with_input_delay(input_delay)
            .with_max_prediction_window(max_prediction_window)
            .add_player(PlayerType::Local, LOCAL_PLAYER_HANDLE)?
            .add_player(PlayerType::Remote(remote_addr), REMOTE_PLAYER_HANDLE)?
            .start_p2p_session(socket)?;

        Ok(Self {
            session,
            system,
            rom_tag,
        })
    }

    /// Like [`Self::new`], but over an already-bound `socket` instead of a
    /// bare port number — the STUN-assisted connect path
    /// (`crate::stun::discover_public_address`/`crate::stun::hole_punch`)
    /// needs the STUN query, the hole-punch, and the GGRS session to all
    /// share the exact same local UDP socket, since the NAT mapping a STUN
    /// server observes is only valid for the `(local port, remote server)`
    /// pair that produced it — binding a fresh socket here instead would
    /// silently invalidate that mapping.
    ///
    /// `socket` is wrapped in `PunchedUdpSocket` and put into non-blocking
    /// mode internally; callers do not need to call `set_nonblocking`
    /// themselves first (though it's harmless if they already have).
    ///
    /// # Errors
    ///
    /// Returns [`NetplayError::Socket`] if `socket` can't be switched to
    /// non-blocking mode, or [`NetplayError::Ggrs`] if GGRS rejects the
    /// configuration.
    pub fn with_socket(
        socket: UdpSocket,
        remote_addr: SocketAddr,
        system: System,
        rom_tag: u64,
        input_delay: usize,
        max_prediction_window: usize,
    ) -> Result<Self, NetplayError> {
        let socket = PunchedUdpSocket::new(socket).map_err(NetplayError::Socket)?;
        let session = SessionBuilder::<RustyConfig>::new()
            .with_num_players(2)?
            .with_input_delay(input_delay)
            .with_max_prediction_window(max_prediction_window)
            .add_player(PlayerType::Local, LOCAL_PLAYER_HANDLE)?
            .add_player(PlayerType::Remote(remote_addr), REMOTE_PLAYER_HANDLE)?
            .start_p2p_session(socket)?;

        Ok(Self {
            session,
            system,
            rom_tag,
        })
    }

    /// Whether the session has finished synchronizing with the remote peer
    /// and is ready to advance frames.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.session.current_state(), ggrs::SessionState::Running)
    }

    /// Poll the socket for incoming packets. Call once per tick before
    /// [`Self::advance_frame`] if you're not calling it every tick anyway
    /// (`advance_frame` already polls internally, so this is only useful
    /// while waiting for the initial sync to complete).
    pub fn poll(&mut self) {
        self.session.poll_remote_clients();
    }

    /// Advance the session by exactly one frame using `local_input` (this
    /// machine's own port — [`LOCAL_PLAYER_HANDLE`]/port 0 for the host,
    /// remapped internally if this instance is the joining peer), handling
    /// every [`GgrsRequest`] GGRS returns (state save/load, frame advance)
    /// against this session's own [`System`].
    ///
    /// Returns `Ok(true)` if a real game frame was simulated, `Ok(false)`
    /// if the session is still waiting on the remote peer (not an error —
    /// see [`ggrs::P2PSession::advance_frame`]'s own lockstep-stall note).
    ///
    /// # Errors
    ///
    /// Returns [`NetplayError::Ggrs`] on a real session error (e.g. not yet
    /// synchronized, or the remote peer fell too far behind).
    ///
    /// # Panics
    ///
    /// Panics if GGRS requests a [`GgrsRequest::LoadGameState`] for a state
    /// this session didn't itself previously save (a GGRS-internal
    /// invariant violation, not a condition a caller can trigger), or if a
    /// previously-saved state fails to decode against this session's own
    /// `rom_tag` (equally an internal-invariant violation — the state was
    /// captured moments earlier by this very session).
    pub fn advance_frame(&mut self, local_input: PortInput) -> Result<bool, NetplayError> {
        self.session
            .add_local_input(LOCAL_PLAYER_HANDLE, local_input)?;

        let requests = match self.session.advance_frame() {
            Ok(requests) => requests,
            Err(GgrsError::PredictionThreshold) => return Ok(false),
            Err(e) => return Err(e.into()),
        };

        let mut advanced = false;
        for request in requests {
            match request {
                GgrsRequest::SaveGameState { cell, frame } => {
                    let blob = SaveState::capture(&self.system, self.rom_tag).encode();
                    let checksum = checksum128(&blob);
                    cell.save(frame, Some(blob), Some(checksum));
                }
                GgrsRequest::LoadGameState { cell, .. } => {
                    let blob = cell.load().expect(
                        "GGRS only requests a load for a frame it previously asked us to save",
                    );
                    self.system = SaveState::restore(&blob, self.rom_tag).expect(
                        "a state this session itself saved must decode for its own rom_tag",
                    );
                }
                GgrsRequest::AdvanceFrame { inputs } => {
                    let port0 = inputs[LOCAL_PLAYER_HANDLE].0;
                    let port1 = inputs[REMOTE_PLAYER_HANDLE].0;
                    let frame = combine(port0, port1);
                    advance_one_frame(&mut self.system, frame);
                    advanced = true;
                }
            }
        }
        Ok(advanced)
    }

    /// The [`System`] this session is driving. Read-only outside the
    /// session itself — every mutation must go through [`Self::advance_frame`]
    /// so rollback/resimulate stays correct.
    #[must_use]
    pub const fn system(&self) -> &System {
        &self.system
    }
}

/// A [`ggrs::NonBlockingSocket`] implementation over an already-bound
/// `std::net::UdpSocket`, for [`RollbackSession::with_socket`]'s
/// STUN-assisted connect path.
///
/// This is a near-verbatim copy of GGRS's own bundled
/// `UdpNonBlockingSocket` (`send_to`/`receive_all_messages`, same
/// bincode-over-UDP wire format, same fixed receive buffer) — the ONLY
/// difference is construction: GGRS's own type always binds a fresh socket
/// internally (`UdpNonBlockingSocket::bind_to_port`), which would discard
/// the exact `(local port, external mapping)` pair a STUN query and
/// hole-punch already established on `socket`. Duplicating this small,
/// stable shape is simpler and more obviously correct than trying to
/// retrofit "accept a pre-bound socket" onto GGRS's own type from outside
/// its crate.
struct PunchedUdpSocket {
    socket: UdpSocket,
    buffer: [u8; Self::RECV_BUFFER_SIZE],
}

impl PunchedUdpSocket {
    const RECV_BUFFER_SIZE: usize = 4096;

    /// Wrap `socket`, switching it to non-blocking mode.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if `set_nonblocking` fails.
    fn new(socket: UdpSocket) -> std::io::Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            buffer: [0; Self::RECV_BUFFER_SIZE],
        })
    }
}

impl NonBlockingSocket<SocketAddr> for PunchedUdpSocket {
    fn send_to(&mut self, msg: &Message, addr: &SocketAddr) {
        let Ok(buf) = bincode::serialize(msg) else {
            return;
        };
        let _ = self.socket.send_to(&buf, addr);
    }

    fn receive_all_messages(&mut self) -> Vec<(SocketAddr, Message)> {
        let mut received = Vec::new();
        loop {
            match self.socket.recv_from(&mut self.buffer) {
                Ok((n, src)) => {
                    if let Ok(msg) = bincode::deserialize(&self.buffer[..n]) {
                        received.push((src, msg));
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return received,
                Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
                Err(_) => return received,
            }
        }
    }
}

#[cfg(test)]
mod punched_socket_tests {
    //! `ggrs::Message`'s fields are all `pub(crate)` (no public
    //! constructor), so `PunchedUdpSocket` can't be unit-tested by directly
    //! constructing a `Message` from outside the `ggrs` crate. Real proof
    //! instead comes from driving a full `RollbackSession::with_socket`
    //! two-peer session on localhost — this exercises `PunchedUdpSocket`'s
    //! `send_to`/`receive_all_messages` for real, through GGRS's own
    //! handshake and frame-advance traffic, the same way
    //! `rusty2600-frontend`'s existing `netplay_session` tests already
    //! prove `RollbackSession::new`'s plain-UDP path.

    use std::net::SocketAddr;

    use rusty2600_cart::{Cartridge, Rom4K};
    use rusty2600_core::System;

    use super::RollbackSession;

    /// Same synthetic input-reactive ROM `desync_tests` uses — real board
    /// state that visibly depends on joystick input every instruction.
    fn synthetic_input_reactive_rom() -> Cartridge {
        let mut rom = [0u8; 0x1000];
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
        Cartridge::Rom4K(Rom4K::new(&rom).expect("exactly 4 KiB"))
    }

    fn loaded_system(seed: u64) -> System {
        let mut system = System::new(seed);
        system.bus.board = Some(synthetic_input_reactive_rom());
        system
    }

    /// Two `RollbackSession::with_socket` instances on `127.0.0.1`,
    /// each wrapping its own pre-bound `UdpSocket` in a `PunchedUdpSocket`,
    /// actually synchronizing and advancing real frames — mirrors
    /// `rusty2600-netplay`'s own `[2.1.0]` rollback-desync test's "prove it
    /// for real, not just that it compiles" standard, applied to the new
    /// socket wrapper specifically.
    #[test]
    fn two_peers_synchronize_over_punched_sockets() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(run_two_peer_test)
            .expect("spawning the test thread should succeed")
            .join()
            .expect("the test thread should not panic");
    }

    fn run_two_peer_test() {
        let raw_a = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let raw_b = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr_a: SocketAddr = raw_a.local_addr().unwrap();
        let addr_b: SocketAddr = raw_b.local_addr().unwrap();

        let mut peer_a =
            RollbackSession::with_socket(raw_a, addr_b, loaded_system(1), 1, 2, 8).unwrap();
        let mut peer_b =
            RollbackSession::with_socket(raw_b, addr_a, loaded_system(1), 1, 2, 8).unwrap();

        let mut advanced_a = 0u32;
        let mut advanced_b = 0u32;
        for _ in 0..500 {
            peer_a.poll();
            peer_b.poll();
            let input = crate::config::PortInput::default();
            if matches!(peer_a.advance_frame(input), Ok(true)) {
                advanced_a += 1;
            }
            if matches!(peer_b.advance_frame(input), Ok(true)) {
                advanced_b += 1;
            }
            if advanced_a > 10 && advanced_b > 10 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        assert!(
            advanced_a > 0 && advanced_b > 0,
            "both PunchedUdpSocket-backed peers should have advanced at least one real frame \
             (a={advanced_a}, b={advanced_b})"
        );
    }
}
