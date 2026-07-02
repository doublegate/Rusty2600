//! The always-on egui shell — menu bar, status bar, tabbed Settings, and the debugger panels.
//!
//! THE NON-NEGOTIABLE RULE (RustyNES `docs/frontend.md`): egui runs **every frame**, drawing the
//! menu bar (File / Emulation / Tools / View / Debug / Help) + the status bar + the tabbed Settings
//! window, with the debugger panels layered on top when visible. The shell NEVER holds the emu lock
//! inside the egui closure. A menu interaction returns a [`MenuAction`]; [`crate::app::App`]
//! dispatches it *after* the egui pass. The render branch copies the display buffer under a brief
//! lock, drops the lock, then renders + presents.
//!
//! The debugger panels are 2600 stubs (6507 / TIA / RIOT / memory) — TODO bodies, not real
//! register read-outs, until the chip models land.

use crate::config::Config;
use crate::palette::Region;

/// An action a menu / shortcut interaction requests. Returned from the egui closure and dispatched
/// by [`crate::app::App`] AFTER the egui pass — so the emu lock is never held across UI work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// File -> Open ROM (the path comes from the native file picker / drag-drop).
    OpenRom,
    /// File -> Close ROM (present a clean blank frame).
    CloseRom,
    /// File -> Save State -> Slot `N` (`0..=7`): capture the running system
    /// into that slot, keyed by the loaded ROM's identity tag. Native-only —
    /// wasm-side persistence is a later release's scope (see `docs/frontend.md`).
    #[cfg(not(target_arch = "wasm32"))]
    SaveStateSlot(u8),
    /// File -> Load State -> Slot `N` (`0..=7`): restore the system from
    /// that slot, if it exists and matches the loaded ROM's identity tag.
    /// Native-only, matching [`MenuAction::SaveStateSlot`].
    #[cfg(not(target_arch = "wasm32"))]
    LoadStateSlot(u8),
    /// Emulation -> Pause / Resume toggle.
    TogglePause,
    /// Emulation -> Reset (the console Reset switch / soft reset).
    Reset,
    /// Emulation -> Power Cycle (re-seed power-on alignment).
    PowerCycle,
    /// Emulation -> Region -> select NTSC / PAL / SECAM.
    SetRegion(Region),
    /// Emulation -> toggle a console switch (Color-B&W / Difficulty).
    ConsoleSwitch(ConsoleSwitchAction),
    /// Debug -> toggle the debugger overlay (the `` ` `` key).
    ToggleDebugger,
    /// Debugger -> execute exactly one CPU instruction.
    #[cfg(feature = "debug-hooks")]
    DebugStep,
    /// Debugger -> run until a breakpoint is hit or a safety step-cap fires.
    #[cfg(feature = "debug-hooks")]
    DebugContinue,
    /// TAStudio -> jump playback to this recorded frame index, via the
    /// existing rewind ring (`crate::debugger::tastudio_panel`).
    #[cfg(feature = "debug-hooks")]
    TastudioJumpToFrame(usize),
    /// TAStudio -> save a branch point (`.r26m`) to disk.
    #[cfg(feature = "debug-hooks")]
    TastudioSaveBranch,
    /// Emulation -> RetroAchievements -> toggle hardcore mode.
    #[cfg(feature = "retroachievements")]
    ToggleHardcore,
    /// Tools -> Load Script... (the path comes from the native file picker).
    #[cfg(feature = "scripting")]
    LoadScript,
    /// Tools -> Unload Script.
    #[cfg(feature = "scripting")]
    UnloadScript,
    /// Netplay dialog -> Connect: bind `local_port` and start synchronizing
    /// with `remote_addr` (direct-IP/LAN only — see `docs/netplay.md`). The
    /// underlying `RollbackSession::new` API is symmetric — both peers
    /// supply each other's address out-of-band, there is no separate
    /// "host and wait for any connector" server mode.
    #[cfg(feature = "netplay")]
    NetplayConnect {
        /// The local UDP port to bind.
        local_port: u16,
        /// The peer's address.
        remote_addr: std::net::SocketAddr,
    },
    /// Netplay dialog -> Connect via STUN (`[2.3.0]`): bind `local_port`,
    /// discover this machine's own public address via a real STUN query,
    /// best-effort hole-punch toward the peer's own already-discovered
    /// public address (`remote_public_addr`, exchanged out-of-band the
    /// same way `NetplayConnect`'s address is), then start synchronizing.
    /// See `rusty2600-netplay::stun`'s module doc for exactly what is and
    /// isn't verified about real NAT traversal.
    #[cfg(feature = "netplay")]
    NetplayConnectStun {
        /// The local UDP port to bind.
        local_port: u16,
        /// The peer's own STUN-discovered public address.
        remote_public_addr: std::net::SocketAddr,
    },
    /// Tools -> Disconnect Netplay.
    #[cfg(feature = "netplay")]
    NetplayDisconnect,
    /// View -> toggle fullscreen.
    ToggleFullscreen,
    /// File -> open the Settings window.
    OpenSettings,
    /// Help -> open the in-app Documentation pane.
    OpenDocs,
    /// File -> Quit.
    Quit,
    /// A Settings-window widget changed this frame — persist `cfg` to disk.
    /// Native-only in effect (the wasm build has no filesystem config path;
    /// [`crate::config::Config::save`] simply doesn't exist there), but kept
    /// as a plain action here since [`ShellState::render`] is shared code and
    /// must never call platform-specific I/O directly.
    SaveConfig,
}

/// The console-switch menu actions (the 2600-specific panel the NES shell lacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleSwitchAction {
    /// Toggle Color vs. B&W.
    ToggleColor,
    /// Toggle the left-player (P0) difficulty A/B.
    ToggleLeftDifficulty,
    /// Toggle the right-player (P1) difficulty A/B.
    ToggleRightDifficulty,
}

/// One manual save-state slot's on-disk status.
///
/// Probed fresh each frame from the filesystem (cheap `stat` calls, no emu
/// lock needed — see [`ShellInfo::save_slots`]'s doc comment) so the File
/// menu can show real per-slot info without ever touching the emulator.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy)]
pub struct SaveSlotInfo {
    /// The slot index (`0..SAVE_SLOT_COUNT`).
    pub slot: u8,
    /// Whether a save file exists in this slot for the currently-loaded ROM.
    pub exists: bool,
    /// The slot file's last-modified time, if it exists.
    pub modified: Option<std::time::SystemTime>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SaveSlotInfo {
    /// Probes the filesystem for `slot`'s current status under `rom_tag`.
    #[must_use]
    pub fn probe(rom_tag: u64, slot: u8) -> Self {
        let metadata = crate::config::save_slot_path(rom_tag, slot).and_then(|p| p.metadata().ok());
        Self {
            slot,
            exists: metadata.is_some(),
            modified: metadata.and_then(|m| m.modified().ok()),
        }
    }

    /// A human-readable menu label, e.g. `"Slot 3 (empty)"` or a slot with a
    /// real save shown as `"Slot 3 -- 2026-07-02 14:03:07 UTC"`. Formats the
    /// timestamp by hand (no `chrono`/`time` dependency in this crate today)
    /// via `SystemTime`'s Unix-epoch offset, good enough for a menu label.
    #[must_use]
    pub fn label(&self) -> String {
        self.modified.map_or_else(
            || format!("Slot {} (empty)", self.slot),
            |t| {
                let secs = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                format!("Slot {} -- {}", self.slot, format_unix_timestamp(secs))
            },
        )
    }
}

/// Formats a Unix timestamp as `YYYY-MM-DD HH:MM:SS UTC` using only
/// `core`/`std` civil-calendar arithmetic (Howard Hinnant's `civil_from_days`
/// algorithm, public domain) — avoids pulling `chrono`/`time` into this
/// crate purely for a save-slot menu label.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
fn format_unix_timestamp(secs: u64) -> String {
    let days = (secs / 86_400).cast_signed();
    let time_of_day = secs % 86_400;
    let (hour, minute, second) = (
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    );

    // civil_from_days (Hinnant): days since 1970-01-01 -> (year, month, day).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned(); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe.cast_signed() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// Which debugger panel is selected in the overlay (2600 chip set).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebugPanel {
    /// 6507 CPU registers + disassembly.
    #[default]
    Cpu,
    /// TIA (object registers + beam position + collision latches).
    Tia,
    /// RIOT (the interval timer + the I/O ports).
    Riot,
    /// The memory map (the RIOT's 128 bytes + the cart window view).
    Memory,
    /// Watch expressions + conditional breakpoints (`crate::debugger::expr`).
    Watch,
    /// The live JSR/RTS call stack.
    Callstack,
    /// A per-scanline TIA register write scatter.
    Events,
    /// Live player/missile/ball position + registers.
    Pmb,
    /// TAStudio-lite: piano-roll movie recording/editing (`tastudio_panel`).
    Tastudio,
    /// Per-address write-count heatmap (`access_counter`).
    AccessCounter,
    /// Byte-by-byte memory-snapshot diff (`memory_compare_panel`).
    MemoryCompare,
    /// RetroAchievements: login, achievement/leaderboard lists, rich
    /// presence, recent unlocks (`T-0802-005`).
    #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
    Cheevos,
    /// A loaded Lua script's captured `print()`/error output
    /// (`lua_console_panel`, `[2.5.0]`).
    #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
    LuaConsole,
}

/// Persistent shell UI toggles (which panels are open, theme, status). Separate from the emulator
/// so the shell renders even with no ROM and the emu lock is never taken to draw it.
#[derive(Debug, Default, Clone)]
pub struct ShellState {
    /// Whether the debugger overlay is visible (`` ` `` toggles it).
    pub debugger_visible: bool,
    /// Whether the Settings window is open.
    pub settings_open: bool,
    /// The selected debugger panel.
    pub panel: DebugPanel,
    /// The selected Settings tab index.
    pub settings_tab: usize,
    /// A transient status-bar message (e.g. "Loaded `<game>`", "ROM closed").
    pub status: String,
    /// Whether emulation is paused (mirrored from the app for the menu checkmark).
    pub paused: bool,
    /// Which debugger panels are open (per-chip toggles).
    pub panels: PanelVisibility,
    /// Breakpoints, the memory-viewer cursor, and other persistent debugger
    /// UI state (`crate::debugger::DebuggerState`).
    #[cfg(feature = "debug-hooks")]
    pub debugger: crate::debugger::DebuggerState,
    /// Whether the Host/Join Netplay dialog is open.
    #[cfg(feature = "netplay")]
    pub netplay_dialog_open: bool,
    /// The local UDP port text field in the Netplay dialog.
    #[cfg(feature = "netplay")]
    pub netplay_local_port: String,
    /// The remote peer address (`ip:port`) text field in the Netplay dialog.
    #[cfg(feature = "netplay")]
    pub netplay_remote_addr: String,
    /// The cached save-slot statuses shown in the File -> Save State / Load
    /// State submenus, and the `rom_tag` they were probed against.
    ///
    /// Probing is 8 filesystem `stat` calls (see [`SaveSlotInfo::probe`]) —
    /// re-running that every single frame at 60+ FPS would be needless
    /// blocking I/O on the render path. Instead this cache is only
    /// refreshed when the loaded ROM changes (`rom_tag` differs from the
    /// cached one) or [`Self::save_slots_dirty`] is set (after a
    /// save/load-slot action actually changes what's on disk) — see
    /// `app.rs`'s frame-info population and its `SaveStateSlot`/
    /// `LoadStateSlot` dispatch arms.
    #[cfg(not(target_arch = "wasm32"))]
    pub save_slots_cache: Vec<SaveSlotInfo>,
    /// See [`Self::save_slots_cache`].
    #[cfg(not(target_arch = "wasm32"))]
    pub save_slots_cache_rom_tag: Option<u64>,
    /// Forces a re-probe of [`Self::save_slots_cache`] on the next frame
    /// regardless of `rom_tag`, set after a save/load-slot action so the
    /// menu reflects the change immediately rather than on the next ROM
    /// load. See [`Self::save_slots_cache`].
    #[cfg(not(target_arch = "wasm32"))]
    pub save_slots_dirty: bool,
}

impl ShellState {
    /// Whether the locked render branch must be taken: true when the overlay is visible (mirrors
    /// the RustyNES rule that the brief-lock branch runs only when something reads emu state).
    #[must_use]
    pub const fn needs_emu_read(&self) -> bool {
        self.debugger_visible
    }
}

/// Read-only facts the shell needs to render the status bar + window title without taking the emu
/// lock (the app copies these out under the brief lock, then renders).
#[derive(Debug, Clone, Default)]
pub struct ShellInfo {
    /// The loaded board's tier label, if any (the 2600 `Board` trait has no name; `Tier::name`
    /// is the honesty marker shown here).
    pub board_tier: Option<String>,
    /// The current region.
    pub region: Region,
    /// The measured frames-per-second (the pacer's smoothed estimate).
    pub fps: f32,
    /// Whether a ROM is loaded.
    pub rom_loaded: bool,
    /// The live debugger snapshot (registers/TIA/RIOT/disassembly), built
    /// under the brief emu lock only while the debugger overlay is open.
    /// `None` when the overlay is closed (no point paying the copy cost) or
    /// no ROM is loaded.
    #[cfg(feature = "debug-hooks")]
    pub debug: Option<crate::debugger::DebugSnapshot>,
    /// Whether RetroAchievements hardcore mode is currently enabled.
    #[cfg(feature = "retroachievements")]
    pub cheevos_hardcore: bool,
    /// Whether RetroAchievements has successfully identified the loaded ROM.
    #[cfg(feature = "retroachievements")]
    pub cheevos_game_loaded: bool,
    /// Whether a Lua script is currently loaded.
    #[cfg(feature = "scripting")]
    pub script_loaded: bool,
    /// This frame's accumulated `emu.drawText`/`drawRect`/`drawPixel` calls,
    /// drained from the script engine under the same brief emu lock `debug`/
    /// `cheevos_hardcore` already use. Composited by `app.rs`'s render pass
    /// (`draw_script_overlay`), not by `ShellState::render` itself — the
    /// script overlay is a distinct concern from the menu/status-bar chrome
    /// this module owns.
    #[cfg(feature = "scripting")]
    pub overlay: rusty2600_script::Overlay,
    /// Whether a rollback netplay session is currently active.
    #[cfg(feature = "netplay")]
    pub netplay_active: bool,
    /// This frame's save-state slot status for the loaded ROM (empty if no
    /// ROM is loaded), used by the File -> Save State / Load State
    /// submenus. Probed fresh each frame by `app.rs` via
    /// [`SaveSlotInfo::probe`] AFTER the brief emu lock is dropped — the
    /// probe is pure filesystem `stat` calls (keyed by the `rom_tag` copied
    /// out under the lock), never touching the emulator itself, matching
    /// this struct's own "read-only facts copied under the emu lock" rule
    /// for its emu-derived fields while staying lock-free for this one.
    /// Native-only.
    #[cfg(not(target_arch = "wasm32"))]
    pub save_slots: Vec<SaveSlotInfo>,
}

/// Which debugger panels are currently shown.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanelVisibility {
    /// The 6507 CPU register panel.
    pub cpu: bool,
    /// The TIA panel (object registers + beam position).
    pub tia: bool,
    /// The RIOT panel (timer + I/O ports).
    pub riot: bool,
    /// The memory panel (the RIOT's 128 bytes + the cart window view).
    pub memory: bool,
}

impl ShellState {
    /// Render the always-on shell (menu bar + status bar + the optional Settings/debugger windows)
    /// and collect any requested [`MenuAction`]s. Returns the actions for the app to dispatch AFTER
    /// this pass — this function NEVER touches the emulator.
    ///
    /// Uses the egui 0.34 panel API: the caller passes the root `Ui` from `Context::run_ui`, into
    /// which the top/bottom panels are nested with `show_inside`.
    // One straight-line immediate-mode egui pass (menu bar + status bar + windows); the line count
    // is inherent to the panel layout and reads more clearly as a unit than split apart.
    #[allow(clippy::too_many_lines)]
    pub fn render(
        &mut self,
        root_ui: &mut egui::Ui,
        info: &ShellInfo,
        cfg: &mut Config,
        #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
        cheevos: &mut crate::cheevos::CheevosState,
        #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))] script: Option<
            &crate::scripting::ScriptState,
        >,
    ) -> Vec<MenuAction> {
        let mut actions = Vec::new();
        let ctx = root_ui.ctx().clone();

        egui::Panel::top("menu_bar").show_inside(root_ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open ROM...").clicked() {
                        actions.push(MenuAction::OpenRom);
                        ui.close();
                    }
                    if ui
                        .add_enabled(info.rom_loaded, egui::Button::new("Close ROM"))
                        .clicked()
                    {
                        actions.push(MenuAction::CloseRom);
                        ui.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.separator();
                        ui.add_enabled_ui(info.rom_loaded, |ui| {
                            ui.menu_button("Save State", |ui| {
                                for slot in &info.save_slots {
                                    if ui.button(slot.label()).clicked() {
                                        actions.push(MenuAction::SaveStateSlot(slot.slot));
                                        ui.close();
                                    }
                                }
                            });
                            ui.menu_button("Load State", |ui| {
                                for slot in &info.save_slots {
                                    if ui
                                        .add_enabled(slot.exists, egui::Button::new(slot.label()))
                                        .clicked()
                                    {
                                        actions.push(MenuAction::LoadStateSlot(slot.slot));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
                    ui.separator();
                    if ui.button("Settings...").clicked() {
                        self.settings_open = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        actions.push(MenuAction::Quit);
                        ui.close();
                    }
                });

                ui.menu_button("Emulation", |ui| {
                    let pause_label = if self.paused { "Resume" } else { "Pause" };
                    if ui
                        .add_enabled(info.rom_loaded, egui::Button::new(pause_label))
                        .clicked()
                    {
                        actions.push(MenuAction::TogglePause);
                        ui.close();
                    }
                    if ui
                        .add_enabled(info.rom_loaded, egui::Button::new("Reset"))
                        .clicked()
                    {
                        actions.push(MenuAction::Reset);
                        ui.close();
                    }
                    if ui
                        .add_enabled(info.rom_loaded, egui::Button::new("Power Cycle"))
                        .clicked()
                    {
                        actions.push(MenuAction::PowerCycle);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Region", |ui| {
                        for (region, label) in [
                            (Region::Ntsc, "NTSC"),
                            (Region::Pal, "PAL"),
                            (Region::Secam, "SECAM"),
                        ] {
                            if ui.radio(info.region == region, label).clicked() {
                                actions.push(MenuAction::SetRegion(region));
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button("Console switches", |ui| {
                        if ui.button("Toggle Color / B&W").clicked() {
                            actions
                                .push(MenuAction::ConsoleSwitch(ConsoleSwitchAction::ToggleColor));
                            ui.close();
                        }
                        if ui.button("Toggle Left Difficulty").clicked() {
                            actions.push(MenuAction::ConsoleSwitch(
                                ConsoleSwitchAction::ToggleLeftDifficulty,
                            ));
                            ui.close();
                        }
                        if ui.button("Toggle Right Difficulty").clicked() {
                            actions.push(MenuAction::ConsoleSwitch(
                                ConsoleSwitchAction::ToggleRightDifficulty,
                            ));
                            ui.close();
                        }
                    });
                    #[cfg(feature = "retroachievements")]
                    {
                        ui.separator();
                        ui.menu_button("RetroAchievements", |ui| {
                            let label = if info.cheevos_game_loaded {
                                "Game recognized"
                            } else {
                                "No game recognized"
                            };
                            ui.label(label);
                            let mut hardcore = info.cheevos_hardcore;
                            if ui.checkbox(&mut hardcore, "Hardcore mode").changed() {
                                actions.push(MenuAction::ToggleHardcore);
                                ui.close();
                            }
                        });
                    }
                });

                ui.menu_button("Tools", |ui| {
                    // TODO(impl-phase): TIA audio scope, cheat editor, ROM-DB editor, TAStudio.
                    #[cfg(feature = "scripting")]
                    {
                        if info.script_loaded {
                            if ui.button("Unload Script").clicked() {
                                actions.push(MenuAction::UnloadScript);
                                ui.close();
                            }
                        } else if ui.button("Load Script...").clicked() {
                            actions.push(MenuAction::LoadScript);
                            ui.close();
                        }
                    }
                    #[cfg(feature = "netplay")]
                    {
                        if info.netplay_active {
                            if ui.button("Disconnect Netplay").clicked() {
                                actions.push(MenuAction::NetplayDisconnect);
                                ui.close();
                            }
                        } else if ui.button("Netplay...").clicked() {
                            self.netplay_dialog_open = true;
                            ui.close();
                        }
                    }
                    #[cfg(not(any(feature = "scripting", feature = "netplay")))]
                    ui.label("(tools — TODO)");
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut cfg.video.integer_scale, "Integer scale");
                    if ui.button("Toggle fullscreen").clicked() {
                        actions.push(MenuAction::ToggleFullscreen);
                        ui.close();
                    }
                    // TODO(impl-phase): shader/filter picklist, per-side overscan.
                });

                ui.menu_button("Debug", |ui| {
                    if ui
                        .checkbox(&mut self.debugger_visible, "Debugger overlay")
                        .clicked()
                    {
                        ui.close();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Documentation").clicked() {
                        actions.push(MenuAction::OpenDocs);
                        ui.close();
                    }
                    ui.label("Rusty2600 v0.1.0 (scaffold)");
                });
            });
        });

        egui::Panel::bottom("status_bar").show_inside(root_ui, |ui| {
            ui.horizontal(|ui| {
                let title = info.board_tier.as_deref().unwrap_or(if info.rom_loaded {
                    "<unknown board>"
                } else {
                    "no ROM"
                });
                ui.label(title);
                ui.separator();
                ui.label(info.region.label());
                ui.separator();
                ui.label(format!("{:.1} fps", info.fps));
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
            });
        });

        // The Settings + debugger windows float above the panels (rendered on the same ctx).
        if self.settings_open {
            self.render_settings(&ctx, cfg, &mut actions);
        }
        if self.debugger_visible {
            self.render_debugger(
                &ctx,
                info,
                &mut actions,
                #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
                cheevos,
                #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
                script,
            );
        }
        #[cfg(feature = "netplay")]
        if self.netplay_dialog_open {
            self.render_netplay_dialog(&ctx, &mut actions);
        }

        actions
    }

    /// The Host/Join Netplay dialog (`Tools -> Netplay...`).
    ///
    /// Direct-IP/LAN only, matching `rusty2600-netplay`'s `[1.10.0]` scope
    /// (see `docs/netplay.md`) — Host binds `local_port` and waits; Join
    /// connects to `remote_addr` from its own `local_port`. Malformed
    /// input (a non-numeric port, an unparsable address) shows an inline
    /// error rather than silently doing nothing.
    #[cfg(feature = "netplay")]
    fn render_netplay_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<MenuAction>) {
        let mut open = self.netplay_dialog_open;
        let mut error: Option<String> = None;
        let mut submitted = false;
        egui::Window::new("Netplay")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Local port:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netplay_local_port)
                            .hint_text("9000")
                            .desired_width(80.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Remote address:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.netplay_remote_addr)
                            .hint_text("192.168.1.10:9000")
                            .desired_width(160.0),
                    );
                });
                ui.label(
                    "Both players exchange addresses out-of-band, then each enters \
                     the other's address here — there's no separate host/join step. \
                     \"Connect\" uses the address directly (LAN/port-forwarded); \
                     \"Connect via STUN\" discovers your own public address first \
                     (shown in the status bar once found) and hole-punches toward \
                     the peer's.",
                );
                ui.horizontal(|ui| {
                    let parsed_port = self.netplay_local_port.trim().parse::<u16>();
                    if ui.button("Connect").clicked() {
                        let remote = self.netplay_remote_addr.trim().parse();
                        match (parsed_port, remote) {
                            (Ok(local_port), Ok(remote_addr)) => {
                                actions.push(MenuAction::NetplayConnect {
                                    local_port,
                                    remote_addr,
                                });
                                submitted = true;
                            }
                            (Err(_), _) => error = Some("local port must be 0-65535".into()),
                            (_, Err(_)) => {
                                error = Some("remote address must be ip:port".into());
                            }
                        }
                    }
                    if ui
                        .button("Connect via STUN")
                        .on_hover_text(
                            "Discovers your own public address via STUN, hole-punches \
                             toward the peer's already-discovered address (typed above \
                             as the remote address), then connects. Real traversal \
                             through both peers' NATs is not guaranteed to succeed.",
                        )
                        .clicked()
                    {
                        let parsed_port = self.netplay_local_port.trim().parse::<u16>();
                        let remote = self.netplay_remote_addr.trim().parse();
                        match (parsed_port, remote) {
                            (Ok(local_port), Ok(remote_public_addr)) => {
                                actions.push(MenuAction::NetplayConnectStun {
                                    local_port,
                                    remote_public_addr,
                                });
                                submitted = true;
                            }
                            (Err(_), _) => error = Some("local port must be 0-65535".into()),
                            (_, Err(_)) => {
                                error = Some("peer's public address must be ip:port".into());
                            }
                        }
                    }
                });
                if let Some(e) = &error {
                    ui.colored_label(egui::Color32::RED, e);
                }
            });
        // `open` reflects the window's own close (X) button; a successful Host/Join
        // submission closes the dialog too, but a validation error keeps it open even
        // though neither button click flipped `open` to false.
        self.netplay_dialog_open = open && !submitted;
    }

    /// The tabbed Settings window (Video / Audio / Input / System). v0.1 wires the live config
    /// fields; deep per-knob panels (NTSC, shader stack, per-game overrides) are TODO.
    ///
    /// Every widget here edits `cfg` in place immediately (so the change takes effect this
    /// frame), but a live edit alone never reaches disk — [`crate::config::Config::save`] is
    /// native-only I/O this shared module must not call directly. Instead, any widget that
    /// reports `.changed()` pushes [`MenuAction::SaveConfig`] so the native-only app layer
    /// persists it once, after this render pass (the previous behavior only ever saved on the
    /// top menu bar's Region submenu, so every OTHER Settings-window change silently never
    /// stuck between sessions).
    fn render_settings(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut Config,
        actions: &mut Vec<MenuAction>,
    ) {
        let mut open = self.settings_open;
        let mut changed = false;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, name) in ["Video", "Audio", "Input", "System"].iter().enumerate() {
                        ui.selectable_value(&mut self.settings_tab, i, *name);
                    }
                });
                ui.separator();
                match self.settings_tab {
                    0 => {
                        ui.label("Present mode:");
                        for m in ["fifo", "mailbox", "immediate"] {
                            if ui.radio(cfg.video.present_mode == m, m).clicked() {
                                cfg.video.present_mode = m.to_string();
                                changed = true;
                            }
                        }
                        if ui
                            .checkbox(&mut cfg.video.integer_scale, "Integer scale")
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut cfg.video.runahead_frames, 0..=4)
                                    .text("Run-ahead frames"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.separator();
                        ui.label("Shader stack (empty = off, byte-identical default):");
                        for kind in [
                            rusty2600_gfx_shaders::PassKind::CompositeArtifact,
                            rusty2600_gfx_shaders::PassKind::CrtScanline,
                        ] {
                            let mut enabled = cfg.video.shader_passes.contains(&kind);
                            if ui.checkbox(&mut enabled, kind.label()).changed() {
                                if enabled {
                                    cfg.video.shader_passes.push(kind);
                                } else {
                                    cfg.video.shader_passes.retain(|p| *p != kind);
                                }
                                changed = true;
                            }
                        }
                    }
                    1 => {
                        if ui
                            .checkbox(&mut cfg.audio.enabled, "Audio enabled")
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(egui::Slider::new(&mut cfg.audio.volume, 0.0..=1.0).text("Volume"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                    2 => {
                        // TODO(impl-phase): the 2600 key-rebind grid (joystick * 2 players + the
                        // console-switch row).
                        ui.label("Input rebinding — TODO (defaults in input.rs).");
                    }
                    _ => {
                        ui.label("Region:");
                        if ui
                            .radio_value(&mut cfg.region, Region::Ntsc, "NTSC")
                            .changed()
                            || ui
                                .radio_value(&mut cfg.region, Region::Pal, "PAL")
                                .changed()
                            || ui
                                .radio_value(&mut cfg.region, Region::Secam, "SECAM")
                                .changed()
                        {
                            changed = true;
                        }
                    }
                }
            });
        self.settings_open = open;
        if changed {
            actions.push(MenuAction::SaveConfig);
        }
    }

    /// The debugger overlay: a panel selector + the live 6507/TIA/RIOT/memory
    /// panels (`crate::debugger`, behind the `debug-hooks` feature).
    #[allow(clippy::too_many_lines)]
    fn render_debugger(
        &mut self,
        ctx: &egui::Context,
        info: &ShellInfo,
        #[cfg_attr(not(feature = "debug-hooks"), allow(unused_variables))] actions: &mut Vec<
            MenuAction,
        >,
        #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
        cheevos: &mut crate::cheevos::CheevosState,
        #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))] script: Option<
            &crate::scripting::ScriptState,
        >,
    ) {
        let mut open = self.debugger_visible;
        egui::Window::new("Debugger")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.panel, DebugPanel::Cpu, "6507");
                    ui.selectable_value(&mut self.panel, DebugPanel::Tia, "TIA");
                    ui.selectable_value(&mut self.panel, DebugPanel::Riot, "RIOT");
                    ui.selectable_value(&mut self.panel, DebugPanel::Memory, "Memory");
                    ui.selectable_value(&mut self.panel, DebugPanel::Watch, "Watch");
                    ui.selectable_value(&mut self.panel, DebugPanel::Callstack, "Callstack");
                    ui.selectable_value(&mut self.panel, DebugPanel::Events, "Events");
                    ui.selectable_value(&mut self.panel, DebugPanel::Pmb, "P/M/B");
                    ui.selectable_value(&mut self.panel, DebugPanel::Tastudio, "TAStudio");
                    ui.selectable_value(&mut self.panel, DebugPanel::AccessCounter, "Access");
                    ui.selectable_value(&mut self.panel, DebugPanel::MemoryCompare, "Compare");
                    #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
                    ui.selectable_value(&mut self.panel, DebugPanel::Cheevos, "Cheevos");
                    #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
                    ui.selectable_value(&mut self.panel, DebugPanel::LuaConsole, "Lua Console");
                });
                ui.separator();

                #[cfg(feature = "debug-hooks")]
                {
                    let Some(snap) = info.debug.as_ref() else {
                        ui.label("(no ROM loaded)");
                        return;
                    };
                    let mut debug_actions = Vec::new();
                    match self.panel {
                        DebugPanel::Cpu => crate::debugger::render_cpu_panel(
                            ui,
                            &snap.cpu,
                            &snap.disassembly_at_pc,
                            &mut self.debugger,
                            &mut debug_actions,
                        ),
                        DebugPanel::Tia => crate::debugger::render_tia_panel(ui, &snap.tia),
                        DebugPanel::Riot => crate::debugger::render_riot_panel(ui, &snap.riot),
                        DebugPanel::Memory => crate::debugger::render_memory_panel(
                            ui,
                            &mut self.debugger,
                            &snap.memory_view,
                        ),
                        DebugPanel::Watch => crate::debugger::watch_panel::render_watch_panel(
                            ui,
                            snap,
                            &mut self.debugger,
                        ),
                        DebugPanel::Callstack => {
                            crate::debugger::callstack::render_callstack_panel(
                                ui,
                                &self.debugger.call_stack,
                            );
                        }
                        DebugPanel::Events => {
                            crate::debugger::event_panel::render_event_panel(ui, &snap.tia_writes);
                        }
                        DebugPanel::Pmb => {
                            crate::debugger::pmb_panel::render_pmb_panel(ui, &snap.tia);
                        }
                        DebugPanel::Tastudio => {
                            let tastudio_actions =
                                crate::debugger::tastudio_panel::render_tastudio_panel(
                                    ui,
                                    &mut self.debugger.tastudio,
                                );
                            for action in tastudio_actions {
                                actions.push(match action {
                                    crate::debugger::tastudio_panel::TastudioAction::JumpToFrame(
                                        idx,
                                    ) => MenuAction::TastudioJumpToFrame(idx),
                                    crate::debugger::tastudio_panel::TastudioAction::SaveBranch => {
                                        MenuAction::TastudioSaveBranch
                                    }
                                });
                            }
                        }
                        DebugPanel::AccessCounter => {
                            crate::debugger::access_counter::render_access_counter_panel(
                                ui,
                                &snap.tia_writes,
                            );
                        }
                        DebugPanel::MemoryCompare => {
                            crate::debugger::memory_compare_panel::render_memory_compare_panel(
                                ui,
                                &mut self.debugger.memory_compare_baseline,
                                &snap.riot_ram,
                            );
                        }
                        #[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
                        DebugPanel::Cheevos => {
                            crate::debugger::cheevos_panel::render_cheevos_panel(ui, cheevos);
                        }
                        #[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
                        DebugPanel::LuaConsole => {
                            crate::debugger::lua_console_panel::render_lua_console_panel(
                                ui, script,
                            );
                        }
                    }
                    for action in debug_actions {
                        actions.push(match action {
                            crate::debugger::DebugAction::Step => MenuAction::DebugStep,
                            crate::debugger::DebugAction::Continue => MenuAction::DebugContinue,
                        });
                    }
                }

                #[cfg(not(feature = "debug-hooks"))]
                {
                    let _ = self.panel;
                    let _ = info;
                    ui.label("Debugger disabled — build with `--features debug-hooks`.");
                }
            });
        self.debugger_visible = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_action_carries_region() {
        let a = MenuAction::SetRegion(Region::Pal);
        assert_eq!(a, MenuAction::SetRegion(Region::Pal));
        assert_ne!(a, MenuAction::SetRegion(Region::Ntsc));
    }

    #[test]
    fn locked_branch_taken_only_when_reading_emu() {
        let mut s = ShellState::default();
        assert!(!s.needs_emu_read());
        s.debugger_visible = true;
        assert!(s.needs_emu_read());
    }

    #[test]
    fn shell_state_defaults_closed() {
        let s = ShellState::default();
        assert!(!s.debugger_visible);
        assert!(!s.settings_open);
        assert_eq!(s.panel, DebugPanel::Cpu);
    }
}
