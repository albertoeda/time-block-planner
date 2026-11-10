//! Header, status bar, overlays and egui visuals derived from a `Theme`.

use chrono::NaiveDate;
use egui::{Align2, Color32, CornerRadius, Frame, Id, Order, RichText, Shadow, Stroke, vec2};

use crate::state::{AppState, Mode};
use crate::theme::{Rgb, Theme, themes};

pub fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

pub fn faded(c: Rgb, alpha: f32) -> Color32 {
    color(c).gamma_multiply(alpha)
}

/// Square, shadowless egui visuals in the theme's colors.
pub fn visuals(theme: &Theme) -> egui::Visuals {
    let mut v = if theme.is_light { egui::Visuals::light() } else { egui::Visuals::dark() };
    let (bg, fg, dim) = (color(theme.bg), color(theme.fg), color(theme.dim));
    v.override_text_color = Some(fg);
    v.panel_fill = bg;
    v.window_fill = bg;
    v.extreme_bg_color = bg;
    v.faint_bg_color = bg;
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_stroke = Stroke::new(1.0, dim);
    v.selection.bg_fill = color(theme.selection);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::ZERO;
        w.bg_stroke.color = dim;
    }
    v
}

fn bar_frame(theme: &Theme) -> Frame {
    Frame::NONE.fill(color(theme.bg)).inner_margin(egui::Margin::symmetric(8, 4))
}

pub fn header(ui: &mut egui::Ui, state: &AppState, theme: &Theme, today: NaiveDate) {
    egui::Panel::top("header").frame(bar_frame(theme)).show(ui, |ui| {
        let tag = if state.date == today { " [today] ─" } else { "" };
        let text = format!("┌─ {}{tag}", state.date.format("%a %Y-%m-%d ─"));
        let fill = "─".repeat(200);
        ui.add(egui::Label::new(RichText::new(format!("{text}{fill}")).color(color(theme.accent))).truncate());
    });
}

pub fn status_bar(ui: &mut egui::Ui, state: &AppState, theme: &Theme) {
    egui::Panel::bottom("status").frame(bar_frame(theme)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mode = match state.mode {
                Mode::Normal => "NORMAL",
                Mode::Create { .. } => "CREATE",
                Mode::EditTitle { .. } | Mode::EditTask { .. } => "EDIT",
                Mode::MoveBlock { .. } => "MOVE",
                Mode::ThemePicker { .. } => "THEME",
                Mode::Help => "HELP",
            };
            ui.label(RichText::new(format!(" {mode} ")).color(color(theme.bg)).background_color(color(theme.accent)));
            let status_color = if state.status == "saved" { theme.fg } else { theme.error };
            ui.label(RichText::new(&state.status).color(color(status_color)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{} │ F1 help", theme.name)).color(color(theme.dim)));
            });
        });
    });
}

/// Ctrl on Windows/Linux, Cmd on macOS.
fn mod_key() -> &'static str {
    if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" }
}

const HELP: &[(&str, &str)] = &[
    ("↑ ↓", "move 15 min"),
    ("PgUp PgDn", "move 1 hour"),
    ("Home End, ^+↑ ^+↓", "start / end of day"),
    ("← →", "previous / next day"),
    ("^+T", "today"),
    ("Tab Shift+Tab", "next / previous block"),
    ("^+N", "new block at cursor"),
    ("Shift+↑ Shift+↓", "select a range for a new block"),
    ("Enter, F2", "rename block"),
    ("1 2 3", "standard / scheduled / deep"),
    ("Alt+↑ Alt+↓", "move block"),
    ("Alt+Shift+↑ ↓", "shorten / lengthen block"),
    ("Delete, Backspace", "delete block"),
    ("^+Z", "undo"),
    ("^+Shift+Z, ^+Y", "redo"),
    ("^+S", "save (autosaves anyway)"),
    ("^+,", "choose theme"),
    ("F1, ?", "this help"),
    ("^+Q", "quit"),
    ("mouse drag", "create a block"),
];

fn overlay(ctx: &egui::Context, id: &str, theme: &Theme, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(Id::unique(id)).order(Order::Foreground).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
        ctx,
        |ui| {
            Frame::NONE
                .fill(color(theme.bg))
                .stroke(Stroke::new(1.0, color(theme.accent)))
                .inner_margin(12)
                .show(ui, contents);
        },
    );
}

pub fn help_overlay(ctx: &egui::Context, theme: &Theme) {
    overlay(ctx, "help", theme, |ui| {
        ui.label(RichText::new("─ keys ─").color(color(theme.accent)));
        for (keys, what) in HELP {
            let keys = keys.replace('^', mod_key());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{keys:<22}")).color(color(theme.accent)));
                ui.label(*what);
            });
        }
        ui.label(RichText::new("Esc closes").color(color(theme.dim)));
    });
}

pub fn theme_picker(ctx: &egui::Context, state: &AppState, theme: &Theme) {
    overlay(ctx, "themes", theme, |ui| {
        ui.label(RichText::new("─ theme ─").color(color(theme.accent)));
        for (i, t) in themes().iter().enumerate() {
            let current = i == state.theme;
            let text = format!("{} {}", if current { "▶" } else { " " }, t.name);
            let c = if current { theme.accent } else { theme.fg };
            ui.label(RichText::new(text).color(color(c)));
        }
        ui.label(RichText::new("↑↓ preview · Enter keep · Esc revert").color(color(theme.dim)));
    });
}
