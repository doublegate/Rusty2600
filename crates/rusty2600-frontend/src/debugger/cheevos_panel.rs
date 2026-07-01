//! The RetroAchievements panel — login, live achievement/leaderboard lists,
//! rich presence, and recent unlock notifications (`T-0802-005`).
//!
//! `rusty2600-cheevos`'s `RaClient` already exposes the entire surface this
//! needs (`begin_login_password`, `achievement_list`, `leaderboard_list`,
//! `rich_presence`, progress (de)serialization) — this panel is UI plumbing
//! over `crate::cheevos::CheevosState`, not new client capability.

use crate::cheevos::{CheevosState, LoginState};

/// Renders the full RetroAchievements panel against the live `cheevos` state.
pub fn render_cheevos_panel(ui: &mut egui::Ui, cheevos: &mut CheevosState) {
    render_login_section(ui, cheevos);
    ui.separator();

    if cheevos.login_state() != LoginState::LoggedIn {
        ui.label("Log in to see achievements, leaderboards, and rich presence.");
        return;
    }

    if !cheevos.game_loaded() {
        ui.label("(no ROM identified yet)");
        return;
    }

    render_summary_section(ui, cheevos);
    ui.separator();
    render_toasts_section(ui, cheevos);
    ui.separator();
    render_achievement_list(ui, cheevos);
    ui.separator();
    render_leaderboard_list(ui, cheevos);
}

fn render_login_section(ui: &mut egui::Ui, cheevos: &mut CheevosState) {
    match cheevos.login_state() {
        LoginState::LoggedIn => {
            let name = cheevos
                .user_info()
                .map_or_else(|| "(unknown user)".to_string(), |u| u.display_name);
            ui.horizontal(|ui| {
                ui.label(format!("Logged in as {name}"));
                if ui.button("Log out").clicked() {
                    cheevos.logout();
                }
            });
        }
        LoginState::LoggingIn => {
            ui.label("Logging in...");
        }
        LoginState::LoggedOut | LoginState::Error(_) => {
            if let LoginState::Error(msg) = cheevos.login_state() {
                ui.colored_label(egui::Color32::RED, format!("Login failed: {msg}"));
            }
            ui.horizontal(|ui| {
                ui.label("Username:");
                ui.text_edit_singleline(&mut cheevos.username_input);
            });
            ui.horizontal(|ui| {
                ui.label("Password:");
                ui.add(egui::TextEdit::singleline(&mut cheevos.password_input).password(true));
            });
            if ui.button("Log in").clicked() {
                cheevos.begin_login();
            }
        }
    }
}

fn render_summary_section(ui: &mut egui::Ui, cheevos: &mut CheevosState) {
    let summary = cheevos.game_summary();
    ui.label(format!(
        "{} / {} achievements unlocked ({} unofficial, {} unsupported)",
        summary.num_unlocked_achievements,
        summary.num_core_achievements,
        summary.num_unofficial_achievements,
        summary.num_unsupported_achievements
    ));
    let presence = cheevos.rich_presence();
    if !presence.is_empty() {
        ui.label(format!("Rich presence: {presence}"));
    }
}

fn render_toasts_section(ui: &mut egui::Ui, cheevos: &CheevosState) {
    if cheevos.toasts.is_empty() {
        return;
    }
    ui.label("Recent unlocks:");
    for toast in cheevos.toasts.iter().rev().take(5) {
        let color = if toast.is_error {
            egui::Color32::RED
        } else {
            egui::Color32::GOLD
        };
        ui.colored_label(color, format!("{} — {}", toast.title, toast.detail));
    }
}

fn render_achievement_list(ui: &mut egui::Ui, cheevos: &mut CheevosState) {
    let achievements = cheevos.achievement_list();
    ui.label(format!("Achievements ({}):", achievements.len()));
    egui::ScrollArea::vertical()
        .max_height(250.0)
        .id_salt("cheevos_achievement_list")
        .show(ui, |ui| {
            egui::Grid::new("cheevos_achievements")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    for a in &achievements {
                        let unlocked = a.state == 2;
                        let marker = if unlocked { "[x]" } else { "[ ]" };
                        ui.label(marker);
                        ui.label(&a.title);
                        ui.label(format!("{} pts", a.points));
                        ui.end_row();
                    }
                });
        });
}

fn render_leaderboard_list(ui: &mut egui::Ui, cheevos: &mut CheevosState) {
    let leaderboards = cheevos.leaderboard_list();
    if leaderboards.is_empty() {
        return;
    }
    ui.label(format!("Leaderboards ({}):", leaderboards.len()));
    egui::ScrollArea::vertical()
        .max_height(150.0)
        .id_salt("cheevos_leaderboard_list")
        .show(ui, |ui| {
            for lb in &leaderboards {
                ui.label(&lb.title);
            }
        });
}
