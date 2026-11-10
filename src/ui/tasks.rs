//! Tasks pane: the nested task list, drawn as monospace rows.

use egui::text::LayoutJob;
use egui::{
    DragAndDrop, Id, LayerId, Order, Rect, RichText, Sense, Stroke, StrokeKind, TextFormat,
    TextStyle, pos2, vec2,
};

use super::chrome::color;
use super::timeline::TaskDrag;
use crate::state::{AppState, Command, Focus, Mode};
use crate::tasks::Task;
use crate::theme::Theme;

/// A row's `(prefix, title)`: two spaces per depth level plus the `[ ]`/`[x]` box, and the title
/// (`(untitled)` when empty).
pub fn task_row_text(task: &Task) -> (String, String) {
    let mark = if task.done { "[x] " } else { "[ ] " };
    let prefix = format!("{}{mark}", "  ".repeat(task.depth as usize));
    let title = if task.title.is_empty() { "(untitled)".to_string() } else { task.title.clone() };
    (prefix, title)
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme) {
    let frame = egui::Frame::NONE.fill(color(theme.bg)).inner_margin(egui::Margin::symmetric(8, 4));
    egui::Panel::left("tasks").resizable(true).default_size(320.0).frame(frame).show(ui, |ui| {
        let head = if state.focus == Focus::Tasks { theme.accent } else { theme.dim };
        let text = format!("┌─ tasks ─{}", "─".repeat(200));
        ui.add(egui::Label::new(RichText::new(text).color(color(head))).truncate());
        if state.tasks.is_empty() {
            ui.label(RichText::new("n: new task").color(color(theme.dim)));
            return;
        }
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| draw(ui, state, theme));
    });
    drag_ghost(ui.ctx(), state, theme);
}

fn draw(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.text_style_height(&TextStyle::Monospace) + 4.0;
    let char_w = ui.painter().layout_no_wrap("0".into(), font.clone(), color(theme.fg)).rect.width();
    let width = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;

    let (dim, fg) = (color(theme.dim), color(theme.fg));
    let editing = match state.mode {
        Mode::EditTask { idx, .. } => Some(idx),
        _ => None,
    };
    let blink_on = ui.input(|i| i.time) % 1.0 < 0.5;
    let selected = state.selected_task();
    let outline = if state.focus == Focus::Tasks { theme.accent } else { theme.dim };

    // Scroll to the selection only when it changes, so the wheel still works.
    let scrolled_id = Id::unique("tasks-scrolled-to");
    let scroll = ui.data(|d| d.get_temp::<Option<usize>>(scrolled_id)) != Some(selected);
    ui.data_mut(|d| d.insert_temp(scrolled_id, selected));

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
            let format = TextFormat { strikethrough: Stroke::new(1.0, dim), ..TextFormat::simple(font.clone(), dim) };
            job.append(&title, 0.0, format);
        } else {
            job.append(&title, 0.0, TextFormat::simple(font.clone(), fg));
        }
        let painter = ui.painter_at(rect);
        let galley = painter.layout_job(job);
        let text_pos = pos2(rect.left() + char_w * 0.5, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, fg);

        if selected == Some(idx) {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5, color(outline)), StrokeKind::Inside);
            if scroll {
                ui.scroll_to_rect(rect.expand2(vec2(0.0, row_h * 2.0)), None);
            }
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
            DragAndDrop::set_payload(ui.ctx(), TaskDrag(idx));
        }
    }
    if editing.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    if let Some((idx, on_checkbox)) = clicked {
        if matches!(state.mode, Mode::EditTask { .. } | Mode::EditTitle { .. }) {
            state.apply(Command::Confirm, super::today());
        }
        state.click_task(idx, on_checkbox);
    }
}

/// While a task is being dragged, paint its title beside the pointer, above everything.
fn drag_ghost(ctx: &egui::Context, state: &AppState, theme: &Theme) {
    let Some(drag) = DragAndDrop::payload::<TaskDrag>(ctx) else { return };
    let (Some(task), Some(pointer)) = (state.tasks.items().get(drag.0), ctx.pointer_latest_pos()) else {
        return;
    };
    let font = TextStyle::Monospace.resolve(&ctx.global_style());
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("task-drag")));
    let (_, title) = task_row_text(task);
    let galley = painter.layout_no_wrap(title, font, color(theme.fg));
    let pad = 3.0;
    let rect = Rect::from_min_size(pointer + vec2(14.0, 4.0), galley.size()).expand(pad);
    painter.rect_filled(rect, 0.0, color(theme.bg));
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, color(theme.accent)), StrokeKind::Inside);
    painter.galley(rect.min + vec2(pad, pad), galley, color(theme.fg));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::Task;

    #[test]
    fn row_text_indents_and_marks() {
        let t = Task { title: "Draft".into(), done: true, depth: 2 };
        assert_eq!(task_row_text(&t), ("    [x] ".into(), "Draft".into()));
        let e = Task { title: String::new(), done: false, depth: 0 };
        assert_eq!(task_row_text(&e), ("[ ] ".into(), "(untitled)".into()));
    }
}
