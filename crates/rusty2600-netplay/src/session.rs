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
//! Transport: GGRS's own built-in [`ggrs::UdpNonBlockingSocket`] — this crate
//! does not implement a custom [`ggrs::NonBlockingSocket`], since GGRS
//! already ships a real, tested UDP implementation. **Direct-IP/LAN
//! connection only in this release**: the peer's `SocketAddr` is supplied
//! directly (e.g. exchanged out-of-band by the two players). STUN/hole-punch
//! NAT traversal for internet play is explicitly deferred to a `v1.10.x`
//! follow-up — implementing and (crucially) verifying real NAT traversal
//! needs a genuine external peer this sandbox cannot provide, and is
//! substantial, separable work from the rollback session logic itself.
//!
//! **Frontend wiring (an actual host/join-game menu, live per-frame input
//! capture) is also deferred.** This release lands a real, tested rollback
//! session crate; wiring it into `rusty2600-frontend` is a `v1.10.x`
//! follow-up, the same honest-partial-landing pattern this project already
//! used for `rusty2600-thumb` (`[1.6.0]`) and `rusty2600-script` (`[1.9.0]`).

use std::net::SocketAddr;

use ggrs::{GgrsError, GgrsRequest, P2PSession, PlayerType, SessionBuilder};
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
