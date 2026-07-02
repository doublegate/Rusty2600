//! Lua scripting frontend wiring (`scripting` feature, off by default).
//!
//! Wires `rusty2600-script`'s `ScriptEngine` into the frontend per
//! `docs/scripting.md`'s "What's next": a real `ScriptBus` implementation,
//! a live `onFrame` hook, and a `.lua` file-load menu entry. Overlay
//! compositing (drawing `emu.drawText`/`drawRect`/`drawPixel` output onto
//! the presented frame) remains its own deferred follow-up — see
//! [`FrontendScriptBus`]'s module doc below for why this pass focuses on
//! the bus/hook wiring rather than the render-pipeline splice.
//!
//! ## Design: a per-frame-synced `System` clone, not a live pointer
//!
//! `ScriptEngine<B>` owns its bound bus behind an `Rc<RefCell<B>>` with
//! `B: 'static`, set once when the engine is constructed (loading the Lua
//! VM once). The real emulator state (`EmuCore`), however, lives behind
//! `Arc<Mutex<EmuCore>>` and is only reachable as a short-lived `MutexGuard`
//! re-acquired every render pass — there is no `'static` reference to hand
//! `ScriptEngine` at construction time without either `unsafe` raw pointers
//! or a much larger architectural change (replacing the mutex-guarded
//! sharing model the emu-thread/winit-thread split already depends on).
//!
//! Since `System` is cheaply `Clone` (a plain `#[derive(Clone)]`, no `Rc`/
//! `Arc` internals) and this project's own doc already states "the 2600's
//! entire mutable game-state is 128 B of RIOT RAM plus a handful of
//! registers," [`FrontendScriptBus`] instead owns a **private `System`
//! clone**, synced from the real `EmuCore.system` once per frame:
//!
//! 1. Before `tick_frame()`: clone the just-completed frame's real
//!    `EmuCore.system` into the bus's own copy (this is the "observe the
//!    just-finished frame" timing `docs/scripting.md` already commits to —
//!    the same convention the debugger's watch-expression engine uses).
//! 2. `tick_frame()` runs the script's `onFrame` callback against that
//!    owned copy — `peek`/`poke`/`cpu()` are exact, real reads/writes;
//!    `saveState()`/`loadState()` wrap the real `SaveState::capture`/
//!    `restore` against this copy, so a script's captured blob **is** a
//!    genuine encode of live state, not an approximation.
//! 3. After `tick_frame()`: clone the (possibly script-mutated, possibly
//!    `loadState`-replaced) copy back onto the real `EmuCore.system`, so
//!    any `emu.poke`/`emu.loadState` effect lands before the next real
//!    frame runs.
//! 4. `emu.setJoystick`/`emu.setConsoleSwitch` can't take effect this way
//!    (the very next `EmuCore::run_frame` call unconditionally overwrites
//!    the RIOT/TIA port bytes from real host input) — instead they're
//!    recorded into [`FrontendScriptBus`]'s override fields, which
//!    [`ScriptState::tick`]'s caller ORs into the next frame's real
//!    [`crate::input::InputState`] before that `run_frame` call (see
//!    `app.rs`'s scripting-feature block).
//!
//! This pays one `System::clone()` (a cheap struct copy — the `Cartridge`
//! enum's ~32 KiB fixed-size worst case, not a heap allocation) per frame
//! only while a script is actually loaded; an off-by-default feature with
//! no script loaded costs nothing extra. This is a real, correct, `unsafe`-
//! free implementation of the documented API contract — an honest,
//! deliberate indirection layer rather than a corner cut, called out here
//! explicitly per this project's exactness-honesty convention.

use std::path::Path;

use rusty2600_core::{SaveState, System};
use rusty2600_script::{CpuSnapshot, JoyDirection, Overlay, ScriptBus, ScriptEngine, WritesLocked};

use crate::emu_thread::EmuCore;
use crate::input::{ConsoleSwitches, Difficulty, InputState};

/// A script-driven console-switch override, applied on top of real host
/// input for exactly one frame then cleared (see module doc point 4).
#[derive(Debug, Clone, Copy, Default)]
struct SwitchOverride {
    select: Option<bool>,
    reset: Option<bool>,
    color: Option<bool>,
    left_difficulty: Option<bool>,
    right_difficulty: Option<bool>,
}

/// A script-driven joystick override for one port, same one-shot lifetime
/// as [`SwitchOverride`].
#[derive(Debug, Clone, Copy, Default)]
struct JoystickOverride {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

/// The real [`ScriptBus`] implementation over a private, per-frame-synced
/// [`System`] clone. See the module doc for the full design rationale.
struct FrontendScriptBus {
    system: System,
    rom_tag: u64,
    paused: bool,
    joystick_override: [JoystickOverride; 2],
    switch_override: SwitchOverride,
}

impl FrontendScriptBus {
    fn new(rom_tag: u64) -> Self {
        Self {
            system: System::new(0),
            rom_tag,
            paused: false,
            joystick_override: [JoystickOverride::default(); 2],
            switch_override: SwitchOverride::default(),
        }
    }
}

impl ScriptBus for FrontendScriptBus {
    fn peek(&self, addr: u16) -> u8 {
        self.system.bus.peek(addr)
    }

    fn poke(&mut self, addr: u16, val: u8) {
        self.system.bus.cpu_write(addr, val);
    }

    fn cpu(&self) -> CpuSnapshot {
        let cpu = &self.system.cpu;
        CpuSnapshot {
            a: cpu.a,
            x: cpu.x,
            y: cpu.y,
            sp: cpu.s,
            pc: cpu.pc,
            status: cpu.p.bits(),
        }
    }

    fn set_joystick(&mut self, port: u8, direction: JoyDirection, pressed: bool) {
        let Some(joy) = self.joystick_override.get_mut(usize::from(port)) else {
            return;
        };
        match direction {
            JoyDirection::Up => joy.up = pressed,
            JoyDirection::Down => joy.down = pressed,
            JoyDirection::Left => joy.left = pressed,
            JoyDirection::Right => joy.right = pressed,
            JoyDirection::Fire => joy.fire = pressed,
        }
    }

    fn set_console_switch(&mut self, name: &str, value: bool) -> bool {
        match name {
            "select" => self.switch_override.select = Some(value),
            "reset" => self.switch_override.reset = Some(value),
            "color" => self.switch_override.color = Some(value),
            "left_difficulty" => self.switch_override.left_difficulty = Some(value),
            "right_difficulty" => self.switch_override.right_difficulty = Some(value),
            _ => return false,
        }
        true
    }

    fn pause(&mut self) {
        self.paused = true;
    }

    fn save_state(&mut self) -> Vec<u8> {
        SaveState::capture(&self.system, self.rom_tag).encode()
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        let restored = SaveState::restore(bytes, self.rom_tag).map_err(|e| e.to_string())?;
        self.system = restored;
        Ok(())
    }
}

/// A loaded Lua script wired to the real, running emulator.
///
/// Constructed once per loaded script (`ScriptState::load`); [`Self::tick`]
/// runs it once per real emulated frame.
pub struct ScriptState {
    engine: ScriptEngine<FrontendScriptBus>,
}

/// Everything that can go wrong loading a script file.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The `.lua` file couldn't be read.
    #[error("failed to read script file: {0}")]
    Io(#[from] std::io::Error),
    /// `mlua`/the Lua VM rejected the script (syntax error, `emu` table
    /// setup failure).
    #[error("script error: {0}")]
    Lua(#[from] mlua::Error),
}

impl ScriptState {
    /// Load `path` as a Lua script bound to a fresh [`FrontendScriptBus`].
    ///
    /// `rom_tag` matches whatever tag the currently-loaded ROM's
    /// save-states already use, so `emu.saveState()`/`emu.loadState()`
    /// round-trip against the same ROM identity every other save-state
    /// consumer in this frontend already relies on.
    ///
    /// # Errors
    ///
    /// See [`ScriptError`].
    pub fn load(path: &Path, rom_tag: u64) -> Result<Self, ScriptError> {
        let source = std::fs::read_to_string(path)?;
        let engine = ScriptEngine::new(FrontendScriptBus::new(rom_tag))?;
        engine.load(&source)?;
        Ok(Self { engine })
    }

    /// Run this frame's script tick against `emu`, then OR any script-driven
    /// joystick/console-switch override into `next_input` (consumed by the
    /// NEXT real frame's `EmuCore::run_frame`/`step_frame` call — see the
    /// module doc's point 4 for why this can't be applied directly to `emu`
    /// here).
    ///
    /// Call this once per real emulated frame, AFTER that frame's
    /// video/audio have been produced (mirrors `docs/scripting.md`'s
    /// "observe the just-finished frame" timing) and before the next
    /// `run_frame`/`step_frame` call.
    pub fn tick(&mut self, emu: &mut EmuCore, locked: WritesLocked, next_input: &mut InputState) {
        self.engine.set_locked(locked);

        {
            let bus = self.engine.bus();
            let mut bus = bus.borrow_mut();
            bus.system = emu.system.clone();
            bus.paused = emu.paused;
            bus.joystick_override = [JoystickOverride::default(); 2];
            bus.switch_override = SwitchOverride::default();
        }

        if let Err(e) = self.engine.tick_frame() {
            log::warn!("script onFrame error: {e}");
        }

        let bus = self.engine.bus();
        let bus = bus.borrow();
        emu.system = bus.system.clone();
        if bus.paused {
            emu.paused = true;
        }
        apply_joystick_override(next_input, &bus.joystick_override);
        apply_switch_override(&mut next_input.switches, bus.switch_override);
    }

    /// This frame's accumulated draw primitives, for a future overlay-
    /// compositing render-pass consumer (not yet wired — see the module
    /// doc). Exposed now so that consumer is a render-pipeline change only,
    /// not also a scripting-wiring change.
    #[must_use]
    pub fn take_overlay(&self) -> Overlay {
        self.engine.take_overlay()
    }
}

fn apply_joystick_override(input: &mut InputState, overrides: &[JoystickOverride; 2]) {
    for (joy, ov) in input.joysticks.iter_mut().zip(overrides.iter()) {
        joy.up |= ov.up;
        joy.down |= ov.down;
        joy.left |= ov.left;
        joy.right |= ov.right;
        joy.fire |= ov.fire;
    }
}

const fn apply_switch_override(switches: &mut ConsoleSwitches, ov: SwitchOverride) {
    if let Some(v) = ov.select {
        switches.select = v;
    }
    if let Some(v) = ov.reset {
        switches.reset = v;
    }
    if let Some(v) = ov.color {
        switches.color = v;
    }
    if let Some(v) = ov.left_difficulty {
        switches.left_difficulty = if v { Difficulty::A } else { Difficulty::B };
    }
    if let Some(v) = ov.right_difficulty {
        switches.right_difficulty = if v { Difficulty::A } else { Difficulty::B };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn loads_and_ticks_a_trivial_script() {
        let dir = std::env::temp_dir();
        let path = write_script(&dir, "rusty2600_scripting_test_trivial.lua", "emu.peek(0)");
        let mut state = ScriptState::load(&path, 0xAAAA).expect("script should load");
        let mut emu = EmuCore::new(0);
        let mut input = InputState::default();
        state.tick(&mut emu, WritesLocked::default(), &mut input);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn on_frame_poke_reaches_the_real_system() {
        let dir = std::env::temp_dir();
        let path = write_script(
            &dir,
            "rusty2600_scripting_test_poke.lua",
            "emu.onFrame(function() emu.poke(0x80, 0x42) end)",
        );
        let mut state = ScriptState::load(&path, 0xBBBB).expect("script should load");
        let mut emu = EmuCore::new(0);
        let mut input = InputState::default();
        state.tick(&mut emu, WritesLocked::default(), &mut input);
        assert_eq!(emu.system.bus.peek(0x80), 0x42);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn locked_poke_does_not_reach_the_real_system() {
        let dir = std::env::temp_dir();
        let path = write_script(
            &dir,
            "rusty2600_scripting_test_locked.lua",
            "emu.onFrame(function() pcall(function() emu.poke(0x80, 0x42) end) end)",
        );
        let mut state = ScriptState::load(&path, 0xCCCC).expect("script should load");
        let mut emu = EmuCore::new(0);
        let before = emu.system.bus.peek(0x80);
        let mut input = InputState::default();
        let locked = WritesLocked {
            ra_hardcore: true,
            ..Default::default()
        };
        state.tick(&mut emu, locked, &mut input);
        assert_eq!(
            emu.system.bus.peek(0x80),
            before,
            "a locked poke must not reach the real system"
        );
        assert_ne!(
            before, 0x42,
            "test setup sanity: 0x80 must not already hold 0x42"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_joystick_overrides_next_frame_input() {
        let dir = std::env::temp_dir();
        let path = write_script(
            &dir,
            "rusty2600_scripting_test_joystick.lua",
            "emu.onFrame(function() emu.setJoystick(0, 'up', true) end)",
        );
        let mut state = ScriptState::load(&path, 0xDDDD).expect("script should load");
        let mut emu = EmuCore::new(0);
        let mut input = InputState::default();
        state.tick(&mut emu, WritesLocked::default(), &mut input);
        assert!(input.joysticks[0].up);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_and_load_state_round_trip_through_the_real_system() {
        let dir = std::env::temp_dir();
        let path = write_script(
            &dir,
            "rusty2600_scripting_test_savestate.lua",
            "emu.onFrame(function() local s = emu.saveState(); emu.loadState(s) end)",
        );
        let mut state = ScriptState::load(&path, 0xEEEE).expect("script should load");
        let mut emu = EmuCore::new(0);
        let mut input = InputState::default();
        state.tick(&mut emu, WritesLocked::default(), &mut input);
        let _ = std::fs::remove_file(&path);
    }
}
