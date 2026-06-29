//! Input mapping — the 2600 controller + console-switch model.
//!
//! This differs substantially from the NES d-pad the RustyNES shell maps. The
//! 2600 surface is:
//!
//! - **Joysticks** — each of the two ports drives four direction bits into
//!   `SWCHA` (RIOT) plus one fire button into the TIA's `INPT4` (port 0) /
//!   `INPT5` (port 1). All are active-LOW on real hardware (pressed = 0).
//! - **Paddles** — analog: two paddles per port, their pots read through the
//!   TIA's `INPT0..=INPT3` dump capacitors, plus a fire button per paddle on
//!   `SWCHA` bits. v0.1 maps an analog axis (gamepad stick / mouse X) to a paddle
//!   position byte.
//! - **Console switches** on `SWCHB` (RIOT): Select, Reset (both momentary),
//!   Color/B&W, and the two Difficulty switches (P0 / P1, each toggling).
//!
//! The frontend collects host input (keyboard + gamepad) into a [`InputState`],
//! which the emu-thread snapshots LATE (just before producing a frame) through
//! the lock-free [`crate::emu_thread::SharedInput`] — late-latched input is part
//! of the determinism / latency story (see RustyNES `docs/frontend.md`).
//!
//! ## Default key map (documented in the README too)
//!
//! Player 1 joystick: Arrow keys = directions, `Z` (or `Space`) = fire.
//! Player 2 joystick: `W/A/S/D` = directions, `Q` = fire.
//! Console switches: `F1` = Select, `F2` = Reset, `F3` = Color/B&W toggle,
//! `F4` = Left (P0) Difficulty toggle, `F5` = Right (P1) Difficulty toggle.
//! System: `Esc` = quit, `` ` `` = toggle debugger, `F12` = open ROM.
//! USB gamepads auto-bind to P1 (D-pad / left stick = directions, South = fire);
//! the left stick X also feeds the P1 paddle position.

/// A single joystick's four directions + fire. Active-low is applied when these
/// are packed into the `SWCHA` / `INPTx` bytes — here `true` means "pressed".
#[derive(Debug, Default, Clone, Copy)]
pub struct Joystick {
    /// Up.
    pub up: bool,
    /// Down.
    pub down: bool,
    /// Left.
    pub left: bool,
    /// Right.
    pub right: bool,
    /// The single fire button (`INPT4` / `INPT5`).
    pub fire: bool,
}

impl Joystick {
    /// Pack the four directions into the high or low nibble of `SWCHA`.
    ///
    /// `SWCHA` layout: bits 7-4 = port 0 (P0) right/left/down/up, bits 3-0 =
    /// port 1 (P1). Active-LOW: a pressed direction drives its bit to 0.
    ///
    /// TODO(T-PS-054): confirm the exact bit order against a real `SWCHA`
    /// read-back test ROM before wiring this into the RIOT port latch.
    #[must_use]
    pub const fn swcha_nibble(self) -> u8 {
        let mut n = 0b1111u8; // idle = all pulled high
        if self.up {
            n &= !0b0001;
        }
        if self.down {
            n &= !0b0010;
        }
        if self.left {
            n &= !0b0100;
        }
        if self.right {
            n &= !0b1000;
        }
        n
    }
}

/// A paddle's analog position (`0..=255`, the dumped-capacitor value the TIA
/// reads through `INPTx`) plus its fire button.
#[derive(Debug, Default, Clone, Copy)]
pub struct Paddle {
    /// Pot position, `0` (fully clockwise) ..= `255` (fully counter-clockwise).
    pub position: u8,
    /// The paddle's fire button (wired to a `SWCHA` bit).
    pub fire: bool,
}

/// The Difficulty switch position (A = "pro"/hard, B = "amateur"/easy).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    /// A position (the "pro" / harder setting). Drives the `SWCHB` bit high.
    A,
    /// B position (the "amateur" / easier setting). Default on most consoles.
    #[default]
    B,
}

impl Difficulty {
    /// Flip A <-> B (the switch is a toggle).
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// The six-switch console panel, read through `SWCHB`.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleSwitches {
    /// Game Select (momentary).
    pub select: bool,
    /// Game Reset (momentary).
    pub reset: bool,
    /// Color (true) vs. B&W (false). A latching toggle.
    pub color: bool,
    /// Left player (P0) difficulty.
    pub left_difficulty: Difficulty,
    /// Right player (P1) difficulty.
    pub right_difficulty: Difficulty,
}

impl Default for ConsoleSwitches {
    fn default() -> Self {
        Self {
            select: false,
            reset: false,
            color: true, // most users run colour
            left_difficulty: Difficulty::default(),
            right_difficulty: Difficulty::default(),
        }
    }
}

impl ConsoleSwitches {
    /// Pack the console switches into the `SWCHB` byte.
    ///
    /// `SWCHB` (active-low for the momentary switches): bit 0 = Reset, bit 1 =
    /// Select, bit 3 = Color/B&W (1 = colour), bit 6 = P0 difficulty (1 = A),
    /// bit 7 = P1 difficulty (1 = A). Bits 2/4/5 read high.
    ///
    /// TODO(T-PS-055): pin against a `SWCHB` read-back test ROM, then route
    /// into the RIOT port-1 latch.
    #[must_use]
    pub const fn swchb(self) -> u8 {
        let mut b = 0b1111_1111u8;
        if self.reset {
            b &= !0b0000_0001;
        }
        if self.select {
            b &= !0b0000_0010;
        }
        if !self.color {
            b &= !0b0000_1000; // B&W drives the bit low
        }
        if matches!(self.left_difficulty, Difficulty::B) {
            b &= !0b0100_0000;
        }
        if matches!(self.right_difficulty, Difficulty::B) {
            b &= !0b1000_0000;
        }
        b
    }
}

/// The complete host-input snapshot the emu-thread latches each frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct InputState {
    /// The two joysticks (port 0, port 1).
    pub joysticks: [Joystick; 2],
    /// Up to four paddles (two per port).
    pub paddles: [Paddle; 4],
    /// The console-switch panel.
    pub switches: ConsoleSwitches,
}

impl InputState {
    /// Build the `(SWCHA, SWCHB)` port bytes the RIOT exposes to the program.
    ///
    /// TODO(T-PS-056): fold the paddle fire buttons into `SWCHA` and honour the
    /// data-direction registers (`SWACNT` / `SWBCNT`) so output bits read back
    /// the last written value.
    #[must_use]
    pub const fn riot_ports(self) -> (u8, u8) {
        let swcha = (self.joysticks[0].swcha_nibble() << 4) | self.joysticks[1].swcha_nibble();
        (swcha, self.switches.swchb())
    }

    /// The two joystick fire buttons as `(INPT4, INPT5)`. Active-low: a pressed
    /// fire reads `0x00`, idle reads `0x80` (the TIA latches bit 7).
    #[must_use]
    pub const fn fire_inputs(self) -> (u8, u8) {
        const fn level(pressed: bool) -> u8 {
            if pressed { 0x00 } else { 0x80 }
        }
        (level(self.joysticks[0].fire), level(self.joysticks[1].fire))
    }
}

/// A bindable host action: which 2600 input a physical key drives. The window
/// handler resolves a live key event to one of these via [`KeyBindings::action_for`]
/// and applies it to the [`InputState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InputAction {
    /// A joystick direction for a player (`0` = P0, `1` = P1).
    JoyUp(u8),
    /// See [`InputAction::JoyUp`].
    JoyDown(u8),
    /// See [`InputAction::JoyUp`].
    JoyLeft(u8),
    /// See [`InputAction::JoyUp`].
    JoyRight(u8),
    /// A joystick fire button for a player.
    JoyFire(u8),
    /// Game Select (momentary).
    SwitchSelect,
    /// Game Reset (momentary).
    SwitchReset,
    /// Color / B&W latching toggle.
    SwitchColor,
    /// Left-player (P0) difficulty toggle.
    SwitchLeftDifficulty,
    /// Right-player (P1) difficulty toggle.
    SwitchRightDifficulty,
}

/// A keyboard binding table: which physical key drives each 2600 input action.
///
/// Keys are stored as the winit `KeyCode` debug name (a string) so the config TOML
/// is human-editable; the window handler converts a live key event to its name via
/// `format!("{code:?}")` and calls [`KeyBindings::action_for`]. The default is the
/// layout documented in the module docs (and the README).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyBindings {
    /// `(KeyCode-name, InputAction)` pairs.
    pub binds: Vec<(String, InputAction)>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        // P1 joystick: arrows + Z fire. P2 joystick: WASD + Q fire.
        // Switches: F1 Select, F2 Reset, F3 Color, F4/F5 Left/Right difficulty.
        let binds = vec![
            ("ArrowUp".into(), InputAction::JoyUp(0)),
            ("ArrowDown".into(), InputAction::JoyDown(0)),
            ("ArrowLeft".into(), InputAction::JoyLeft(0)),
            ("ArrowRight".into(), InputAction::JoyRight(0)),
            ("KeyZ".into(), InputAction::JoyFire(0)),
            ("KeyW".into(), InputAction::JoyUp(1)),
            ("KeyS".into(), InputAction::JoyDown(1)),
            ("KeyA".into(), InputAction::JoyLeft(1)),
            ("KeyD".into(), InputAction::JoyRight(1)),
            ("KeyQ".into(), InputAction::JoyFire(1)),
            ("F1".into(), InputAction::SwitchSelect),
            ("F2".into(), InputAction::SwitchReset),
            ("F3".into(), InputAction::SwitchColor),
            ("F4".into(), InputAction::SwitchLeftDifficulty),
            ("F5".into(), InputAction::SwitchRightDifficulty),
        ];
        Self { binds }
    }
}

impl KeyBindings {
    /// Resolve a winit `KeyCode` debug-name (e.g. `"ArrowUp"`, `"KeyZ"`) to the
    /// action it's bound to, if any.
    #[must_use]
    pub fn action_for(&self, key_name: &str) -> Option<InputAction> {
        self.binds
            .iter()
            .find(|(name, _)| name == key_name)
            .map(|(_, a)| *a)
    }
}

impl InputState {
    /// Apply a momentary / held action to this input snapshot (`pressed` is the key
    /// state). Directions + fire + the momentary Select/Reset switches track the key
    /// edge; the latching toggles (Color, Difficulty) flip only on a press edge.
    pub fn apply_action(&mut self, action: InputAction, pressed: bool) {
        match action {
            InputAction::JoyUp(p) => self.set_joy(p, |j| &mut j.up, pressed),
            InputAction::JoyDown(p) => self.set_joy(p, |j| &mut j.down, pressed),
            InputAction::JoyLeft(p) => self.set_joy(p, |j| &mut j.left, pressed),
            InputAction::JoyRight(p) => self.set_joy(p, |j| &mut j.right, pressed),
            InputAction::JoyFire(p) => self.set_joy(p, |j| &mut j.fire, pressed),
            InputAction::SwitchSelect => self.switches.select = pressed,
            InputAction::SwitchReset => self.switches.reset = pressed,
            InputAction::SwitchColor => {
                if pressed {
                    self.switches.color = !self.switches.color;
                }
            }
            InputAction::SwitchLeftDifficulty => {
                if pressed {
                    self.switches.left_difficulty = self.switches.left_difficulty.toggled();
                }
            }
            InputAction::SwitchRightDifficulty => {
                if pressed {
                    self.switches.right_difficulty = self.switches.right_difficulty.toggled();
                }
            }
        }
    }

    /// Set a joystick bit for player `p` (ignored if out of range).
    fn set_joy(&mut self, p: u8, field: impl Fn(&mut Joystick) -> &mut bool, pressed: bool) {
        if let Some(j) = self.joysticks.get_mut(p as usize) {
            *field(j) = pressed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_binds_cover_both_joysticks_and_switches() {
        let kb = KeyBindings::default();
        assert_eq!(kb.action_for("ArrowUp"), Some(InputAction::JoyUp(0)));
        assert_eq!(kb.action_for("KeyZ"), Some(InputAction::JoyFire(0)));
        assert_eq!(kb.action_for("KeyQ"), Some(InputAction::JoyFire(1)));
        assert_eq!(kb.action_for("F1"), Some(InputAction::SwitchSelect));
        assert_eq!(kb.action_for("Nonexistent"), None);
    }

    #[test]
    fn apply_action_sets_and_toggles() {
        let mut s = InputState::default();
        s.apply_action(InputAction::JoyUp(0), true);
        assert!(s.joysticks[0].up);
        s.apply_action(InputAction::JoyUp(0), false);
        assert!(!s.joysticks[0].up);

        // Color is a latch: it flips on a press edge, not on release.
        assert!(s.switches.color);
        s.apply_action(InputAction::SwitchColor, true);
        assert!(!s.switches.color);
        s.apply_action(InputAction::SwitchColor, false);
        assert!(!s.switches.color, "release must not re-toggle a latch");
    }

    #[test]
    fn idle_ports_pull_high() {
        let s = InputState::default();
        let (swcha, swchb) = s.riot_ports();
        assert_eq!(swcha, 0xFF, "idle joysticks read all-high");
        // Default: colour on (bit 3 high), both difficulties B (bits 6/7 low).
        assert_eq!(swchb & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn pressing_up_clears_its_bit() {
        let mut s = InputState::default();
        s.joysticks[0].up = true;
        let (swcha, _) = s.riot_ports();
        // P0 lives in the high nibble; up is bit 4.
        assert_eq!(swcha & 0b0001_0000, 0);
    }

    #[test]
    fn fire_is_active_low() {
        let mut s = InputState::default();
        assert_eq!(s.fire_inputs(), (0x80, 0x80));
        s.joysticks[0].fire = true;
        assert_eq!(s.fire_inputs().0, 0x00);
    }

    #[test]
    fn difficulty_toggles() {
        assert_eq!(Difficulty::A.toggled(), Difficulty::B);
        assert_eq!(Difficulty::B.toggled(), Difficulty::A);
    }
}
