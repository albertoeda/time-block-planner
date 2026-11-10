//! Timeline drawing and mouse hit-testing.

use chrono::{Local, NaiveTime, Timelike};
use egui::{Align, Align2, Rect, Sense, Shape, Stroke, StrokeKind, TextStyle, pos2, vec2};

use super::chrome::{color, faded};
use crate::model::{Kind, SLOTS_PER_DAY, Slot};
use crate::state::{AppState, Command, Mode, START_SLOT};
use crate::theme::Theme;

/// The slot row containing screen y-coordinate `y`, clamped to the day.
pub fn y_to_slot(y: f32, top: f32, row_h: f32) -> Slot {
    let row = ((y - top) / row_h).floor();
    Slot(row.clamp(0.0, (SLOTS_PER_DAY - 1) as f32) as u16)
}

/// The kind's corner shape, so kinds read without relying on color alone.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Marker {
    Circle,
    Square,
    Triangle,
}

pub fn marker(k: Kind) -> Marker {
    match k {
        Kind::Standard => Marker::Circle,
        Kind::Scheduled => Marker::Square,
        Kind::Deep => Marker::Triangle,
    }
}

/// Fractional slots since midnight for wall-clock time `t`.
pub fn now_offset(t: NaiveTime) -> f32 {
    t.num_seconds_from_midnight() as f32 / 900.0
}

/// Drag-and-drop payload: the title of a task dragged onto the timeline (by value, so it
/// cannot go stale if the task list changes mid-drag).
#[derive(Clone)]
pub struct TaskDrag(pub String);

/// Where the timeline should scroll this frame.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    None,
    /// Keep the cursor row in view.
    Cursor,
    /// Put the `START_SLOT` row at the top (first frame).
    Start,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme, scroll: Scroll) {
    let frame = egui::Frame::NONE
        .fill(color(theme.bg))
        .inner_margin(egui::Margin::symmetric(8, 4));
    egui::CentralPanel::default_margins()
        .frame(frame)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    draw(ui, state, theme, scroll);
                });
        });
}

fn draw(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme, scroll: Scroll) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.text_style_height(&TextStyle::Monospace) + 4.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, row_h * SLOTS_PER_DAY as f32),
        Sense::click_and_drag(),
    );

    handle_mouse(ui, state, &response, rect.top(), row_h);

    // A task dropped from the task list is scheduled where it lands.
    let pointer_slot = ui
        .input(|i| i.pointer.latest_pos())
        .map(|p| y_to_slot(p.y, rect.top(), row_h));
    if let Some(payload) = response.dnd_release_payload::<TaskDrag>()
        && let Some(slot) = pointer_slot
    {
        state.drop_task(&payload.0, slot);
    }

    let painter = ui.painter_at(rect);
    let char_w = painter
        .layout_no_wrap("0".into(), font.clone(), color(theme.fg))
        .rect
        .width();
    let gutter = char_w * 8.0;
    let body_left = rect.left() + gutter;
    let row_rect = |from: u16, to: u16| {
        Rect::from_min_max(
            pos2(body_left, rect.top() + from as f32 * row_h),
            pos2(rect.right(), rect.top() + to as f32 * row_h),
        )
    };
    let accent = |w: f32| Stroke::new(w, color(theme.accent));

    // Gutter labels and hour gridlines (drawn first so the opaque blocks hide them).
    for i in 0..SLOTS_PER_DAY {
        let y = rect.top() + i as f32 * row_h;
        let hour = i % 4 == 0;
        let label_color = if hour { theme.fg } else { theme.dim };
        let label = if hour || i % 2 == 0 {
            Slot(i).label()
        } else {
            "     ".into()
        };
        painter.text(
            pos2(rect.left(), y + row_h / 2.0),
            Align2::LEFT_CENTER,
            format!("{label} │"),
            font.clone(),
            color(label_color),
        );
        if hour && i > 0 {
            painter.hline(
                body_left..=rect.right(),
                y,
                Stroke::new(1.0, faded(theme.dim, 0.5)),
            );
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
    let moving = match state.mode {
        Mode::MoveBlock { idx, .. } => Some(idx),
        _ => None,
    };
    let blink_on = ui.input(|i| i.time) % 1.0 < 0.5;
    let inset = char_w * 0.5;
    let mark = row_h * 0.45;
    for (idx, block) in state.day.blocks().iter().enumerate() {
        let r = row_rect(block.start.0, block.end.0);
        // 1 px shy top and bottom so adjacent blocks show a seam.
        let body = Rect::from_min_max(
            pos2(r.left(), r.top() + 1.0),
            pos2(r.right(), r.bottom() - 1.0),
        );
        let fill = theme.block_fill(block.kind);
        let alpha = if moving == Some(idx) { 0.5 } else { 1.0 };
        let ink = faded(theme.text_on(fill), alpha);
        painter.rect_filled(body, 0.0, faded(fill, alpha));

        // Corner marker, top-right.
        let m = Rect::from_min_size(
            pos2(body.right() - inset - mark, body.top() + inset),
            vec2(mark, mark),
        );
        match marker(block.kind) {
            Marker::Circle => {
                painter.circle_filled(m.center(), mark / 2.0, ink);
            }
            Marker::Square => {
                painter.rect_filled(m, 0.0, ink);
            }
            Marker::Triangle => {
                let points = vec![m.center_top(), m.right_bottom(), m.left_bottom()];
                painter.add(Shape::convex_polygon(points, ink, Stroke::NONE));
            }
        }

        let title = if editing == Some(idx) {
            format!("{}{}", state.title_buf, if blink_on { "█" } else { " " })
        } else if block.title.is_empty() {
            "(untitled)".into()
        } else {
            block.title.clone()
        };
        // Clipped to the block, stopping short of the marker.
        let clip = Rect::from_min_max(body.min, pos2(m.left() - inset, body.bottom()));
        painter.with_clip_rect(clip).text(
            pos2(r.left() + char_w, r.top() + row_h / 2.0),
            Align2::LEFT_CENTER,
            title,
            font.clone(),
            ink,
        );
    }
    if editing.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }

    // Move ghost: where the dragged block would land.
    if let Some((start, end)) = state.move_preview() {
        painter.rect_stroke(
            row_rect(start.0, end.0),
            0.0,
            accent(1.5),
            StrokeKind::Inside,
        );
    }

    // Task drop target: exactly the range `drop_task` would fill (nothing if occupied).
    if response.dnd_hover_payload::<TaskDrag>().is_some()
        && let Some((start, end)) = pointer_slot.and_then(|slot| state.drop_range(slot))
    {
        painter.rect_stroke(
            row_rect(start.0, end.0),
            0.0,
            accent(1.5),
            StrokeKind::Inside,
        );
    }

    // Now line.
    if state.date == super::today() {
        let now = Local::now().time();
        let y = rect.top() + now_offset(now) * row_h;
        let red = color(theme.error);
        painter.hline(body_left..=rect.right(), y, Stroke::new(2.0, red));
        painter.circle_filled(pos2(body_left, y), 4.0, red);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(60 - now.second() as u64));
    }

    // Cursor: outline the whole block it is in, or just its row on a free slot.
    let cursor_rect = row_rect(state.cursor.0, state.cursor.0 + 1);
    match state.day.block_at(state.cursor) {
        Some(i) => {
            let b = &state.day.blocks()[i];
            painter.rect_stroke(
                row_rect(b.start.0, b.end.0),
                0.0,
                accent(2.0),
                StrokeKind::Inside,
            );
        }
        None => {
            painter.rect_stroke(cursor_rect, 0.0, accent(1.5), StrokeKind::Inside);
        }
    }
    painter.text(
        pos2(rect.left() + char_w * 6.0, cursor_rect.center().y),
        Align2::LEFT_CENTER,
        "▶",
        font,
        color(theme.accent),
    );
    match scroll {
        Scroll::None => {}
        Scroll::Cursor => {
            let full_row =
                Rect::from_min_max(pos2(rect.left(), cursor_rect.top()), cursor_rect.max);
            ui.scroll_to_rect(full_row.expand2(vec2(0.0, row_h * 2.0)), None);
        }
        Scroll::Start => {
            let r = row_rect(START_SLOT.0, START_SLOT.0 + 1);
            ui.scroll_to_rect(
                Rect::from_min_max(pos2(rect.left(), r.top()), r.max),
                Some(Align::Min),
            );
        }
    }
}

fn handle_mouse(
    ui: &egui::Ui,
    state: &mut AppState,
    response: &egui::Response,
    top: f32,
    row_h: f32,
) {
    let today = super::today();
    let finish_edit = |state: &mut AppState| {
        if matches!(state.mode, Mode::EditTitle { .. } | Mode::EditTask { .. }) {
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

    #[test]
    fn markers_are_distinct() {
        assert_eq!(
            [
                marker(Kind::Standard),
                marker(Kind::Scheduled),
                marker(Kind::Deep)
            ],
            [Marker::Circle, Marker::Square, Marker::Triangle]
        );
    }

    #[test]
    fn now_offset_in_slots() {
        assert_eq!(now_offset(NaiveTime::from_hms_opt(8, 30, 0).unwrap()), 34.0);
        assert_eq!(now_offset(NaiveTime::from_hms_opt(0, 7, 30).unwrap()), 0.5);
    }
}
