//! Pane headers, status bar, overlays and egui visuals derived from a `Theme`.

use egui::text::LayoutJob;
use egui::{
    Align2, Color32, CornerRadius, Frame, Id, Order, RichText, Shadow, Stroke, TextFormat,
    TextStyle, vec2,
};

use crate::state::{AppState, Focus, Mode};
use crate::theme::{Rgb, Theme, themes};

pub fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

pub fn faded(c: Rgb, alpha: f32) -> Color32 {
    color(c).gamma_multiply(alpha)
}

/// Square, shadowless egui visuals in the theme's colors.
pub fn visuals(theme: &Theme) -> egui::Visuals {
    let mut v = if theme.is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
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
    Frame::NONE
        .fill(color(theme.bg))
        .inner_margin(egui::Margin::symmetric(8, 4))
}

/// A pane's title line, `┌─ label ───…`. The focused pane shows its label inverted on the
/// accent color with an accent rule; the other pane is dimmed.
pub fn pane_header(ui: &mut egui::Ui, label: &str, focused: bool, theme: &Theme) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let line = color(if focused { theme.accent } else { theme.dim });
    let mut job = LayoutJob::default();
    job.append("┌─", 0.0, TextFormat::simple(font.clone(), line));
    let label_format = if focused {
        TextFormat {
            background: color(theme.accent),
            ..TextFormat::simple(font.clone(), color(theme.bg))
        }
    } else {
        TextFormat::simple(font.clone(), line)
    };
    job.append(&format!(" {label} "), 0.0, label_format);
    job.append(&"─".repeat(200), 0.0, TextFormat::simple(font, line));
    ui.add(egui::Label::new(job).truncate());
}

/// The footer's pane name: which pane the keys act on.
pub fn pane_name(focus: Focus) -> &'static str {
    match focus {
        Focus::Tasks => "tasks",
        Focus::Timeline => "time",
    }
}

/// `(keys, action)` pairs for the footer, for the current mode and pane. `^` is Ctrl/Cmd.
pub fn hints(state: &AppState) -> &'static [(&'static str, &'static str)] {
    match state.mode {
        Mode::Normal => match state.focus {
            Focus::Tasks => &[
                ("n", "new"),
                ("Space", "tick"),
                ("Tab/Shift+Tab", "indent"),
                ("Alt+↑↓", "swap"),
                ("p", "plan"),
                ("→", "time"),
            ],
            Focus::Timeline => &[
                ("n", "new"),
                ("1 2 3", "type"),
                ("Alt+↑↓", "move"),
                ("^↑↓", "resize"),
                ("Shift+←→", "day"),
                ("←", "tasks"),
            ],
        },
        Mode::Create { .. } => &[("↑↓", "size"), ("Enter", "create"), ("Esc", "cancel")],
        Mode::EditTitle { .. } | Mode::EditTask { .. } => &[("Enter", "save"), ("Esc", "cancel")],
        Mode::MoveBlock { .. } => &[("drag", "move"), ("Esc", "cancel")],
        Mode::ThemePicker { .. } => &[("↑↓", "preview"), ("Enter", "keep"), ("Esc", "revert")],
        Mode::Help => &[("Esc", "close")],
    }
}

pub fn status_bar(ui: &mut egui::Ui, state: &AppState, theme: &Theme) {
    egui::Panel::bottom("status")
        .frame(bar_frame(theme))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(" {} ", pane_name(state.focus)))
                        .color(color(theme.bg))
                        .background_color(color(theme.accent)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} │ F1 help", theme.name)).color(color(theme.dim)),
                    );
                    if !state.status.is_empty() {
                        let status_color = if state.status == "saved" {
                            theme.fg
                        } else {
                            theme.error
                        };
                        ui.label(
                            RichText::new(format!("{} │", state.status)).color(color(status_color)),
                        );
                    }
                    // The hints take the space left between the pane name and the right side.
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(hint_job(ui, state, theme)).truncate());
                    });
                });
            });
        });
}

/// Footer hints as `keys action  keys action…`, keys in the accent color.
fn hint_job(ui: &egui::Ui, state: &AppState, theme: &Theme) -> LayoutJob {
    let font = TextStyle::Monospace.resolve(ui.style());
    let mut job = LayoutJob::default();
    for (i, (keys, what)) in hints(state).iter().enumerate() {
        let sep = if i == 0 { "" } else { "  " };
        let key_format = TextFormat::simple(font.clone(), color(theme.accent));
        job.append(&format!("{sep}{keys}"), 0.0, key_format);
        let what_format = TextFormat::simple(font.clone(), color(theme.dim));
        job.append(&format!(" {what}"), 0.0, what_format);
    }
    job
}

const HELP: &[(&str, &str)] = &[
    ("← →", "go to the tasks / timeline pane"),
    ("Shift+← Shift+→", "previous / next day"),
    ("t, ^T", "today"),
    ("↑ ↓", "move 15 min · select task"),
    ("PgUp PgDn", "move 1 hour · 10 tasks"),
    ("Home End", "00:00 / 23:45 · first / last task"),
    ("^Shift+↑ ↓", "previous / next block"),
    ("n, ^N", "new block at cursor · new task"),
    ("N", "new subtask"),
    ("Enter, F2", "rename block or task · add on + new task"),
    ("d, Delete, Backspace", "delete block · task and subtasks"),
    ("Alt+↑ Alt+↓", "move block · swap task with sibling"),
    ("^↑ ^↓", "shorten / lengthen block"),
    ("Shift+↑ Shift+↓", "select a range for a new block"),
    ("1 2 3", "standard / scheduled / deep"),
    ("Space, x", "tick / untick task"),
    ("Tab, Shift+Tab", "indent / outdent task"),
    ("p", "plan task on the timeline"),
    ("^Z", "undo"),
    ("^Shift+Z, ^Y", "redo"),
    ("^S", "save (autosaves anyway)"),
    ("^,", "choose theme"),
    ("F1, ?", "this help"),
    ("^Q", "quit"),
    ("mouse drag", "create or move a block"),
    ("drag a task", "plan it (up to 1 hour)"),
    ("click [ ]", "tick / untick task"),
];

fn overlay(ctx: &egui::Context, id: &str, theme: &Theme, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(color(theme.bg))
                .stroke(Stroke::new(1.0, color(theme.accent)))
                .inner_margin(12)
                .show(ui, contents);
        });
}

pub fn help_overlay(ctx: &egui::Context, theme: &Theme) {
    overlay(ctx, "help", theme, |ui| {
        ui.label(RichText::new("─ keys ─").color(color(theme.accent)));
        for (keys, what) in HELP {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{keys:<22}")).color(color(theme.accent)));
                ui.label(*what);
            });
        }
        let ctrl = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let note = format!("^ is {ctrl} · Esc closes");
        ui.label(RichText::new(note).color(color(theme.dim)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;

    #[test]
    fn footer_names_pane_and_lists_its_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let store = Store {
            dir: tmp.path().to_path_buf(),
        };
        let mut s = AppState::new(store, date);
        let keys = |s: &AppState| hints(s).iter().map(|h| h.0).collect::<Vec<_>>();
        s.focus = Focus::Timeline;
        assert_eq!(pane_name(s.focus), "time");
        assert!(keys(&s).contains(&"1 2 3"));
        s.focus = Focus::Tasks;
        assert_eq!(pane_name(s.focus), "tasks");
        assert!(keys(&s).contains(&"Space"));
        assert!(!keys(&s).contains(&"^Z") && !keys(&s).contains(&"d"));
        s.mode = Mode::Help;
        assert_eq!(keys(&s), ["Esc"]);
        // Ctrl/Cmd is always written as ^.
        assert!(
            HELP.iter()
                .all(|(k, _)| !k.contains("Ctrl") && !k.contains("^+"))
        );
    }
}
