//! The performance-monitor panel (`[v2.12.0]`).
//!
//! Tracks the render pass's own presented-frame interval — the same
//! measurement `crate::app` already computes every frame for the smoothed
//! FPS shown in the status bar (`ShellInfo::fps`), just retained as a short
//! rolling history instead of only an EMA. This is a deliberately smaller
//! scope than a dual "produced vs. presented" cadence graph: Rusty2600's
//! `app.rs` render loop does not yet separate those two timings the way a
//! more elaborate profiler would, so this panel honestly tracks the one
//! signal that's actually available (see the plan's "land the real slice,
//! document the deferred slice" convention). A per-subsystem timing
//! breakdown (CPU/TIA/RIOT/render split) is a natural, explicitly deferred
//! follow-up once that instrumentation exists.
//!
//! Sampling is gated by the caller (`app.rs`) to only push while this panel
//! is the selected/open one, so an unused perf panel costs nothing beyond
//! the FPS smoothing this project's render loop already performs
//! unconditionally.

use std::collections::VecDeque;

/// The rolling history's capacity (samples), sized to a few seconds at a
/// typical 60 Hz presented cadence.
const HISTORY_CAPACITY: usize = 240;

/// Persistent perf-panel state: the rolling frame-interval history.
#[derive(Debug, Default, Clone)]
pub struct PerfState {
    /// Presented-frame intervals in milliseconds, oldest-first, capped at
    /// `HISTORY_CAPACITY`.
    pub history: VecDeque<f32>,
}

/// Append one frame's presented interval (milliseconds).
///
/// Gating (only recording while the perf panel is open) is the caller's
/// responsibility, not this function's — kept unconditional here so the
/// ring behavior itself always stays simple and testable.
pub fn record_frame(state: &mut PerfState, frame_ms: f32) {
    if state.history.len() >= HISTORY_CAPACITY {
        state.history.pop_front();
    }
    state.history.push_back(frame_ms);
}

/// Basic descriptive stats over the current history window.
struct Stats {
    current: f32,
    min: f32,
    max: f32,
    avg: f32,
}

fn compute_stats(history: &VecDeque<f32>) -> Option<Stats> {
    if history.is_empty() {
        return None;
    }
    let current = *history.back()?;
    let min = history.iter().copied().fold(f32::INFINITY, f32::min);
    let max = history.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    #[allow(clippy::cast_precision_loss)]
    let avg = history.iter().copied().sum::<f32>() / history.len() as f32;
    Some(Stats {
        current,
        min,
        max,
        avg,
    })
}

/// Draw a minimal bar-sparkline of `history` (no `egui_plot` dependency —
/// this project keeps its egui stack to exactly `egui`/`egui-wgpu`/
/// `egui-winit`, see `Cargo.toml`'s "always-on egui shell stack" block).
fn sparkline(ui: &mut egui::Ui, history: &VecDeque<f32>) {
    let desired_size = egui::vec2(ui.available_width().min(420.0), 60.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
    if history.is_empty() {
        return;
    }
    let max = history.iter().copied().fold(1.0_f32, f32::max).max(1.0);
    let n = history.len();
    #[allow(clippy::cast_precision_loss)]
    let bar_w = rect.width() / n as f32;
    for (i, &ms) in history.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x0 = (i as f32).mul_add(bar_w, rect.left());
        let h = (ms / max).clamp(0.0, 1.0) * rect.height();
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, rect.bottom() - h),
            egui::pos2(x0 + bar_w.max(1.0), rect.bottom()),
        );
        // Warmer color as frame time rises toward (and past) the 16.67 ms
        // 60 Hz budget — a quick eyeball signal, not a precise gradient.
        let color = if ms > 33.3 {
            egui::Color32::from_rgb(0xE0, 0x50, 0x50)
        } else if ms > 16.7 {
            egui::Color32::from_rgb(0xE0, 0xC0, 0x50)
        } else {
            egui::Color32::from_rgb(0x50, 0xC0, 0x70)
        };
        painter.rect_filled(bar, 0.0, color);
    }
}

/// Render the perf-monitor panel: current FPS, a frame-interval sparkline,
/// and min/avg/max stats over the current rolling window.
pub fn render_perf_panel(ui: &mut egui::Ui, fps: f32, state: &PerfState) {
    ui.label(format!("FPS: {fps:.1}"));
    ui.separator();
    sparkline(ui, &state.history);
    ui.separator();
    if let Some(stats) = compute_stats(&state.history) {
        egui::Grid::new("perf_stats_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Current:");
                ui.monospace(format!("{:.2} ms", stats.current));
                ui.end_row();
                ui.label("Min:");
                ui.monospace(format!("{:.2} ms", stats.min));
                ui.end_row();
                ui.label("Max:");
                ui.monospace(format!("{:.2} ms", stats.max));
                ui.end_row();
                ui.label("Avg:");
                ui.monospace(format!("{:.2} ms", stats.avg));
                ui.end_row();
            });
    } else {
        ui.weak("(collecting samples…)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_frame_caps_at_capacity() {
        let mut state = PerfState::default();
        for i in 0..HISTORY_CAPACITY + 5 {
            #[allow(clippy::cast_precision_loss)]
            record_frame(&mut state, i as f32);
        }
        assert_eq!(state.history.len(), HISTORY_CAPACITY);
        // The oldest 5 were evicted.
        assert!((state.history[0] - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn stats_reflect_recorded_samples() {
        let mut state = PerfState::default();
        for ms in [10.0, 20.0, 30.0] {
            record_frame(&mut state, ms);
        }
        let stats = compute_stats(&state.history).unwrap();
        assert!((stats.current - 30.0).abs() < f32::EPSILON);
        assert!((stats.min - 10.0).abs() < f32::EPSILON);
        assert!((stats.max - 30.0).abs() < f32::EPSILON);
        assert!((stats.avg - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_history_has_no_stats() {
        let state = PerfState::default();
        assert!(compute_stats(&state.history).is_none());
    }
}
