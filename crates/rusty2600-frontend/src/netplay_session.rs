//! Rollback netplay frontend wiring (`netplay` feature, off by default).
//!
//! Wires `rusty2600-netplay`'s `RollbackSession` into the frontend per
//! `docs/netplay.md`'s "What's next": a real Connect dialog (see
//! `shell.rs`'s `render_netplay_dialog`) and live per-frame input capture
//! driving a running session. STUN/hole-punch NAT traversal and the WebRTC
//! browser transport remain explicitly deferred (need a genuine external
//! peer no sandbox can verify against) — this wiring is direct-IP/LAN only,
//! matching the session crate's own `[1.10.0]` scope exactly.
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

use std::net::SocketAddr;

use rusty2600_core::System;
use rusty2600_netplay::{NetplayError, PortInput, RollbackSession};

use crate::input::InputState;

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
