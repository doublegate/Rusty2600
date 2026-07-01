//! Run-ahead: hides a game's internal input lag by speculatively simulating
//! a few frames ahead of the canonical timeline and displaying THAT, while
//! discarding the speculative work so the real, persistent timeline never
//! sees it.
//!
//! Built entirely on the snapshot/restore primitives [`crate::emu_thread`]
//! already has for save-states and rewind (`[1.1.0]`) — no new serialization
//! machinery needed.
//!
//! The algorithm, per real `step_frame` call with `runahead_frames = N`:
//!
//! 1. Run the real, persistent frame with the latched input — rewind capture
//!    stays ON (this frame really happened; it belongs in rewind history),
//!    but its audio is suppressed (see step 4 for why).
//! 2. Snapshot the resulting state — this is the checkpoint step 5 restores.
//! 3. Run `N - 1` hidden frames with the SAME input (there's no future input
//!    to use — run-ahead's whole premise is "assume input stays as it was
//!    and get ahead of where the display would otherwise be"), rewind
//!    capture and audio both suppressed: none of this is canonical.
//! 4. Run one more frame with the same input, rewind capture still
//!    suppressed but audio NOT suppressed and published to the REAL frame
//!    producer — this is the one frame the user actually sees and hears.
//!    Only one frame's worth of audio must ever reach the ring per real
//!    tick, or run-ahead would play audio N times too fast.
//! 5. Restore the checkpoint from step 2, discarding the entire speculative
//!    run (steps 3-4) — the canonical/persistent timeline is exactly where
//!    step 1 left it, unaffected by how far ahead the display just looked.
//!
//! Snapshot/restore happens only at frame boundaries (the VSYNC edge where
//! [`rusty2600_core::tia::Tia::video_buffer`] is drained), never mid-instruction
//! — the WSYNC/RDY beam-stall is a sub-instruction, sub-frame mechanic that
//! never coincides with a frame-boundary snapshot point, and `Tia::rdy_stall`
//! is a plain serialized field, so a restore mid-stall (were it ever to
//! happen, which it doesn't here) would round-trip correctly regardless.

use rusty2600_core::SaveState;

use crate::emu_thread::EmuCore;
use crate::present_buffer::Producer as FrameProducer;

/// Step one frame with `runahead_frames` frames of look-ahead (`0` is
/// equivalent to a plain [`EmuCore::step_frame`] call — no speculative work,
/// no snapshot/restore overhead).
pub fn step_frame(
    core: &mut EmuCore,
    frames: &FrameProducer,
    input: Option<(u8, u8, u8, u8)>,
    runahead_frames: u32,
) {
    if runahead_frames == 0 {
        core.step_frame(frames, input);
        return;
    }

    // A scratch producer for every frame the caller must never see (the
    // persistent step's own framebuffer, and every hidden speculative
    // frame) — its consumer is simply dropped, discarding those frames.
    let (scratch_tx, _scratch_rx) = crate::present_buffer::channel();

    // 1. The real, persistent frame: rewind capture ON, audio OFF (the
    //    speculative frame in step 4 supplies this tick's audio instead).
    core.audio_output_suppressed = true;
    core.step_frame(&scratch_tx, input);
    core.audio_output_suppressed = false;

    // 2. Checkpoint the canonical post-frame state.
    let checkpoint = SaveState::capture(&core.system, 0).encode();

    // 3. Hidden frames: neither rewind nor audio is canonical.
    core.rewind_capture_suppressed = true;
    core.audio_output_suppressed = true;
    for _ in 0..runahead_frames.saturating_sub(1) {
        core.step_frame(&scratch_tx, input);
    }

    // 4. The displayed speculative frame: still not rewind-worthy, but its
    //    audio and video ARE what the user actually perceives this tick.
    core.audio_output_suppressed = false;
    core.step_frame(frames, input);
    core.rewind_capture_suppressed = false;

    // 5. Discard the entire speculative run (steps 3-4); the canonical
    //    timeline resumes exactly where step 1 left it.
    if let Ok(state) = SaveState::decode(&checkpoint) {
        core.system = state.into_system();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_runahead_is_equivalent_to_a_plain_step() {
        let (tx, _rx) = crate::present_buffer::channel();
        let mut plain = EmuCore::new(0);
        plain.rom_loaded = true;
        plain.step_frame(&tx, None);

        let mut ra = EmuCore::new(0);
        ra.rom_loaded = true;
        step_frame(&mut ra, &tx, None, 0);

        assert_eq!(plain.system.color_clocks(), ra.system.color_clocks());
    }

    #[test]
    fn persistent_timeline_matches_a_plain_run_regardless_of_lookahead() {
        let (tx, _rx) = crate::present_buffer::channel();

        let mut plain = EmuCore::new(0);
        plain.rom_loaded = true;
        plain.step_frame(&tx, None);

        for lookahead in [1, 2, 5] {
            let mut ra = EmuCore::new(0);
            ra.rom_loaded = true;
            step_frame(&mut ra, &tx, None, lookahead);

            assert_eq!(
                plain.system.color_clocks(),
                ra.system.color_clocks(),
                "lookahead={lookahead}: persistent timeline must match a plain run"
            );
            assert_eq!(
                plain.system.cpu.pc, ra.system.cpu.pc,
                "lookahead={lookahead}: persistent PC must match a plain run"
            );
        }
    }

    #[test]
    fn speculative_frames_never_enter_rewind_history() {
        let (tx, _rx) = crate::present_buffer::channel();
        let mut ra = EmuCore::new(0);
        ra.rom_loaded = true;

        step_frame(&mut ra, &tx, None, 5);

        // Only the one real, persistent frame should have pushed a rewind
        // snapshot — the 4 hidden frames and the 1 displayed speculative
        // frame must all have been suppressed.
        assert_eq!(ra.snapshots.len(), 1);

        // The suppression flags must be reset afterward, not left dangling.
        assert!(!ra.rewind_capture_suppressed);
        assert!(!ra.audio_output_suppressed);
    }
}
