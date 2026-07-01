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
    /// View -> toggle fullscreen.
    ToggleFullscreen,
    /// File -> open the Settings window.
    OpenSettings,
    /// Help -> open the in-app Documentation pane.
    OpenDocs,
    /// File -> Quit.
    Quit,
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
    /// CPU debug string.
    pub cpu_info: String,
    /// TIA debug string.
    pub tia_info: String,
    /// RIOT debug string.
    pub riot_info: String,
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
                });

                ui.menu_button("Tools", |ui| {
                    // TODO(impl-phase): TIA audio scope, cheat editor, ROM-DB editor, TAStudio.
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
            self.render_settings(&ctx, cfg);
        }
        if self.debugger_visible {
            self.render_debugger(&ctx, info);
        }

        actions
    }

    /// The tabbed Settings window (Video / Audio / Input / System). v0.1 wires the live config
    /// fields; deep per-knob panels (NTSC, shader stack, per-game overrides) are TODO.
    fn render_settings(&mut self, ctx: &egui::Context, cfg: &mut Config) {
        let mut open = self.settings_open;
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
                            }
                        }
                        ui.checkbox(&mut cfg.video.integer_scale, "Integer scale");
                    }
                    1 => {
                        ui.checkbox(&mut cfg.audio.enabled, "Audio enabled");
                        ui.add(egui::Slider::new(&mut cfg.audio.volume, 0.0..=1.0).text("Volume"));
                    }
                    2 => {
                        // TODO(impl-phase): the 2600 key-rebind grid (joystick * 2 players + the
                        // console-switch row).
                        ui.label("Input rebinding — TODO (defaults in input.rs).");
                    }
                    _ => {
                        ui.label("Region:");
                        ui.radio_value(&mut cfg.region, Region::Ntsc, "NTSC");
                        ui.radio_value(&mut cfg.region, Region::Pal, "PAL");
                        ui.radio_value(&mut cfg.region, Region::Secam, "SECAM");
                    }
                }
            });
        self.settings_open = open;
    }

    /// The debugger overlay: a panel selector + the 2600 chip-panel stubs.
    fn render_debugger(&mut self, ctx: &egui::Context, info: &ShellInfo) {
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
                });
                ui.separator();
                match self.panel {
                    // TODO(impl-phase): each panel reads the live chip state (copied out under the
                    // brief emu lock, never read inside this egui closure) and renders the register
                    // grid / disassembly / viewers.
                    DebugPanel::Cpu => {
                        ui.label("6507 — registers (A/X/Y/SP/PC/P), disassembly, breakpoints.");
                        ui.label(info.cpu_info.clone());
                    }
                    DebugPanel::Tia => {
                        ui.label(
                            "TIA — object regs (P0/P1/M0/M1/BL), the playfield, the beam position",
                        );
                        ui.label(info.tia_info.clone());
                    }
                    DebugPanel::Riot => {
                        ui.label(
                            "RIOT — the interval timer (INTIM/INSTAT), the SWCHA/SWCHB ports,",
                        );
                        ui.label(info.riot_info.clone());
                    }
                    DebugPanel::Memory => {
                        ui.label("Memory — the RIOT's 128 bytes of RAM + the cart window view +");
                        ui.label("the bankswitch board's current mapping. TODO(impl-phase).");
                    }
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
