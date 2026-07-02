//! [`ScriptEngine`] — the `mlua`-backed Lua VM wrapper exposing the `emu`
//! table to a loaded script.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::Lua;

use crate::bus::{JoyDirection, ScriptBus};
use crate::lock::WritesLocked;
use crate::log::{LogLine, ScriptLog};
use crate::overlay::{Overlay, PixelPrimitive, RectPrimitive, TextPrimitive};

/// A loaded Lua script bound to a [`ScriptBus`] host.
///
/// Generic over `B` rather than `dyn ScriptBus` — a script engine is always
/// bound to exactly one host for its lifetime, so there's no need to pay
/// for dynamic dispatch or a heap-boxed trait object.
pub struct ScriptEngine<B: ScriptBus + 'static> {
    lua: Lua,
    bus: Rc<RefCell<B>>,
    locked: Rc<Cell<WritesLocked>>,
    overlay: Rc<RefCell<Overlay>>,
    on_frame: Rc<RefCell<Option<mlua::RegistryKey>>>,
    log: Rc<RefCell<ScriptLog>>,
}

/// Returns the standard "writes are locked" runtime error for `name`
/// (`"emu.poke"`, `"emu.setJoystick"`, ...), shared by every gated function.
fn locked_error(name: &str) -> mlua::Error {
    mlua::Error::RuntimeError(format!(
        "{name}: writes are locked (RetroAchievements hardcore mode)"
    ))
}

impl<B: ScriptBus + 'static> ScriptEngine<B> {
    fn install_peek(lua: &Lua, emu: &mlua::Table, bus: &Rc<RefCell<B>>) -> mlua::Result<()> {
        let bus = Rc::clone(bus);
        emu.set(
            "peek",
            lua.create_function(move |_, addr: u16| Ok(bus.borrow().peek(addr)))?,
        )
    }

    fn install_poke(
        lua: &Lua,
        emu: &mlua::Table,
        bus: &Rc<RefCell<B>>,
        locked: &Rc<Cell<WritesLocked>>,
    ) -> mlua::Result<()> {
        let bus = Rc::clone(bus);
        let locked = Rc::clone(locked);
        emu.set(
            "poke",
            lua.create_function_mut(move |_, (addr, val): (u16, u8)| {
                if locked.get().locked() {
                    return Err(locked_error("emu.poke"));
                }
                bus.borrow_mut().poke(addr, val);
                Ok(())
            })?,
        )
    }

    fn install_cpu(lua: &Lua, emu: &mlua::Table, bus: &Rc<RefCell<B>>) -> mlua::Result<()> {
        let bus = Rc::clone(bus);
        emu.set(
            "cpu",
            lua.create_function(move |lua, ()| {
                let snap = bus.borrow().cpu();
                let t = lua.create_table()?;
                t.set("a", snap.a)?;
                t.set("x", snap.x)?;
                t.set("y", snap.y)?;
                t.set("sp", snap.sp)?;
                t.set("pc", snap.pc)?;
                t.set("status", snap.status)?;
                Ok(t)
            })?,
        )
    }

    fn install_on_frame(
        lua: &Lua,
        emu: &mlua::Table,
        on_frame: &Rc<RefCell<Option<mlua::RegistryKey>>>,
    ) -> mlua::Result<()> {
        let on_frame = Rc::clone(on_frame);
        emu.set(
            "onFrame",
            lua.create_function_mut(move |lua, f: mlua::Function| {
                let key = lua.create_registry_value(f)?;
                *on_frame.borrow_mut() = Some(key);
                Ok(())
            })?,
        )
    }

    fn install_set_joystick(
        lua: &Lua,
        emu: &mlua::Table,
        bus: &Rc<RefCell<B>>,
        locked: &Rc<Cell<WritesLocked>>,
    ) -> mlua::Result<()> {
        let bus = Rc::clone(bus);
        let locked = Rc::clone(locked);
        emu.set(
            "setJoystick",
            lua.create_function_mut(move |_, (port, direction, pressed): (u8, String, bool)| {
                if locked.get().locked() {
                    return Err(locked_error("emu.setJoystick"));
                }
                let direction = JoyDirection::parse(&direction).ok_or_else(|| {
                    mlua::Error::RuntimeError(format!(
                        "emu.setJoystick: unrecognized direction {direction:?}"
                    ))
                })?;
                bus.borrow_mut().set_joystick(port, direction, pressed);
                Ok(())
            })?,
        )
    }

    fn install_set_console_switch(
        lua: &Lua,
        emu: &mlua::Table,
        bus: &Rc<RefCell<B>>,
        locked: &Rc<Cell<WritesLocked>>,
    ) -> mlua::Result<()> {
        let bus = Rc::clone(bus);
        let locked = Rc::clone(locked);
        emu.set(
            "setConsoleSwitch",
            lua.create_function_mut(move |_, (name, value): (String, bool)| {
                if locked.get().locked() {
                    return Err(locked_error("emu.setConsoleSwitch"));
                }
                if bus.borrow_mut().set_console_switch(&name, value) {
                    Ok(())
                } else {
                    Err(mlua::Error::RuntimeError(format!(
                        "emu.setConsoleSwitch: unrecognized switch name {name:?}"
                    )))
                }
            })?,
        )
    }

    fn install_draw_fns(
        lua: &Lua,
        emu: &mlua::Table,
        overlay: &Rc<RefCell<Overlay>>,
    ) -> mlua::Result<()> {
        {
            let overlay = Rc::clone(overlay);
            emu.set(
                "drawText",
                lua.create_function_mut(move |_, (x, y, text): (i32, i32, String)| {
                    overlay
                        .borrow_mut()
                        .texts
                        .push(TextPrimitive { x, y, text });
                    Ok(())
                })?,
            )?;
        }
        {
            let overlay = Rc::clone(overlay);
            emu.set(
                "drawRect",
                lua.create_function_mut(
                    move |_, (x, y, w, h, color): (i32, i32, i32, i32, u32)| {
                        overlay
                            .borrow_mut()
                            .rects
                            .push(RectPrimitive { x, y, w, h, color });
                        Ok(())
                    },
                )?,
            )?;
        }
        {
            let overlay = Rc::clone(overlay);
            emu.set(
                "drawPixel",
                lua.create_function_mut(move |_, (x, y, color): (i32, i32, u32)| {
                    overlay
                        .borrow_mut()
                        .pixels
                        .push(PixelPrimitive { x, y, color });
                    Ok(())
                })?,
            )
        }
    }

    /// Overrides Lua's default global `print` to route into `log` instead.
    ///
    /// Lua's real `print` would otherwise write to the process's real
    /// stdout, invisible in a GUI app. Matches real Lua `print` semantics:
    /// each argument is stringified via the global `tostring` (so a table
    /// with a `__tostring` metamethod still formats correctly, not just
    /// primitives) and joined with tabs — a script's existing debug
    /// `print`s read exactly the same here as they would on a real stdout.
    fn install_print(lua: &Lua, log: &Rc<RefCell<ScriptLog>>) -> mlua::Result<()> {
        let log = Rc::clone(log);
        let tostring: mlua::Function = lua.globals().get("tostring")?;
        lua.globals().set(
            "print",
            lua.create_function_mut(move |_, args: mlua::Variadic<mlua::Value>| {
                let mut parts = Vec::with_capacity(args.len());
                for arg in args {
                    let s: String = tostring.call(arg)?;
                    parts.push(s);
                }
                log.borrow_mut().push(LogLine::Print(parts.join("\t")));
                Ok(())
            })?,
        )
    }

    /// Appends a runtime-error line to `log`, called by a host after
    /// [`Self::tick_frame`] returns `Err` — see that method's doc.
    pub fn log_error(&self, message: &str) {
        self.log
            .borrow_mut()
            .push(LogLine::Error(message.to_string()));
    }

    /// This script's captured `print()`/error output so far, oldest-first,
    /// for a debugger console panel to render.
    ///
    /// Read-only: unlike [`Self::take_overlay`], the log is a persistent
    /// history the host doesn't drain every frame (see `log.rs`'s module
    /// doc for why).
    #[must_use]
    pub fn log(&self) -> Rc<RefCell<ScriptLog>> {
        Rc::clone(&self.log)
    }

    fn install_pause_and_state(
        lua: &Lua,
        emu: &mlua::Table,
        bus: &Rc<RefCell<B>>,
    ) -> mlua::Result<()> {
        {
            let bus = Rc::clone(bus);
            emu.set(
                "pause",
                lua.create_function_mut(move |_, ()| {
                    bus.borrow_mut().pause();
                    Ok(())
                })?,
            )?;
        }
        {
            let bus = Rc::clone(bus);
            emu.set(
                "saveState",
                lua.create_function_mut(move |lua, ()| {
                    let bytes = bus.borrow_mut().save_state();
                    lua.create_string(&bytes)
                })?,
            )?;
        }
        {
            let bus = Rc::clone(bus);
            emu.set(
                "loadState",
                lua.create_function_mut(move |_, bytes: mlua::String| {
                    bus.borrow_mut()
                        .load_state(&bytes.as_bytes())
                        .map_err(mlua::Error::RuntimeError)
                })?,
            )
        }
    }

    /// Build a fresh Lua VM bound to `bus`, with the `emu` global table
    /// installed.
    ///
    /// # Errors
    ///
    /// Returns an `mlua::Error` if the VM or table setup fails (should only
    /// happen on OOM or a genuinely broken Lua build).
    pub fn new(bus: B) -> mlua::Result<Self> {
        let lua = Lua::new();
        let bus = Rc::new(RefCell::new(bus));
        let locked = Rc::new(Cell::new(WritesLocked::default()));
        let overlay = Rc::new(RefCell::new(Overlay::default()));
        let on_frame: Rc<RefCell<Option<mlua::RegistryKey>>> = Rc::new(RefCell::new(None));
        let log = Rc::new(RefCell::new(ScriptLog::default()));

        let emu = lua.create_table()?;
        Self::install_peek(&lua, &emu, &bus)?;
        Self::install_poke(&lua, &emu, &bus, &locked)?;
        Self::install_cpu(&lua, &emu, &bus)?;
        Self::install_on_frame(&lua, &emu, &on_frame)?;
        Self::install_set_joystick(&lua, &emu, &bus, &locked)?;
        Self::install_set_console_switch(&lua, &emu, &bus, &locked)?;
        Self::install_draw_fns(&lua, &emu, &overlay)?;
        Self::install_pause_and_state(&lua, &emu, &bus)?;
        lua.globals().set("emu", emu)?;
        Self::install_print(&lua, &log)?;

        Ok(Self {
            lua,
            bus,
            locked,
            overlay,
            on_frame,
            log,
        })
    }

    /// Runs `source` as the script body (top-level statements, function
    /// definitions, an `emu.onFrame(...)` registration, etc.).
    ///
    /// # Errors
    ///
    /// Returns an `mlua::Error` on a Lua syntax error or a runtime error
    /// raised while executing the script's top-level statements.
    pub fn load(&self, source: &str) -> mlua::Result<()> {
        self.lua.load(source).exec()
    }

    /// Update the current [`WritesLocked`] state — call once per frame
    /// (or whenever the real lock sources change, e.g. `RetroAchievements`
    /// hardcore mode toggling) BEFORE [`Self::tick_frame`], so any
    /// `emu.poke`/`setJoystick`/`setConsoleSwitch` calls the frame's
    /// `onFrame` callback makes are checked against the current state.
    pub fn set_locked(&self, locked: WritesLocked) {
        self.locked.set(locked);
    }

    /// Invokes the registered `emu.onFrame` callback, if any, once. A host
    /// calls this once per emulated frame, mirroring the same
    /// observe-the-just-finished-frame timing this project's debugger
    /// watch-expression engine already uses (never mid-instruction).
    ///
    /// # Errors
    ///
    /// Returns an `mlua::Error` if the registered callback itself errors.
    ///
    /// A runtime error is also appended to [`Self::log`] as an
    /// [`LogLine::Error`] before being returned, so a debugger console
    /// panel shows it even if the caller only logs the returned `Err` via
    /// `log::warn!` (or ignores it) rather than surfacing it itself.
    pub fn tick_frame(&self) -> mlua::Result<()> {
        let key = self.on_frame.borrow();
        if let Some(key) = key.as_ref() {
            let f: mlua::Function = self.lua.registry_value(key)?;
            if let Err(e) = f.call::<()>(()) {
                self.log_error(&e.to_string());
                return Err(e);
            }
        }
        Ok(())
    }

    /// Takes this frame's accumulated draw primitives, clearing the
    /// internal buffer for the next frame. See [`Overlay`]'s module doc for why
    /// compositing this into the presented frame isn't wired yet.
    #[must_use]
    pub fn take_overlay(&self) -> Overlay {
        let mut overlay = self.overlay.borrow_mut();
        let taken = overlay.clone();
        overlay.clear();
        taken
    }

    /// Direct access to the bound bus (e.g. for a host or test to assert on
    /// its state after running a script).
    #[must_use]
    pub fn bus(&self) -> Rc<RefCell<B>> {
        Rc::clone(&self.bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CpuSnapshot;

    /// A minimal in-memory bus for exercising the `emu` table without any
    /// real chip crate — a 128-entry RAM window, a recorded joystick/switch
    /// log, a pause flag, and a fake save-state ("the byte 0xAB repeated
    /// `len` times") just precise enough to prove `saveState`/`loadState`
    /// round-trip through Lua correctly.
    #[derive(Debug)]
    struct MockBus {
        ram: [u8; 256],
        cpu: CpuSnapshot,
        joystick_log: Vec<(u8, JoyDirection, bool)>,
        switch_log: Vec<(String, bool)>,
        paused: bool,
        saved_len: u8,
    }

    impl Default for MockBus {
        fn default() -> Self {
            Self {
                ram: [0; 256],
                cpu: CpuSnapshot::default(),
                joystick_log: Vec::new(),
                switch_log: Vec::new(),
                paused: false,
                saved_len: 0,
            }
        }
    }

    impl ScriptBus for MockBus {
        fn peek(&self, addr: u16) -> u8 {
            self.ram[usize::from(addr) & 0xFF]
        }
        fn poke(&mut self, addr: u16, val: u8) {
            self.ram[usize::from(addr) & 0xFF] = val;
        }
        fn cpu(&self) -> CpuSnapshot {
            self.cpu
        }
        fn set_joystick(&mut self, port: u8, direction: JoyDirection, pressed: bool) {
            self.joystick_log.push((port, direction, pressed));
        }
        fn set_console_switch(&mut self, name: &str, value: bool) -> bool {
            match name {
                "select" | "reset" | "color" | "left_difficulty" | "right_difficulty" => {
                    self.switch_log.push((name.to_string(), value));
                    true
                }
                _ => false,
            }
        }
        fn pause(&mut self) {
            self.paused = true;
        }
        fn save_state(&mut self) -> Vec<u8> {
            self.saved_len = 4;
            vec![0xAB; 4]
        }
        fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
            if bytes == [0xAB; 4] {
                Ok(())
            } else {
                Err("bad blob".to_string())
            }
        }
    }

    #[test]
    fn peek_reads_the_bus() {
        let mut bus = MockBus::default();
        bus.ram[5] = 0x42;
        let engine = ScriptEngine::new(bus).unwrap();
        engine.load("result = emu.peek(5)").unwrap();
        let result: u8 = engine.lua.globals().get("result").unwrap();
        assert_eq!(result, 0x42);
    }

    #[test]
    fn poke_round_trips_through_peek() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load("emu.poke(9, 200); result = emu.peek(9)")
            .unwrap();
        let result: u8 = engine.lua.globals().get("result").unwrap();
        assert_eq!(result, 200);
    }

    #[test]
    fn poke_is_rejected_when_writes_locked() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.set_locked(WritesLocked {
            ra_hardcore: true,
            ..Default::default()
        });
        let err = engine.load("emu.poke(0, 1)").unwrap_err();
        assert!(err.to_string().contains("locked"));
        // The write must not have gone through.
        assert_eq!(engine.bus().borrow().peek(0), 0);
    }

    #[test]
    fn set_joystick_is_rejected_when_writes_locked() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.set_locked(WritesLocked {
            ra_hardcore: true,
            ..Default::default()
        });
        let err = engine
            .load(r#"emu.setJoystick(0, "up", true)"#)
            .unwrap_err();
        assert!(err.to_string().contains("locked"));
        assert!(engine.bus().borrow().joystick_log.is_empty());
    }

    #[test]
    fn set_joystick_records_direction_and_state() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.load(r#"emu.setJoystick(1, "fire", true)"#).unwrap();
        let bus = engine.bus();
        let log = &bus.borrow().joystick_log;
        assert_eq!(log, &[(1, JoyDirection::Fire, true)]);
    }

    #[test]
    fn set_console_switch_rejects_unknown_name() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        let err = engine
            .load(r#"emu.setConsoleSwitch("not_a_switch", true)"#)
            .unwrap_err();
        assert!(err.to_string().contains("unrecognized"));
    }

    #[test]
    fn set_console_switch_records_known_name() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load(r#"emu.setConsoleSwitch("color", false)"#)
            .unwrap();
        let bus = engine.bus();
        let log = &bus.borrow().switch_log;
        assert_eq!(log, &[("color".to_string(), false)]);
    }

    #[test]
    fn cpu_snapshot_is_exposed_as_a_table() {
        let bus = MockBus {
            cpu: CpuSnapshot {
                a: 1,
                x: 2,
                y: 3,
                sp: 4,
                pc: 0x1234,
                status: 0x24,
            },
            ..MockBus::default()
        };
        let engine = ScriptEngine::new(bus).unwrap();
        engine
            .load("c = emu.cpu(); result = c.a + c.x + c.y")
            .unwrap();
        let result: u8 = engine.lua.globals().get("result").unwrap();
        assert_eq!(result, 6);
        let pc: u16 = engine.lua.load("return (emu.cpu()).pc").eval().unwrap();
        assert_eq!(pc, 0x1234);
    }

    #[test]
    fn on_frame_callback_fires_on_tick() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load("count = 0; emu.onFrame(function() count = count + 1 end)")
            .unwrap();
        engine.tick_frame().unwrap();
        engine.tick_frame().unwrap();
        let count: i64 = engine.lua.globals().get("count").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn tick_frame_without_on_frame_registered_is_a_no_op() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        assert!(engine.tick_frame().is_ok());
    }

    #[test]
    fn pause_reaches_the_bus() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.load("emu.pause()").unwrap();
        assert!(engine.bus().borrow().paused);
    }

    #[test]
    fn save_state_round_trips_through_load_state() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load("blob = emu.saveState(); emu.loadState(blob)")
            .unwrap();
    }

    #[test]
    fn load_state_surfaces_a_bad_blob_as_a_lua_error() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        let err = engine
            .load(r#"emu.loadState("not the right bytes")"#)
            .unwrap_err();
        assert!(err.to_string().contains("bad blob"));
    }

    #[test]
    fn draw_primitives_accumulate_into_the_overlay_and_clear_on_take() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load(
                r#"
                emu.drawText(1, 2, "hi")
                emu.drawRect(0, 0, 10, 10, 0xFF0000)
                emu.drawPixel(5, 5, 0x00FF00)
                "#,
            )
            .unwrap();
        let overlay = engine.take_overlay();
        assert_eq!(overlay.texts.len(), 1);
        assert_eq!(overlay.rects.len(), 1);
        assert_eq!(overlay.pixels.len(), 1);
        assert_eq!(overlay.texts[0].text, "hi");
        assert_eq!(overlay.rects[0].color, 0x00FF_0000);

        // Taking the overlay clears it for the next frame.
        let overlay2 = engine.take_overlay();
        assert!(overlay2.is_empty());
    }

    #[test]
    fn print_is_captured_into_the_log_instead_of_real_stdout() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.load(r#"print("hello", "world")"#).unwrap();
        let log = engine.log();
        let log = log.borrow();
        assert_eq!(log.lines().len(), 1);
        assert!(!log.lines()[0].is_error());
        // Real Lua `print` tab-joins its arguments.
        assert_eq!(log.lines()[0].text(), "hello\tworld");
    }

    #[test]
    fn print_stringifies_non_string_arguments_like_real_lua() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.load("print(42, true, nil)").unwrap();
        let log = engine.log();
        let log = log.borrow();
        assert_eq!(log.lines()[0].text(), "42\ttrue\tnil");
    }

    #[test]
    fn on_frame_error_is_captured_as_an_error_line_and_still_returned() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine
            .load(r#"emu.onFrame(function() error("boom") end)"#)
            .unwrap();
        let err = engine.tick_frame().unwrap_err();
        assert!(err.to_string().contains("boom"));

        let log = engine.log();
        let log = log.borrow();
        assert_eq!(log.lines().len(), 1);
        assert!(log.lines()[0].is_error());
        assert!(log.lines()[0].text().contains("boom"));
    }

    #[test]
    fn log_error_and_print_share_one_ordered_history() {
        let engine = ScriptEngine::new(MockBus::default()).unwrap();
        engine.load(r#"print("before")"#).unwrap();
        engine.log_error("manual error");
        engine.load(r#"print("after")"#).unwrap();

        let log = engine.log();
        let log = log.borrow();
        assert_eq!(log.lines().len(), 3);
        assert_eq!(log.lines()[0].text(), "before");
        assert!(log.lines()[1].is_error());
        assert_eq!(log.lines()[2].text(), "after");
    }
}
