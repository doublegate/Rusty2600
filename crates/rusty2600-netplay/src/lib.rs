//! `rusty2600-netplay` — 2-player rollback netplay, wrapping [`ggrs`].
//!
//! Rollback's `resync()` loop is fundamentally "restore a recent snapshot,
//! replay forward" — the exact primitive `rusty2600_core::SaveState`
//! (`[1.1.0]`, ADR 0007) already provides, and the same determinism ADR 0004
//! already guarantees ("same seed + ROM + input ⇒ bit-identical output" is
//! explicitly cited there as "the basis for rewind, run-ahead, and netplay
//! rollback"). No new determinism infrastructure is needed here — this
//! crate is a thin, real integration layer over [`ggrs`] (a mature,
//! independently-maintained GGPO-style rollback engine for Rust) and the
//! save-state substrate, not a from-scratch reimplementation of rollback
//! networking.
//!
//! See `session.rs`'s module docs for the exact scope of what's real in
//! this release (2-player-only UDP transport, direct-IP/LAN, `[2.3.0]`'s
//! STUN-assisted path, and `[2.6.0]`'s browser WebRTC path — see
//! `webrtc.rs`'s module doc + `docs/adr/0008-netplay-webrtc-async-boundary.md`
//! for why that one is `wasm32`-only and how its async surface stays
//! contained). `config.rs`'s module docs explain [`PortInput`] — the
//! per-player wire type this crate actually uses, and why
//! `rusty2600_core::MovieFrame` (which packs BOTH joystick ports together)
//! doesn't fit `ggrs::Config`'s inherently per-player `Input` type directly.

mod checksum;
mod config;
mod frame_advance;
mod session;
mod stun;
/// Browser WebRTC transport (`[2.6.0]`, ADR 0008) — `wasm32`-only. See its
/// own module doc for the full design; `crate::session`'s module doc for
/// how it fits alongside the native UDP/STUN transports.
#[cfg(target_arch = "wasm32")]
mod webrtc;

pub use checksum::checksum128;
pub use config::{PortInput, RustyConfig};
pub use frame_advance::{advance_one_frame, combine};
pub use session::{
    DEFAULT_INPUT_DELAY, DEFAULT_MAX_PREDICTION_WINDOW, LOCAL_PLAYER_HANDLE, NetplayError,
    REMOTE_PLAYER_HANDLE, RollbackSession,
};
pub use stun::{DEFAULT_STUN_SERVER, StunError, discover_public_address, hole_punch};
#[cfg(target_arch = "wasm32")]
pub use webrtc::{
    CreateAnswerResult, CreateOfferResult, SENTINEL_PEER_ADDR, WebRtcPeer, WebRtcSocket,
};

#[cfg(test)]
mod desync_tests {
    //! The rollback-desync test: proves the actual rollback+resimulate path
    //! is correct, not just that the crate compiles.
    //!
    //! Uses `ggrs::SyncTestSession` — GGRS's own built-in determinism-testing
    //! session type, purpose-built for exactly this: every
    //! `advance_frame()` call saves state, advances, then internally rewinds
    //! `check_distance` frames and RE-SIMULATES forward, comparing the
    //! re-simulation's checksums against the originally-recorded ones.
    //! `SyncTestSession::advance_frame()` panics on any mismatch, so "the
    //! test doesn't panic" IS the pass condition — this drives the exact
    //! same `SaveGameState`/`LoadGameState`/`AdvanceFrame` handling
    //! `RollbackSession` uses in `session.rs`, against a REAL
    //! `rusty2600_core::System`, without needing two real network peers
    //! (which this sandbox can't provide a genuine external one for anyway).

    use ggrs::{GgrsRequest, SessionBuilder};
    use rusty2600_cart::{Cartridge, Rom4K};
    use rusty2600_core::{SaveState, System};

    use crate::checksum::checksum128;
    use crate::config::{PortInput, RustyConfig};
    use crate::frame_advance::{advance_one_frame, combine};

    /// A tiny synthetic 4K program whose CPU/RAM state visibly depends on
    /// the joystick input every instruction: `LDA $0280` (the `SWCHA`
    /// RIOT-mirror address) ; `STA $80` (a RIOT zero-page RAM byte) ; `JMP
    /// $1000` (loop). Without a real program like this, `System::new` alone
    /// has no board attached and its state trajectory is nearly
    /// input-independent — the desync test needs state that ACTUALLY reacts
    /// to input divergence to be a meaningful check, not a vacuous one.
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
        rom[0xFFC] = 0x00; // reset vector low -> $1000
        rom[0xFFD] = 0x10; // reset vector high
        Cartridge::Rom4K(Rom4K::new(&rom).expect("exactly 4 KiB"))
    }

    /// Drives one `SyncTestSession::advance_frame()` call's requests against
    /// `system`/`rom_tag`, exactly mirroring `RollbackSession::advance_frame`'s
    /// own request-handling loop (kept as a free function here rather than
    /// reusing `RollbackSession` directly, since a sync test has no real
    /// remote peer / UDP socket to construct one against).
    fn drive_requests(system: &mut System, rom_tag: u64, requests: Vec<GgrsRequest<RustyConfig>>) {
        for request in requests {
            match request {
                GgrsRequest::SaveGameState { cell, frame } => {
                    let blob = SaveState::capture(system, rom_tag).encode();
                    let checksum = checksum128(&blob);
                    cell.save(frame, Some(blob), Some(checksum));
                }
                GgrsRequest::LoadGameState { cell, .. } => {
                    let blob = cell.load().expect("sync test only loads a frame it saved");
                    *system = SaveState::restore(&blob, rom_tag)
                        .expect("a state this test itself saved must decode for its own rom_tag");
                }
                GgrsRequest::AdvanceFrame { inputs } => {
                    let frame = combine(inputs[0].0, inputs[1].0);
                    advance_one_frame(system, frame);
                }
            }
        }
    }

    #[test]
    fn rollback_resimulate_is_deterministic_over_varied_input() {
        // `rusty2600_cart::Cartridge` is an enum sized to its LARGEST
        // variant regardless of which board is actually loaded (e.g.
        // `BankF4`'s inline 32 KiB ROM array) — `System` embeds it, so every
        // `System::clone()` (inside `SaveState::capture`) copies a
        // several-tens-of-KB struct. Several such clones nested a few
        // stack frames deep (this test -> GGRS's own internal rollback
        // recursion -> `drive_requests` -> `SaveState::capture`) overflow
        // the default ~2 MiB test-thread stack. Run on an explicit
        // larger-stack thread rather than relying on an external
        // `RUST_MIN_STACK` env var a CI runner might not set.
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(run_rollback_resimulate_test)
            .expect("spawning the test thread should succeed")
            .join()
            .expect("the test thread should not panic");
    }

    fn run_rollback_resimulate_test() {
        let rom_tag = 0xC0FF_EE00_u64;
        let mut system = System::new(42); // seed is independent of rom_tag
        system.bus.board = Some(synthetic_input_reactive_rom());

        // check_distance < max_prediction, per `SyncTestSession`'s own
        // documented limitation.
        let mut session = SessionBuilder::<RustyConfig>::new()
            .with_num_players(2)
            .unwrap()
            .with_max_prediction_window(8)
            .with_input_delay(0)
            .start_synctest_session()
            .unwrap();

        // A handful of varied inputs across both ports so the two players'
        // combined `MovieFrame`s actually differ frame to frame — a
        // constant-idle input would never exercise real state transitions.
        let inputs: [(PortInput, PortInput); 12] = [
            (PortInput::default(), PortInput::default()),
            (
                PortInput {
                    up: true,
                    ..PortInput::default()
                },
                PortInput::default(),
            ),
            (
                PortInput {
                    up: true,
                    fire: true,
                    ..PortInput::default()
                },
                PortInput {
                    left: true,
                    ..PortInput::default()
                },
            ),
            (
                PortInput::default(),
                PortInput {
                    fire: true,
                    ..PortInput::default()
                },
            ),
            (
                PortInput {
                    right: true,
                    ..PortInput::default()
                },
                PortInput {
                    right: true,
                    ..PortInput::default()
                },
            ),
            (PortInput::default(), PortInput::default()),
            (
                PortInput {
                    down: true,
                    ..PortInput::default()
                },
                PortInput {
                    down: true,
                    fire: true,
                    ..PortInput::default()
                },
            ),
            (
                PortInput {
                    fire: true,
                    ..PortInput::default()
                },
                PortInput::default(),
            ),
            (
                PortInput {
                    left: true,
                    ..PortInput::default()
                },
                PortInput::default(),
            ),
            (
                PortInput::default(),
                PortInput {
                    up: true,
                    ..PortInput::default()
                },
            ),
            (
                PortInput {
                    fire: true,
                    ..PortInput::default()
                },
                PortInput {
                    fire: true,
                    ..PortInput::default()
                },
            ),
            (PortInput::default(), PortInput::default()),
        ];

        for (p0, p1) in inputs {
            session.add_local_input(0, p0).unwrap();
            session.add_local_input(1, p1).unwrap();
            // `advance_frame()` internally performs save -> advance ->
            // rollback `check_distance` frames -> re-simulate -> compare
            // checksums, and PANICS on any mismatch — reaching the end of
            // this loop without a panic is the actual test assertion.
            let requests = session.advance_frame().unwrap();
            drive_requests(&mut system, rom_tag, requests);
        }
    }
}
