//! [`ScriptBus`] — the seam a host (`rusty2600-frontend`'s [`crate::ScriptEngine`]
//! integration) implements to let a Lua script observe and (when unlocked)
//! drive the emulator.
//!
//! Deliberately smaller than `RustyNES`'s own PPU/APU-heavy `emu` table: the
//! 2600's entire mutable game-state is 128 B of RIOT RAM plus a handful of
//! memory-mapped TIA/RIOT registers, so there is much less to expose.

/// A read-only snapshot of the 6507's register file, handed to
/// `emu.cpu()`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuSnapshot {
    /// Accumulator.
    pub a: u8,
    /// Index register X.
    pub x: u8,
    /// Index register Y.
    pub y: u8,
    /// Stack pointer.
    pub sp: u8,
    /// Program counter.
    pub pc: u16,
    /// Processor status (`P`), packed `C Z I D B U V N` per `rusty2600-cpu`.
    pub status: u8,
}

/// A joystick direction, as passed to `emu.setJoystick(port, direction,
/// pressed)`. Lua passes these as the lowercase strings below; see
/// [`JoyDirection::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoyDirection {
    /// `"up"`.
    Up,
    /// `"down"`.
    Down,
    /// `"left"`.
    Left,
    /// `"right"`.
    Right,
    /// `"fire"` — the single fire button, not a `SWCHA` direction bit, but
    /// exposed through the same `setJoystick` call for a simpler Lua API.
    Fire,
}

impl JoyDirection {
    /// Parses one of `"up"`/`"down"`/`"left"`/`"right"`/`"fire"`
    /// (case-sensitive, matching the exact strings a script passes).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "fire" => Some(Self::Fire),
            _ => None,
        }
    }
}

/// The host seam a Lua script's `emu` table is backed by.
///
/// `rusty2600-script` itself never touches `rusty2600-core`/
/// `rusty2600-frontend` types directly — a host (the frontend, or a test's
/// mock bus) implements this trait over whatever real state it owns.
pub trait ScriptBus {
    /// Side-effect-free memory read (`emu.peek`) — mirrors
    /// `rusty2600_core::Bus::peek`; always allowed, never gated by
    /// [`crate::WritesLocked`], the same way the debugger's own peek is
    /// unconditionally safe.
    fn peek(&self, addr: u16) -> u8;

    /// Real, side-effecting bus write (`emu.poke`) — mirrors
    /// `rusty2600_core::Bus::cpu_write`. The caller (`ScriptEngine`) checks
    /// [`crate::WritesLocked`] before calling this; a host implementation
    /// does not need to re-check it, but may defensively no-op if it wants
    /// a second line of defense.
    fn poke(&mut self, addr: u16, val: u8);

    /// The 6507's current register file (`emu.cpu()`).
    fn cpu(&self) -> CpuSnapshot;

    /// Drive (or release) one joystick input for `port` (`0` or `1`)
    /// (`emu.setJoystick`). Gated by [`crate::WritesLocked`] the same as
    /// [`Self::poke`] — driving fake input is just as much a
    /// determinism-breaking write as a memory poke.
    fn set_joystick(&mut self, port: u8, direction: JoyDirection, pressed: bool);

    /// Drive one console switch (`emu.setConsoleSwitch`): `name` is one of
    /// `"select"`, `"reset"`, `"color"`, `"left_difficulty"`,
    /// `"right_difficulty"` (the two difficulty switches take `true` = A
    /// / "pro", `false` = B / "amateur", collapsing
    /// `rusty2600_frontend::input::Difficulty` to a bool for a simpler Lua
    /// surface). Returns `false` for an unrecognized name (the script gets
    /// a Lua error, not a silent no-op). Gated by [`crate::WritesLocked`].
    fn set_console_switch(&mut self, name: &str, value: bool) -> bool;

    /// Pause emulation (`emu.pause()`) — wraps whatever pause flag the host
    /// already has (e.g. `EmuCore::paused`).
    fn pause(&mut self);

    /// Capture a save-state (`emu.saveState()`) — wraps the host's existing
    /// `rusty2600_core::SaveState::capture`/`encode`, returned as opaque
    /// bytes a script can stash and hand back to [`Self::load_state`].
    fn save_state(&mut self) -> Vec<u8>;

    /// Restore a save-state (`emu.loadState(bytes)`) — wraps the host's
    /// existing `SaveState::restore`.
    ///
    /// # Errors
    ///
    /// Returns `Err` (surfaced to the script as a Lua error) on a
    /// malformed/mismatched blob rather than silently ignoring it.
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), String>;
}
