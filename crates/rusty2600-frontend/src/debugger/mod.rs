//! The real debugger: persistent state (breakpoints, memory-viewer cursor)
//! + structured live-chip snapshots + the panel renderers.
//!
//! Follows the shell's non-negotiable rule: nothing in here ever touches the
//! emu lock. [`crate::debugger::DebugSnapshot`] is built once per frame
//! under the brief lock in `app.rs`, then handed to the (lock-free) render
//! functions here.

pub mod access_counter;
pub mod callstack;
#[cfg(all(not(target_arch = "wasm32"), feature = "retroachievements"))]
pub mod cheevos_panel;
pub mod disasm;
pub mod event_panel;
pub mod expr;
#[cfg(all(not(target_arch = "wasm32"), feature = "scripting"))]
pub mod lua_console_panel;
pub mod memory_compare_panel;
pub mod pmb_panel;
pub mod tastudio_panel;
pub mod watch_panel;

use std::collections::BTreeSet;

/// One user-defined watch expression (`crate::debugger::expr`'s grammar).
#[derive(Debug, Clone)]
pub struct WatchEntry {
    /// The expression text, e.g. `"a == $42"` or `"[$80] != 0"`.
    pub expr: String,
    /// When true, this watch also acts as a conditional breakpoint —
    /// `MenuAction::DebugContinue`'s step loop halts the instant it
    /// evaluates true, in addition to the existing PC breakpoint set.
    pub break_when_true: bool,
}

/// Persistent debugger UI state: breakpoints, the memory-viewer cursor, and
/// text-input buffers.
///
/// Lives on [`crate::shell::ShellState`] so it survives across frames like
/// the rest of the shell's UI toggles.
#[derive(Debug, Default, Clone)]
pub struct DebuggerState {
    /// CPU addresses that halt "Continue" when the PC reaches them.
    pub breakpoints: BTreeSet<u16>,
    /// The hex text the user is typing into the "add breakpoint" field.
    pub breakpoint_input: String,
    /// The memory panel's current base address (hex-viewer scroll position).
    pub memory_base: u16,
    /// The memory panel's address-range text input buffer.
    pub memory_base_input: String,
    /// The watch/conditional-breakpoint expression list (`expr`'s grammar).
    pub watches: Vec<WatchEntry>,
    /// The text the user is typing into the "add watch" field.
    pub watch_input: String,
    /// The live JSR/RTS call stack, return addresses oldest-first —
    /// updated by `crate::app`'s Step/Continue handlers (see `callstack`).
    pub call_stack: Vec<u16>,
    /// TAStudio-lite: the in-progress/loaded movie, cursor, and mode
    /// (`tastudio_panel`).
    pub tastudio: tastudio_panel::TastudioState,
    /// `memory_compare_panel`'s captured baseline snapshot, if any.
    pub memory_compare_baseline: Option<Vec<u8>>,
}

impl DebuggerState {
    /// Parse `breakpoint_input` as a hex address and add it, clearing the
    /// input on success. Silently no-ops on unparsable input (the UI shows
    /// the raw text either way, so a bad parse is visible to the user).
    pub fn commit_breakpoint_input(&mut self) {
        if let Ok(addr) = u16::from_str_radix(self.breakpoint_input.trim_start_matches('$'), 16) {
            self.breakpoints.insert(addr);
            self.breakpoint_input.clear();
        }
    }

    /// Parse `memory_base_input` as a hex address and jump the viewer there.
    pub fn commit_memory_base_input(&mut self) {
        if let Ok(addr) = u16::from_str_radix(self.memory_base_input.trim_start_matches('$'), 16) {
            self.memory_base = addr;
        }
    }

    /// Add `watch_input` as a new (display-only, not breakpoint-armed)
    /// watch entry, clearing the input. No-ops on an empty/whitespace input.
    pub fn commit_watch_input(&mut self) {
        let text = self.watch_input.trim();
        if !text.is_empty() {
            self.watches.push(WatchEntry {
                expr: text.to_string(),
                break_when_true: false,
            });
            self.watch_input.clear();
        }
    }

    /// Evaluates every `break_when_true` watch against `ctx`, returning
    /// true if any evaluates to true (an evaluation error counts as false —
    /// a typo in one watch must never itself halt execution).
    #[must_use]
    pub fn any_breakpoint_watch_triggered(&self, ctx: &expr::EvalContext) -> bool {
        self.watches
            .iter()
            .filter(|w| w.break_when_true)
            .any(|w| expr::evaluate(&w.expr, ctx) == Ok(true))
    }
}

/// A side-effect-free snapshot of live chip state, copied out under the
/// brief emu lock once per frame the debugger overlay is open.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    /// The 6507's registers.
    pub cpu: CpuSnapshot,
    /// The TIA's beam position, object registers, and collision latches.
    pub tia: TiaSnapshot,
    /// The RIOT's timer, ports, and DDRs (RAM is read via `memory_view`
    /// below when the memory panel's base address points at it, not
    /// duplicated here).
    pub riot: RiotSnapshot,
    /// Disassembly lines starting at the CPU's current PC, `(address, text)`.
    pub disassembly_at_pc: Vec<(u16, String)>,
    /// [`MEMORY_VIEW_LEN`] side-effect-free bytes starting at the memory
    /// panel's current base address (one `Bus::peek_range` call).
    pub memory_view: Vec<u8>,
    /// The RIOT's full 128 B of RAM (`$0080-$00FF`) — the console's only
    /// general RAM, and the address range [`expr::EvalContext`]'s `[addr]`
    /// operand actually resolves against.
    pub riot_ram: Vec<u8>,
    /// Frames completed since power-on/ROM-load (`expr`'s `frame` operand).
    pub frame: u64,
    /// This frame's TIA write events (`rusty2600_core::WriteEvent`), for
    /// [`event_panel`] — only populated while that panel is selected, since
    /// recording has a (small but nonzero) per-write cost.
    pub tia_writes: Vec<rusty2600_core::WriteEvent>,
}

impl DebugSnapshot {
    /// Builds an [`expr::EvalContext`] from this snapshot, peeking `[addr]`
    /// operands against [`Self::riot_ram`] (`$0080-$00FF`; any other address
    /// reads as `0` — that range is the console's only general RAM and the
    /// only address space a watch expression can usefully condition on).
    #[must_use]
    pub fn eval_context(&self) -> expr::EvalContext<'_> {
        expr::EvalContext {
            a: self.cpu.a,
            x: self.cpu.x,
            y: self.cpu.y,
            s: self.cpu.s,
            pc: self.cpu.pc,
            scanline: self.tia.scanline,
            color_clock: self.tia.color_clock,
            frame: self.frame,
            mem: &self.riot_ram,
            mem_base: 0x0080,
        }
    }
}

/// The 6507's user-visible register file.
#[derive(Debug, Clone, Default)]
pub struct CpuSnapshot {
    /// Accumulator.
    pub a: u8,
    /// X index register.
    pub x: u8,
    /// Y index register.
    pub y: u8,
    /// Stack pointer.
    pub s: u8,
    /// Program counter.
    pub pc: u16,
    /// Formatted status flags (`NV-BDIZC` style).
    pub p: String,
}

/// The TIA's beam position + object/collision state.
#[derive(Debug, Clone, Default)]
pub struct TiaSnapshot {
    /// The current scanline (0-based from the last VSYNC).
    pub scanline: u16,
    /// The current color clock within the scanline (0..227).
    pub color_clock: u16,
    /// P0/P1/M0/M1/BL horizontal positions (0..159, visible-window space).
    pub pos: [u8; 5],
    /// P0/P1/PF/BK colors (`COLUP0`/`COLUP1`/`COLUPF`/`COLUBK`).
    pub colu: [u8; 4],
    /// The 15 pairwise collision latches, formatted as a short hex summary.
    pub collisions: String,
    /// P0/P1's `NUSIZx` (copy count/spacing + missile-width bits).
    pub nusiz: [u8; 2],
    /// P0/P1/M0/M1/BL's `HMxx` fine-adjust (signed, `-8..=7`).
    pub hm: [i8; 5],
    /// P0/P1's `REFPx` (horizontal mirror) latch.
    pub refp: [bool; 2],
}

/// The RIOT's timer + I/O port state (RAM is shown via the shared memory
/// panel, not duplicated here).
#[derive(Debug, Clone, Default)]
pub struct RiotSnapshot {
    /// The current `INTIM` countdown value.
    pub timer_value: u8,
    /// The timer's prescale divisor, formatted (`"1"`/`"8"`/`"64"`/`"1024"`).
    pub timer_prescale: String,
    /// `SWCHA`/`SWCHB` port pin state.
    pub ports: [u8; 2],
    /// The two ports' Data Direction Registers.
    pub ddr: [u8; 2],
}

/// An action requested from a debugger panel — mirrors
/// [`crate::shell::MenuAction`]'s "return it, dispatch it after the egui
/// pass" pattern so the panels never touch the emu lock either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    /// Execute exactly one CPU instruction.
    Step,
    /// Run until a breakpoint is hit or a safety step-cap is reached.
    Continue,
}

/// Render the CPU panel: register grid, formatted flags, step/continue
/// controls, breakpoint list, and a short disassembly window around PC.
pub fn render_cpu_panel(
    ui: &mut egui::Ui,
    snap: &CpuSnapshot,
    disassembly: &[(u16, String)],
    state: &mut DebuggerState,
    actions: &mut Vec<DebugAction>,
) {
    egui::Grid::new("cpu_registers")
        .num_columns(6)
        .show(ui, |ui| {
            ui.label("A");
            ui.label("X");
            ui.label("Y");
            ui.label("S");
            ui.label("PC");
            ui.label("P");
            ui.end_row();
            ui.monospace(format!("${:02X}", snap.a));
            ui.monospace(format!("${:02X}", snap.x));
            ui.monospace(format!("${:02X}", snap.y));
            ui.monospace(format!("${:02X}", snap.s));
            ui.monospace(format!("${:04X}", snap.pc));
            ui.monospace(&snap.p);
            ui.end_row();
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Step").clicked() {
            actions.push(DebugAction::Step);
        }
        if ui.button("Continue").clicked() {
            actions.push(DebugAction::Continue);
        }
    });
    ui.separator();
    ui.label("Breakpoints:");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.breakpoint_input);
        if ui.button("Add").clicked() {
            state.commit_breakpoint_input();
        }
    });
    let mut to_remove = None;
    for &addr in &state.breakpoints {
        ui.horizontal(|ui| {
            ui.monospace(format!("${addr:04X}"));
            if ui.small_button("x").clicked() {
                to_remove = Some(addr);
            }
        });
    }
    if let Some(addr) = to_remove {
        state.breakpoints.remove(&addr);
    }
    ui.separator();
    ui.label("Disassembly:");
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            for (addr, text) in disassembly {
                let is_pc = *addr == snap.pc;
                let is_bp = state.breakpoints.contains(addr);
                let marker = if is_pc {
                    "> "
                } else if is_bp {
                    "* "
                } else {
                    "  "
                };
                ui.monospace(format!("{marker}${addr:04X}  {text}"));
            }
        });
}

/// Render the TIA panel: beam position, object registers, collision
/// latches.
pub fn render_tia_panel(ui: &mut egui::Ui, snap: &TiaSnapshot) {
    ui.label(format!(
        "Scanline: {}  Color clock: {}",
        snap.scanline, snap.color_clock
    ));
    ui.separator();
    egui::Grid::new("tia_objects")
        .num_columns(3)
        .show(ui, |ui| {
            ui.label("Object");
            ui.label("Position");
            ui.label("Color");
            ui.end_row();
            for (name, pos, colu) in [
                ("P0", snap.pos[0], Some(snap.colu[0])),
                ("P1", snap.pos[1], Some(snap.colu[1])),
                ("M0", snap.pos[2], None),
                ("M1", snap.pos[3], None),
                ("BL", snap.pos[4], None),
            ] {
                ui.label(name);
                ui.monospace(format!("{pos}"));
                ui.monospace(colu.map_or_else(String::new, |c| format!("${c:02X}")));
                ui.end_row();
            }
            ui.label("PF");
            ui.label("");
            ui.monospace(format!("${:02X}", snap.colu[2]));
            ui.end_row();
            ui.label("BK");
            ui.label("");
            ui.monospace(format!("${:02X}", snap.colu[3]));
            ui.end_row();
        });
    ui.separator();
    ui.label("Collisions:");
    ui.monospace(&snap.collisions);
}

/// Render the RIOT panel: interval timer + I/O ports.
pub fn render_riot_panel(ui: &mut egui::Ui, snap: &RiotSnapshot) {
    ui.label(format!(
        "INTIM: ${:02X}  Prescale: {}",
        snap.timer_value, snap.timer_prescale
    ));
    ui.separator();
    egui::Grid::new("riot_ports").num_columns(3).show(ui, |ui| {
        ui.label("Port");
        ui.label("Pins");
        ui.label("DDR");
        ui.end_row();
        ui.label("SWCHA");
        ui.monospace(format!("${:02X}", snap.ports[0]));
        ui.monospace(format!("${:02X}", snap.ddr[0]));
        ui.end_row();
        ui.label("SWCHB");
        ui.monospace(format!("${:02X}", snap.ports[1]));
        ui.monospace(format!("${:02X}", snap.ddr[1]));
        ui.end_row();
    });
}

/// The number of bytes [`render_memory_panel`] displays per refresh (16 rows
/// x 16 columns).
///
/// Callers should fetch exactly this many bytes starting at
/// `state.memory_base` (one `Bus::peek_range` call, not one `Bus::peek` per
/// byte) before calling this function.
pub const MEMORY_VIEW_LEN: u16 = 256;

/// Render the memory panel: a hex+ASCII viewer over `bytes`.
///
/// `bytes` is a pre-fetched, side-effect-free snapshot of
/// [`MEMORY_VIEW_LEN`] bytes starting at `state.memory_base` (see
/// `rusty2600_core::Bus::peek_range` — fetched ONCE per refresh, not once
/// per displayed byte).
pub fn render_memory_panel(ui: &mut egui::Ui, state: &mut DebuggerState, bytes: &[u8]) {
    ui.horizontal(|ui| {
        ui.label("Base address:");
        ui.text_edit_singleline(&mut state.memory_base_input);
        if ui.button("Go").clicked() {
            state.commit_memory_base_input();
        }
        ui.separator();
        if ui.button("RIOT RAM").clicked() {
            state.memory_base = 0x0080;
        }
        if ui.button("Cart window").clicked() {
            state.memory_base = 0x1000;
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            use std::fmt::Write as _;
            const COLS: u16 = 16;
            for (row_idx, row) in bytes.chunks(COLS as usize).enumerate() {
                // `bytes` is exactly MEMORY_VIEW_LEN (256) long, so row_idx never
                // exceeds 256 / COLS = 16 — the truncation clippy warns about here
                // can't actually happen.
                #[allow(clippy::cast_possible_truncation)]
                let row_idx_u16 = row_idx as u16;
                let row_base = state.memory_base.wrapping_add(row_idx_u16 * COLS);
                let mut line = format!("${row_base:04X}: ");
                let mut ascii = String::new();
                for &byte in row {
                    let _ = write!(line, "{byte:02X} ");
                    ascii.push(if byte.is_ascii_graphic() {
                        byte as char
                    } else {
                        '.'
                    });
                }
                line.push_str(" |");
                line.push_str(&ascii);
                line.push('|');
                ui.monospace(line);
            }
        });
}
