//! Timeline drawing and mouse hit-testing.

use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, TextStyle, pos2, vec2};

use super::chrome::{color, faded};
use crate::model::{Kind, SLOTS_PER_DAY, Slot};
use crate::state::{AppState, Command, Mode};
use crate::theme::Theme;

/// The slot row containing screen y-coordinate `y`, clamped to the day.
pub fn y_to_slot(y: f32, top: f32, row_h: f32) -> Slot {
    let row = ((y - top) / row_h).floor();
    Slot(row.clamp(0.0, (SLOTS_PER_DAY - 1) as f32) as u16)
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme, scroll: bool) {
    let frame = egui::Frame::NONE.fill(color(theme.bg)).inner_margin(egui::Margin::symmetric(8, 4));
    egui::CentralPanel::default_margins().frame(frame).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            draw(ui, state, theme, scroll);
        });
    });
}

fn draw(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme, scroll: bool) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.text_style_height(&TextStyle::Monospace) + 4.0;
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, row_h * SLOTS_PER_DAY as f32), Sense::click_and_drag());

    handle_mouse(ui, state, &response, rect.top(), row_h);

    let painter = ui.painter_at(rect);
    let char_w = painter.layout_no_wrap("0".into(), font.clone(), color(theme.fg)).rect.width();
    let gutter = char_w * 8.0;
    let body_left = rect.left() + gutter;
    let row_rect = |from: u16, to: u16| {
        Rect::from_min_max(
            pos2(body_left, rect.top() + from as f32 * row_h),
            pos2(rect.right(), rect.top() + to as f32 * row_h),
        )
    };

    // Gutter labels and hour gridlines.
    for i in 0..SLOTS_PER_DAY {
        let y = rect.top() + i as f32 * row_h;
        let hour = i % 4 == 0;
        let label_color = if hour { theme.fg } else { theme.dim };
        let label = if hour || i % 2 == 0 { Slot(i).label() } else { "     ".into() };
        painter.text(
            pos2(rect.left(), y + row_h / 2.0),
            Align2::LEFT_CENTER,
            format!("{label} │"),
            font.clone(),
            color(label_color),
        );
        if hour && i > 0 {
            painter.hline(body_left..=rect.right(), y, Stroke::new(1.0, faded(theme.dim, 0.5)));
        }
    }

    // Pending create range.
    if let Some((start, end)) = state.pending_range() {
        painter.rect_filled(row_rect(start.0, end.0), 0.0, faded(theme.selection, 0.5));
    }

    // Blocks.
    let editing = match &state.mode {
        Mode::EditTitle { idx, .. } => Some(*idx),
        _ => None,
    };
    let blink_on = ui.input(|i| i.time) % 1.0 < 0.5;
    for (idx, block) in state.day.blocks().iter().enumerate() {
        let r = row_rect(block.start.0, block.end.0);
        let kc = color(theme.kind_color(block.kind));
        match block.kind {
            Kind::Standard => {
                painter.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(1.0, kc), StrokeKind::Inside);
            }
            Kind::Scheduled => shade(&painter, r, row_h, char_w, &font, faded(theme.scheduled, 0.45)),
            Kind::Deep => {
                painter.rect_filled(r, 0.0, faded(theme.deep, 0.35));
            }
        }
        painter.rect_filled(Rect::from_min_size(r.min, vec2(char_w * 0.5, r.height())), 0.0, kc);

        let tag = match block.kind {
            Kind::Standard => "[ ]",
            Kind::Scheduled => "[S]",
            Kind::Deep => "[D]",
        };
        let title = if editing == Some(idx) {
            format!("{}{}", state.title_buf, if blink_on { "█" } else { " " })
        } else if block.title.is_empty() {
            "(untitled)".into()
        } else {
            block.title.clone()
        };
        let text_y = r.top() + row_h / 2.0;
        let tag_pos = pos2(r.left() + char_w, text_y);
        painter.text(tag_pos, Align2::LEFT_CENTER, tag, font.clone(), kc);
        painter.text(
            pos2(tag_pos.x + char_w * 4.0, text_y),
            Align2::LEFT_CENTER,
            title,
            font.clone(),
            color(theme.fg),
        );
    }
    if editing.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    // Cursor.
    let cursor_rect = row_rect(state.cursor.0, state.cursor.0 + 1);
    painter.rect_stroke(cursor_rect, 0.0, Stroke::new(1.5, color(theme.accent)), StrokeKind::Inside);
    painter.text(
        pos2(rect.left() + char_w * 6.0, cursor_rect.center().y),
        Align2::LEFT_CENTER,
        "▶",
        font,
        color(theme.accent),
    );
    if scroll {
        let full_row = Rect::from_min_max(pos2(rect.left(), cursor_rect.top()), cursor_rect.max);
        ui.scroll_to_rect(full_row.expand2(vec2(0.0, row_h * 2.0)), None);
    }
}

/// Fills `r` with `░` characters, one line per row.
fn shade(painter: &egui::Painter, r: Rect, row_h: f32, char_w: f32, font: &FontId, c: egui::Color32) {
    let cols = ((r.width() - char_w) / char_w).max(0.0) as usize;
    let line: String = "░".repeat(cols);
    let mut y = r.top();
    while y + 0.5 < r.bottom() {
        painter.text(pos2(r.left() + char_w * 0.5, y + row_h / 2.0), Align2::LEFT_CENTER, &line, font.clone(), c);
        y += row_h;
    }
}

fn handle_mouse(ui: &egui::Ui, state: &mut AppState, response: &egui::Response, top: f32, row_h: f32) {
    let today = super::today();
    let finish_edit = |state: &mut AppState| {
        if matches!(state.mode, Mode::EditTitle { .. }) {
            state.apply(Command::Confirm, today);
        }
    };
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        finish_edit(state);
        state.drag_start(y_to_slot(pos.y, top, row_h));
        state.apply(Command::Cancel, today);
    }
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        finish_edit(state);
        state.drag_start(y_to_slot(origin.y, top, row_h));
    }
    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.drag_update(y_to_slot(pos.y, top, row_h));
    }
    if response.drag_stopped() {
        state.drag_end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_to_slot_maps_and_clamps() {
        assert_eq!(y_to_slot(100.0, 100.0, 20.0), Slot(0));
        assert_eq!(y_to_slot(159.0, 100.0, 20.0), Slot(2));
        assert_eq!(y_to_slot(50.0, 100.0, 20.0), Slot(0));
        assert_eq!(y_to_slot(1e6, 100.0, 20.0), Slot(95));
    }
}
