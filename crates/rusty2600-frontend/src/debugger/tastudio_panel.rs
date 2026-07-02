//! TAStudio-lite: a piano-roll input grid for recording/editing a
//! [`rusty2600_core::Movie`].
//!
//! Deliberately scoped below RustyNES's own ~609-line TAStudio panel: a flat
//! recorded-frame grid (click-to-toggle editing per input bit), branch
//! points saved as separate `.r26m` files (`Movie::new_branch`, reusing the
//! existing save-state encoding rather than a parallel snapshot system), and
//! jump-to-frame driven by the EXISTING rewind ring
//! (`crate::emu_thread::EmuCore::snapshots`/`rewind`) — there is no separate
//! "greenzone" structure to build, `v1.1.0`'s rewind ring already is one.
//! Foreign movie-format import (no existing 2600 movie format to import) and
//! branch-tree visualization (a flat list of saved branch files is enough
//! for a first cut) are both explicitly out of scope.
//!
//! This module lands the format-integration + the panel's own state machine
//! and rendering; wiring live per-frame recording into `EmuCore::run_frame`'s
//! hot path (so every frame the emulator actually runs gets appended
//! automatically) is left as a follow-up, the same honest-partial-landing
//! call this project already made for `v1.4.0`'s sprite-pack data model
//! (shipped without its render splice, clearly documented as deferred).

use rusty2600_core::{Movie, MovieFrame, MovieRegion};

/// Bit positions within [`MovieFrame::swcha`].
///
/// Matches `rusty2600_frontend::input::Joystick::swcha_nibble`'s packing
/// exactly: port 0 occupies the high nibble (bits 7-4,
/// up/down/left/right), port 1 the low nibble (bits 3-0). Active-low: a
/// pressed direction clears its bit.
pub const SWCHA_P0_UP: u8 = 7;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P0_DOWN: u8 = 6;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P0_LEFT: u8 = 5;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P0_RIGHT: u8 = 4;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P1_UP: u8 = 3;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P1_DOWN: u8 = 2;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P1_LEFT: u8 = 1;
/// See [`SWCHA_P0_UP`].
pub const SWCHA_P1_RIGHT: u8 = 0;

/// Whether the direction at `bit` (one of the `SWCHA_*` constants) is
/// pressed in `frame` (active-low: pressed = bit clear).
#[must_use]
pub const fn swcha_pressed(frame: MovieFrame, bit: u8) -> bool {
    (frame.swcha >> bit) & 1 == 0
}

/// Set or clear the direction at `bit` (active-low: pressed clears the bit).
pub const fn set_swcha_pressed(frame: &mut MovieFrame, bit: u8, pressed: bool) {
    if pressed {
        frame.swcha &= !(1 << bit);
    } else {
        frame.swcha |= 1 << bit;
    }
}

/// Whether joystick `port`'s (`0` or `1`) fire button is pressed in `frame`.
///
/// `MovieFrame::joy_fire`'s own convention: active-HIGH, bit set = pressed
/// — unlike `swcha`, this isn't a direct hardware port byte, so there's no
/// need to mirror `SWCHA`'s active-low convention here.
#[must_use]
pub const fn fire_pressed(frame: MovieFrame, port: u8) -> bool {
    (frame.joy_fire >> port) & 1 != 0
}

/// Set or clear joystick `port`'s fire button.
pub const fn set_fire_pressed(frame: &mut MovieFrame, port: u8, pressed: bool) {
    if pressed {
        frame.joy_fire |= 1 << port;
    } else {
        frame.joy_fire &= !(1 << port);
    }
}

/// Whether `frame`'s Select switch is engaged.
///
/// Active-low on `SWCHB` bit 1, matching
/// `rusty2600_frontend::input::ConsoleSwitches::swchb`.
#[must_use]
pub const fn select_pressed(frame: MovieFrame) -> bool {
    (frame.swchb >> 1) & 1 == 0
}

/// Set or clear Select.
pub const fn set_select_pressed(frame: &mut MovieFrame, pressed: bool) {
    if pressed {
        frame.swchb &= !0b0000_0010;
    } else {
        frame.swchb |= 0b0000_0010;
    }
}

/// Whether `frame`'s Reset switch is engaged (active-low on `SWCHB` bit 0).
#[must_use]
pub const fn reset_pressed(frame: MovieFrame) -> bool {
    frame.swchb & 1 == 0
}

/// Set or clear Reset.
pub const fn set_reset_pressed(frame: &mut MovieFrame, pressed: bool) {
    if pressed {
        frame.swchb &= !0b0000_0001;
    } else {
        frame.swchb |= 0b0000_0001;
    }
}

/// Whether `frame`'s Color/B&W switch is set to Color.
///
/// Active-HIGH on `SWCHB` bit 3 — the opposite polarity from the momentary
/// switches above, since this one is a latching toggle, not a "pressed"
/// button.
#[must_use]
pub const fn color_engaged(frame: MovieFrame) -> bool {
    (frame.swchb >> 3) & 1 != 0
}

/// Set Color (true) or B&W (false).
pub const fn set_color_engaged(frame: &mut MovieFrame, color: bool) {
    if color {
        frame.swchb |= 0b0000_1000;
    } else {
        frame.swchb &= !0b0000_1000;
    }
}

/// Which debugger action a TAStudio interaction requests.
///
/// Bubbles up through [`crate::shell::ShellState::render`] the same way
/// [`super::DebugAction`] already does, so this panel never touches the
/// emu lock either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TastudioAction {
    /// Jump playback/rewind to this recorded frame index (drives
    /// `EmuCore::rewind()` repeatedly until `frame_count` matches, bounded
    /// by the existing 600-frame rewind-ring capacity).
    JumpToFrame(usize),
    /// Save a branch point (a new `.r26m` whose start is the current
    /// system state) to disk.
    SaveBranch,
}

/// Recording/editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TastudioMode {
    /// No movie in progress.
    #[default]
    Idle,
    /// Actively appending frames as the emulator runs.
    Recording,
    /// Reviewing/editing an already-recorded movie; not appending.
    Playback,
}

/// Persistent TAStudio-lite state: the in-progress/loaded movie, the
/// current cursor (selected/playback frame), and mode.
#[derive(Debug, Default, Clone)]
pub struct TastudioState {
    /// The movie being recorded or edited, if any.
    pub movie: Option<Movie>,
    /// Current mode.
    pub mode: TastudioMode,
    /// The selected/playback frame index (piano-roll cursor).
    pub cursor: usize,
    /// The text the user is typing into the "save branch as" field.
    pub branch_name_input: String,
}

impl TastudioState {
    /// Begin a new recording from a fresh power-on.
    pub fn start_recording(&mut self, rom_tag: u64, region: MovieRegion, seed: u64) {
        self.movie = Some(Movie::new_power_on(rom_tag, region, seed));
        self.mode = TastudioMode::Recording;
        self.cursor = 0;
    }

    /// Append one frame while [`TastudioMode::Recording`]. No-op otherwise.
    pub fn record_frame(&mut self, frame: MovieFrame) {
        if self.mode != TastudioMode::Recording {
            return;
        }
        if let Some(movie) = &mut self.movie {
            movie.record_frame(frame);
            self.cursor = movie.len();
        }
    }

    /// Stop recording and switch to reviewing/editing what was captured.
    pub fn stop_recording(&mut self) {
        if self.mode == TastudioMode::Recording {
            self.mode = TastudioMode::Playback;
        }
    }

    /// Overwrite the frame at `index` (piano-roll cell editing).
    pub fn edit_frame(&mut self, index: usize, frame: MovieFrame) {
        if let Some(movie) = &mut self.movie {
            movie.set_frame(index, frame);
        }
    }

    /// Move the cursor to `index` and request the frontend jump playback
    /// there (via the existing rewind ring).
    #[must_use]
    pub const fn jump_to(&mut self, index: usize) -> TastudioAction {
        self.cursor = index;
        TastudioAction::JumpToFrame(index)
    }

    /// Save a branch point: a new movie whose start point is `system`'s
    /// current state, encoded as `.r26m` bytes ready to write to disk.
    #[must_use]
    pub fn save_branch(
        &self,
        rom_tag: u64,
        region: MovieRegion,
        system: &rusty2600_core::System,
    ) -> Vec<u8> {
        Movie::new_branch(rom_tag, region, system).encode()
    }
}

/// Renders the piano-roll grid: one row per recorded frame, one toggleable
/// checkbox column per input bit (P0/P1 directions + fire, Select, Reset,
/// Color).
///
/// Returns any [`TastudioAction`]s the user requested this frame.
pub fn render_tastudio_panel(ui: &mut egui::Ui, state: &mut TastudioState) -> Vec<TastudioAction> {
    let mut actions = Vec::new();

    ui.horizontal(|ui| {
        ui.label(format!("Mode: {:?}", state.mode));
        ui.separator();
        if let Some(movie) = &state.movie {
            ui.label(format!("{} frames recorded", movie.len()));
        } else {
            ui.label("(no movie in progress)");
        }
    });
    ui.separator();

    let Some(movie) = &mut state.movie else {
        ui.label("Start a recording from Emulation -> TAStudio to begin.");
        return actions;
    };

    // `TastudioAction::SaveBranch`'s dispatch needs `rfd`'s native-only save-file dialog (see
    // `MenuAction::TastudioSaveBranch`'s doc comment in `shell.rs`) — the wasm32 debugger
    // overlay (`[v2.9.0]`) hides this row entirely rather than wiring a button that would push
    // an action nothing on that target ever handles.
    #[cfg(not(target_arch = "wasm32"))]
    {
        ui.horizontal(|ui| {
            ui.label("Save branch as:");
            ui.text_edit_singleline(&mut state.branch_name_input);
            if ui.button("Save").clicked() {
                actions.push(TastudioAction::SaveBranch);
            }
        });
        ui.separator();
    }
    #[cfg(target_arch = "wasm32")]
    {
        ui.label("Save branch: native-only (no file-save dialog in the browser).");
        ui.separator();
    }

    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("tastudio_piano_roll")
                .num_columns(12)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("#");
                    for label in [
                        "P0 U", "P0 D", "P0 L", "P0 R", "P0 F", "P1 U", "P1 D", "P1 L", "P1 R",
                        "P1 F", "Go",
                    ] {
                        ui.label(label);
                    }
                    ui.end_row();

                    for i in 0..movie.len() {
                        let Some(mut frame) = movie.frame_at(i) else {
                            continue;
                        };
                        let mut changed = false;
                        let marker = if i == state.cursor { "> " } else { "  " };
                        ui.monospace(format!("{marker}{i}"));

                        for bit in [SWCHA_P0_UP, SWCHA_P0_DOWN, SWCHA_P0_LEFT, SWCHA_P0_RIGHT] {
                            let mut pressed = swcha_pressed(frame, bit);
                            if ui.checkbox(&mut pressed, "").changed() {
                                set_swcha_pressed(&mut frame, bit, pressed);
                                changed = true;
                            }
                        }
                        let mut p0_fire = fire_pressed(frame, 0);
                        if ui.checkbox(&mut p0_fire, "").changed() {
                            set_fire_pressed(&mut frame, 0, p0_fire);
                            changed = true;
                        }
                        for bit in [SWCHA_P1_UP, SWCHA_P1_DOWN, SWCHA_P1_LEFT, SWCHA_P1_RIGHT] {
                            let mut pressed = swcha_pressed(frame, bit);
                            if ui.checkbox(&mut pressed, "").changed() {
                                set_swcha_pressed(&mut frame, bit, pressed);
                                changed = true;
                            }
                        }
                        let mut p1_fire = fire_pressed(frame, 1);
                        if ui.checkbox(&mut p1_fire, "").changed() {
                            set_fire_pressed(&mut frame, 1, p1_fire);
                            changed = true;
                        }

                        if ui.small_button("Go").clicked() {
                            state.cursor = i;
                            actions.push(TastudioAction::JumpToFrame(i));
                        }
                        ui.end_row();

                        if changed {
                            movie.set_frame(i, frame);
                        }
                    }
                });
        });

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swcha_bit_round_trips() {
        let mut frame = MovieFrame::default();
        assert!(
            !swcha_pressed(frame, SWCHA_P0_UP),
            "idle reads high (not pressed)"
        );
        set_swcha_pressed(&mut frame, SWCHA_P0_UP, true);
        assert!(swcha_pressed(frame, SWCHA_P0_UP));
        set_swcha_pressed(&mut frame, SWCHA_P0_UP, false);
        assert!(!swcha_pressed(frame, SWCHA_P0_UP));
    }

    #[test]
    fn p0_and_p1_directions_occupy_separate_nibbles() {
        let mut frame = MovieFrame::default();
        set_swcha_pressed(&mut frame, SWCHA_P0_UP, true);
        // P1's bits must be untouched by a P0 edit.
        assert!(!swcha_pressed(frame, SWCHA_P1_UP));
        assert!(!swcha_pressed(frame, SWCHA_P1_DOWN));
    }

    #[test]
    fn fire_bits_round_trip_independently_per_port() {
        let mut frame = MovieFrame::default();
        set_fire_pressed(&mut frame, 0, true);
        assert!(fire_pressed(frame, 0));
        assert!(!fire_pressed(frame, 1));
        set_fire_pressed(&mut frame, 1, true);
        assert!(fire_pressed(frame, 1));
    }

    #[test]
    fn select_and_reset_are_independent_active_low_bits() {
        let mut frame = MovieFrame {
            swchb: 0xFF,
            ..Default::default()
        };
        assert!(!select_pressed(frame));
        assert!(!reset_pressed(frame));
        set_select_pressed(&mut frame, true);
        assert!(select_pressed(frame));
        assert!(!reset_pressed(frame), "reset bit must be untouched");
        set_reset_pressed(&mut frame, true);
        assert!(reset_pressed(frame));
    }

    #[test]
    fn color_switch_is_active_high_unlike_the_momentary_switches() {
        let mut frame = MovieFrame {
            swchb: 0x00,
            ..Default::default()
        };
        assert!(!color_engaged(frame), "bit clear means B&W");
        set_color_engaged(&mut frame, true);
        assert!(color_engaged(frame));
    }

    #[test]
    fn recording_lifecycle_appends_then_stops() {
        let mut state = TastudioState::default();
        assert_eq!(state.mode, TastudioMode::Idle);
        state.start_recording(1, MovieRegion::Ntsc, 42);
        assert_eq!(state.mode, TastudioMode::Recording);
        state.record_frame(MovieFrame::default());
        state.record_frame(MovieFrame::default());
        assert_eq!(state.movie.as_ref().unwrap().len(), 2);
        assert_eq!(state.cursor, 2);
        state.stop_recording();
        assert_eq!(state.mode, TastudioMode::Playback);
        // Recording while not in Recording mode is a no-op.
        state.record_frame(MovieFrame::default());
        assert_eq!(state.movie.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn edit_frame_overwrites_a_recorded_cell() {
        let mut state = TastudioState::default();
        state.start_recording(1, MovieRegion::Ntsc, 1);
        state.record_frame(MovieFrame::default());
        let mut edited = MovieFrame::default();
        set_swcha_pressed(&mut edited, SWCHA_P0_UP, true);
        state.edit_frame(0, edited);
        assert_eq!(state.movie.as_ref().unwrap().frame_at(0), Some(edited));
    }

    #[test]
    fn jump_to_updates_cursor_and_returns_the_action() {
        let mut state = TastudioState::default();
        let action = state.jump_to(5);
        assert_eq!(state.cursor, 5);
        assert_eq!(action, TastudioAction::JumpToFrame(5));
    }

    #[test]
    fn save_branch_produces_a_decodable_movie() {
        let state = TastudioState::default();
        let system = rusty2600_core::System::new(9);
        let bytes = state.save_branch(0x1234, MovieRegion::Secam, &system);
        let restored = Movie::restore(&bytes, 0x1234).expect("branch bytes must decode");
        assert_eq!(restored.region(), MovieRegion::Secam);
    }
}
