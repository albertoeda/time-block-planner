//! Tasks pane: the nested task list, drawn as monospace rows.

use egui::text::LayoutJob;
use egui::{
    DragAndDrop, Id, LayerId, Order, Rect, Sense, Stroke, StrokeKind, TextFormat, TextStyle, pos2,
    vec2,
};

use super::chrome::{color, pane_header};
use super::timeline::TaskDrag;
use crate::state::{AppState, Command, Focus, Mode};
use crate::tasks::Task;
use crate::theme::Theme;

/// Clickable row below the last task that appends a new one.
const NEW_TASK_ROW: &str = "+ new task";

/// A row's `(prefix, title)`: two spaces per depth level plus the `[ ]`/`[x]` box, and the title
/// (`(untitled)` when empty).
pub fn task_row_text(task: &Task) -> (String, String) {
    let mark = if task.done { "[x] " } else { "[ ] " };
    let prefix = format!("{}{mark}", "  ".repeat(task.depth as usize));
    let title = if task.title.is_empty() {
        "(untitled)".to_string()
    } else {
        task.title.clone()
    };
    (prefix, title)
}

/// Draws the tasks pane and returns its outer width.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme) -> f32 {
    let frame = egui::Frame::NONE
        .fill(color(theme.bg))
        .inner_margin(egui::Margin::symmetric(8, 4));
    let width = egui::Panel::left("tasks")
        .resizable(true)
        .default_size((ui.available_width() / 2.0).round())
        .frame(frame)
        .show(ui, |ui| {
            // Fill the pane so its width never follows the content (a truncated header
            // would otherwise shrink the pane a little on every repaint).
            ui.set_min_width(ui.available_width());
            pane_header(ui, "tasks", state.focus == Focus::Tasks, theme);
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| draw(ui, state, theme));
        })
        .response
        .rect
        .width();
    drag_ghost(ui.ctx(), theme);
    width
}

fn draw(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.text_style_height(&TextStyle::Monospace) + 4.0;
    let char_w = ui
        .painter()
        .layout_no_wrap("0".into(), font.clone(), color(theme.fg))
        .rect
        .width();
    let width = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;

    let (dim, fg) = (color(theme.dim), color(theme.fg));
    let editing = match state.mode {
        Mode::EditTask { idx, .. } => Some(idx),
        _ => None,
    };
    let blink_on = ui.input(|i| i.time) % 1.0 < 0.5;
    // Selected row; index `len` is the "+ new task" row.
    let sel_row = state.task_sel.min(state.tasks.len());
    let outline = if state.focus == Focus::Tasks {
        theme.accent
    } else {
        theme.dim
    };

    // Scroll to the selection only when it changes, so the wheel still works.
    let scrolled_id = Id::unique("tasks-scrolled-to");
    let scroll = ui.data(|d| d.get_temp::<usize>(scrolled_id)) != Some(sel_row);
    ui.data_mut(|d| d.insert_temp(scrolled_id, sel_row));
    let mark_selected = |ui: &egui::Ui, rect: Rect| {
        ui.painter().rect_stroke(
            rect,
            0.0,
            Stroke::new(1.5, color(outline)),
            StrokeKind::Inside,
        );
        if scroll {
            ui.scroll_to_rect(rect.expand2(vec2(0.0, row_h * 2.0)), None);
        }
    };

    let mut clicked = None;
    for (idx, task) in state.tasks.items().iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, row_h), Sense::click_and_drag());
        let (prefix, title) = task_row_text(task);

        let mut job = LayoutJob::default();
        job.append(&prefix, 0.0, TextFormat::simple(font.clone(), dim));
        if editing == Some(idx) {
            let text = format!("{}{}", state.title_buf, if blink_on { "█" } else { " " });
            job.append(&text, 0.0, TextFormat::simple(font.clone(), fg));
        } else if task.done {
            let format = TextFormat {
                strikethrough: Stroke::new(1.0, dim),
                ..TextFormat::simple(font.clone(), dim)
            };
            job.append(&title, 0.0, format);
        } else {
            job.append(&title, 0.0, TextFormat::simple(font.clone(), fg));
        }
        let painter = ui.painter_at(rect);
        let galley = painter.layout_job(job);
        let text_pos = pos2(
            rect.left() + char_w * 0.5,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, fg);

        if sel_row == idx {
            mark_selected(ui, rect);
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            // The "[ ]" glyph spans three characters after the indent.
            let box_left = text_pos.x + char_w * (task.depth as f32 * 2.0);
            let on_checkbox = pos.x >= box_left && pos.x < box_left + char_w * 3.0;
            clicked = Some((idx, on_checkbox));
        }
        if response.drag_started() && state.mode == Mode::Normal {
            DragAndDrop::set_payload(ui.ctx(), TaskDrag(task.title.clone()));
        }
    }
    // "+ new task" row after the last task.
    let (rect, response) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
    let on_row = sel_row == state.tasks.len();
    let lit = state.mode == Mode::Normal
        && (response.hovered() || (on_row && state.focus == Focus::Tasks));
    if on_row {
        mark_selected(ui, rect);
    }
    ui.painter_at(rect).text(
        pos2(rect.left() + char_w * 0.5, rect.center().y),
        egui::Align2::LEFT_CENTER,
        NEW_TASK_ROW,
        font,
        color(if lit { theme.accent } else { theme.dim }),
    );
    let new_task = response.clicked();

    if editing.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    let finish_edit = |state: &mut AppState| {
        if matches!(state.mode, Mode::EditTask { .. } | Mode::EditTitle { .. }) {
            state.apply(Command::Confirm, super::today());
        }
    };
    if let Some((idx, on_checkbox)) = clicked {
        finish_edit(state);
        state.click_task(idx, on_checkbox);
    }
    if new_task {
        finish_edit(state);
        state.new_task_at_end();
    }
}

/// While a task is being dragged, paint its title beside the pointer, above everything.
fn drag_ghost(ctx: &egui::Context, theme: &Theme) {
    let Some(drag) = DragAndDrop::payload::<TaskDrag>(ctx) else {
        return;
    };
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    let font = TextStyle::Monospace.resolve(&ctx.global_style());
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("task-drag")));
    let title = if drag.0.is_empty() {
        "(untitled)".to_string()
    } else {
        drag.0.clone()
    };
    let galley = painter.layout_no_wrap(title, font, color(theme.fg));
    let pad = 3.0;
    let rect = Rect::from_min_size(pointer + vec2(14.0, 4.0), galley.size()).expand(pad);
    painter.rect_filled(rect, 0.0, color(theme.bg));
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, color(theme.accent)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(pad, pad), galley, color(theme.fg));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;
    use crate::tasks::Task;
    use crate::theme::themes;

    /// Runs the tasks pane for `frames` frames on a 900x600 screen; returns its width per frame.
    fn pane_widths(tasks_txt: &str, frames: usize) -> Vec<f32> {
        let tmp = tempfile::tempdir().unwrap();
        if !tasks_txt.is_empty() {
            std::fs::write(tmp.path().join("tasks.txt"), tasks_txt).unwrap();
        }
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let mut state = AppState::new(
            Store {
                dir: tmp.path().to_path_buf(),
            },
            date,
        );
        let theme = &themes()[0];
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        for i in 0..frames {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0))),
                // Pointer moving over the right half (the timeline) every frame.
                events: vec![egui::Event::PointerMoved(egui::pos2(
                    700.0,
                    100.0 + i as f32,
                ))],
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| widths.push(show(ui, &mut state, theme)));
            out.textures_delta.clear();
        }
        widths
    }

    #[test]
    fn pane_opens_at_half_width_and_stays_put() {
        for txt in ["", "- [ ] a\n"] {
            let w = pane_widths(txt, 10);
            assert!((w[0] - 450.0).abs() <= 1.0, "{txt:?}: opened at {}", w[0]);
            assert!(
                w.iter().all(|x| (x - w[0]).abs() < 0.5),
                "{txt:?}: drifted {w:?}"
            );
        }
    }

    #[test]
    fn row_text_indents_and_marks() {
        let t = Task {
            title: "Draft".into(),
            done: true,
            depth: 2,
        };
        assert_eq!(task_row_text(&t), ("    [x] ".into(), "Draft".into()));
        let e = Task {
            title: String::new(),
            done: false,
            depth: 0,
        };
        assert_eq!(task_row_text(&e), ("[ ] ".into(), "(untitled)".into()));
    }
}
