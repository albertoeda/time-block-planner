//! Application state machine. Keyboard (via `Command`) and mouse (via the drag API) both land
//! here; every mutation of the day is autosaved and checkpointed for undo.

use chrono::NaiveDate;

use crate::model::{Block, Day, EditError, Kind, SLOTS_PER_DAY, Slot};
use crate::storage::Store;
use crate::theme::{theme_index, themes};

const HISTORY_CAP: usize = 100;
const LAST_SLOT: u16 = SLOTS_PER_DAY - 1;
const START_OF_WORKDAY: Slot = Slot(36); // 09:00

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Normal,
    Create { anchor: Slot },
    EditTitle { idx: usize, original: String, is_new: bool },
    ThemePicker { original: usize },
    Help,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    PrevDay,
    NextDay,
    Today,
    NextBlock,
    PrevBlock,
    NewBlock,
    ExtendUp,
    ExtendDown,
    EditTitle,
    SetKind(Kind),
    MoveUp,
    MoveDown,
    Grow,
    Shrink,
    Delete,
    Undo,
    Redo,
    SaveNow,
    OpenThemes,
    ToggleHelp,
    Confirm,
    Cancel,
    Text(String),
    Backspace,
}

pub struct AppState {
    pub store: Store,
    pub date: NaiveDate,
    pub day: Day,
    /// Always in `0..=95`.
    pub cursor: Slot,
    pub mode: Mode,
    pub theme: usize,
    pub title_buf: String,
    pub status: String,
    history: Vec<(NaiveDate, Day)>,
    future: Vec<(NaiveDate, Day)>,
    /// The current date's file could not be read; never write it, or we'd destroy it.
    load_failed: bool,
}

impl AppState {
    pub fn new(store: Store, today: NaiveDate) -> AppState {
        let theme = theme_index(&store.load_theme().unwrap_or_default());
        let mut s = AppState {
            store,
            date: today,
            day: Day::default(),
            cursor: START_OF_WORKDAY,
            mode: Mode::Normal,
            theme,
            title_buf: String::new(),
            status: String::new(),
            history: Vec::new(),
            future: Vec::new(),
            load_failed: false,
        };
        s.load_current();
        s
    }

    pub fn selected(&self) -> Option<usize> {
        self.day.block_at(self.cursor)
    }

    pub fn pending_range(&self) -> Option<(Slot, Slot)> {
        match self.mode {
            Mode::Create { anchor } => self.day.free_range(anchor, self.cursor),
            _ => None,
        }
    }

    pub fn save(&mut self) {
        if self.load_failed {
            self.status = format!("not saving: {} could not be read", self.date);
            return;
        }
        if let Err(e) = self.store.save_day(self.date, &self.day) {
            self.status = format!("save failed: {e}");
        }
    }

    pub fn drag_start(&mut self, slot: Slot) {
        if self.mode != Mode::Normal {
            return;
        }
        self.cursor = clamp_cursor(slot.0 as i32);
        if self.selected().is_none() {
            self.mode = Mode::Create { anchor: self.cursor };
        }
    }

    pub fn drag_update(&mut self, slot: Slot) {
        if matches!(self.mode, Mode::Create { .. }) {
            self.cursor = clamp_cursor(slot.0 as i32);
        }
    }

    pub fn drag_end(&mut self) {
        if matches!(self.mode, Mode::Create { .. }) {
            self.confirm_create();
        }
    }

    pub fn apply(&mut self, cmd: Command, today: NaiveDate) {
        self.status.clear();
        match self.mode.clone() {
            Mode::Normal => self.apply_normal(cmd, today),
            Mode::Create { .. } => match cmd {
                Command::Up | Command::ExtendUp => self.move_cursor(-1),
                Command::Down | Command::ExtendDown => self.move_cursor(1),
                Command::Confirm => self.confirm_create(),
                Command::Cancel => self.mode = Mode::Normal,
                _ => {}
            },
            Mode::EditTitle { idx, original, is_new } => match cmd {
                Command::Text(t) => self.title_buf.extend(t.chars().filter(|c| !c.is_control())),
                Command::Backspace => {
                    self.title_buf.pop();
                }
                Command::Confirm => {
                    if !is_new && self.title_buf.trim() != original {
                        self.checkpoint();
                    }
                    let buf = std::mem::take(&mut self.title_buf);
                    self.day.set_title(idx, &buf);
                    self.mode = Mode::Normal;
                    self.save();
                }
                Command::Cancel => {
                    if is_new && let Some((_, day)) = self.history.pop() {
                        self.day = day;
                        self.save();
                    }
                    self.title_buf.clear();
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::ThemePicker { original } => {
                let n = themes().len();
                match cmd {
                    Command::Up => self.theme = (self.theme + n - 1) % n,
                    Command::Down => self.theme = (self.theme + 1) % n,
                    Command::Confirm => {
                        if let Err(e) = self.store.save_theme(themes()[self.theme].name) {
                            self.status = format!("save failed: {e}");
                        }
                        self.mode = Mode::Normal;
                    }
                    Command::Cancel => {
                        self.theme = original;
                        self.mode = Mode::Normal;
                    }
                    _ => {}
                }
            }
            Mode::Help => {
                if matches!(cmd, Command::ToggleHelp | Command::Cancel) {
                    self.mode = Mode::Normal;
                }
            }
        }
    }

    fn apply_normal(&mut self, cmd: Command, today: NaiveDate) {
        match cmd {
            Command::Up => self.move_cursor(-1),
            Command::Down => self.move_cursor(1),
            Command::PageUp => self.move_cursor(-4),
            Command::PageDown => self.move_cursor(4),
            Command::Home => self.cursor = Slot(0),
            Command::End => self.cursor = Slot(LAST_SLOT),
            Command::PrevDay => self.go_to(self.date.pred_opt()),
            Command::NextDay => self.go_to(self.date.succ_opt()),
            Command::Today => self.go_to(Some(today)),
            Command::NextBlock => {
                if let Some(b) = self.day.blocks().iter().find(|b| b.start > self.cursor) {
                    self.cursor = b.start;
                }
            }
            Command::PrevBlock => {
                if let Some(b) = self.day.blocks().iter().rev().find(|b| b.start < self.cursor) {
                    self.cursor = b.start;
                }
            }
            Command::NewBlock | Command::ExtendDown | Command::ExtendUp => {
                if self.selected().is_some() {
                    self.status = "slot occupied".into();
                    return;
                }
                self.mode = Mode::Create { anchor: self.cursor };
                match cmd {
                    Command::ExtendDown => self.move_cursor(1),
                    Command::ExtendUp => self.move_cursor(-1),
                    _ => {}
                }
            }
            Command::EditTitle => {
                let Some(idx) = self.selected_or_status() else { return };
                let original = self.day.blocks()[idx].title.clone();
                self.title_buf = original.clone();
                self.mode = Mode::EditTitle { idx, original, is_new: false };
            }
            Command::SetKind(k) => self.edit(|d, i| {
                d.set_kind(i, k);
                Ok(i)
            }),
            Command::MoveUp => self.edit(|d, i| d.shift(i, -1)),
            Command::MoveDown => self.edit(|d, i| d.shift(i, 1)),
            Command::Grow => self.edit(|d, i| d.resize_end(i, 1).map(|_| i)),
            Command::Shrink => self.edit(|d, i| d.resize_end(i, -1).map(|_| i)),
            Command::Delete => self.edit(|d, i| {
                d.remove(i);
                Ok(usize::MAX)
            }),
            Command::Undo => self.time_travel(true),
            Command::Redo => self.time_travel(false),
            Command::SaveNow => {
                self.save();
                if self.status.is_empty() {
                    self.status = "saved".into();
                }
            }
            Command::OpenThemes => self.mode = Mode::ThemePicker { original: self.theme },
            Command::ToggleHelp => self.mode = Mode::Help,
            Command::Confirm | Command::Cancel | Command::Text(_) | Command::Backspace => {}
        }
    }

    fn selected_or_status(&mut self) -> Option<usize> {
        let sel = self.selected();
        if sel.is_none() {
            self.status = "no block here".into();
        }
        sel
    }

    /// Applies an edit to the block under the cursor. On success it checkpoints the prior day,
    /// keeps the cursor on the edited block (`usize::MAX` = block removed), and saves.
    fn edit(&mut self, op: impl FnOnce(&mut Day, usize) -> Result<usize, EditError>) {
        let Some(idx) = self.selected_or_status() else { return };
        let before = self.day.clone();
        let offset = self.cursor.0 - self.day.blocks()[idx].start.0;
        match op(&mut self.day, idx) {
            Ok(new_idx) => {
                self.push_history(before);
                if let Some(b) = self.day.blocks().get(new_idx) {
                    let last = b.end.0 - 1;
                    self.cursor = Slot((b.start.0 + offset).min(last));
                }
                self.save();
            }
            Err(e) => {
                self.status = match e {
                    EditError::Overlap => "overlaps another block",
                    EditError::OutOfBounds => "out of day bounds",
                    EditError::Empty => "block can't be empty",
                }
                .into();
            }
        }
    }

    fn confirm_create(&mut self) {
        let range = self.pending_range();
        self.mode = Mode::Normal;
        let Some((start, end)) = range else { return };
        self.checkpoint();
        let block = Block { start, end, kind: Kind::Standard, title: String::new() };
        match self.day.add(block) {
            Ok(idx) => {
                self.cursor = start;
                self.title_buf.clear();
                self.mode = Mode::EditTitle { idx, original: String::new(), is_new: true };
                self.save();
            }
            Err(_) => {
                self.history.pop();
                self.status = "slot occupied".into();
            }
        }
    }

    fn checkpoint(&mut self) {
        self.push_history(self.day.clone());
    }

    fn push_history(&mut self, day: Day) {
        if self.history.len() == HISTORY_CAP {
            self.history.remove(0);
        }
        self.history.push((self.date, day));
        self.future.clear();
    }

    fn time_travel(&mut self, undo: bool) {
        let popped = if undo { self.history.pop() } else { self.future.pop() };
        let Some((date, day)) = popped else {
            self.status = if undo { "nothing to undo" } else { "nothing to redo" }.into();
            return;
        };
        // Switch to the snapshot's day first so the opposite stack records that day's state.
        // Every edit is already saved, so leaving the current day needs no save.
        if date != self.date {
            self.date = date;
            self.load_current();
        }
        let current = (self.date, self.day.clone());
        if undo { self.future.push(current) } else { self.history.push(current) }
        self.day = day;
        self.save();
    }

    /// Edits are saved as they happen, so changing days never writes the day being left.
    fn go_to(&mut self, date: Option<NaiveDate>) {
        let Some(date) = date else { return };
        if date == self.date {
            return;
        }
        self.date = date;
        self.load_current();
    }

    fn load_current(&mut self) {
        match self.store.load_day(self.date) {
            Ok(day) => {
                self.day = day;
                self.load_failed = false;
            }
            Err(e) => {
                self.status = format!("load failed: {e}");
                self.day = Day::default();
                self.load_failed = true;
            }
        }
    }

    fn move_cursor(&mut self, delta: i32) {
        self.cursor = clamp_cursor(self.cursor.0 as i32 + delta);
    }
}

fn clamp_cursor(v: i32) -> Slot {
    Slot(v.clamp(0, LAST_SLOT as i32) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Kind, Slot};
    use crate::theme::themes;

    const D: NaiveDate = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();

    fn st() -> (tempfile::TempDir, AppState) {
        let tmp = tempfile::tempdir().unwrap();
        let s = AppState::new(Store { dir: tmp.path().to_path_buf() }, D);
        (tmp, s)
    }

    fn st_with_block(s0: u16, e0: u16) -> (tempfile::TempDir, AppState) {
        let (tmp, mut s) = st();
        s.day
            .add(Block { start: Slot(s0), end: Slot(e0), kind: Kind::Standard, title: "a".into() })
            .unwrap();
        s.save();
        s.cursor = Slot(s0);
        (tmp, s)
    }

    #[test]
    fn keyboard_create_flow() {
        let (_t, mut s) = st();
        s.cursor = Slot(36);
        s.apply(Command::ExtendDown, D);
        s.apply(Command::ExtendDown, D);
        s.apply(Command::Down, D);
        assert_eq!(s.pending_range(), Some((Slot(36), Slot(40))));
        s.apply(Command::Confirm, D);
        assert!(matches!(s.mode, Mode::EditTitle { is_new: true, .. }));
        s.apply(Command::Text("Deep sync".into()), D);
        s.apply(Command::Confirm, D);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.day.blocks()[0].title, "Deep sync");
        assert_eq!(s.store.load_day(D).unwrap(), s.day);
    }

    #[test]
    fn edit_mode_swallows_commands() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::EditTitle, D);
        for c in [
            Command::Delete,
            Command::SetKind(Kind::Deep),
            Command::NextDay,
            Command::ToggleHelp,
            Command::Undo,
        ] {
            s.apply(c, D);
        }
        assert_eq!(s.day.blocks().len(), 1);
        assert_eq!(s.day.blocks()[0].kind, Kind::Standard);
        assert_eq!(s.date, D);
        assert!(matches!(s.mode, Mode::EditTitle { .. }));
    }

    #[test]
    fn cancel_new_block_removes_it() {
        let (t, mut s) = st();
        s.cursor = Slot(10);
        s.apply(Command::NewBlock, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Text("x".into()), D);
        s.apply(Command::Cancel, D);
        assert!(s.day.blocks().is_empty());
        assert!(!t.path().join("2026-11-10.txt").exists());
        s.apply(Command::Undo, D);
        assert_eq!(s.status, "nothing to undo");
    }

    #[test]
    fn cancel_edit_restores_title() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::EditTitle, D);
        s.apply(Command::Backspace, D);
        s.apply(Command::Text("b".into()), D);
        s.apply(Command::Cancel, D);
        assert_eq!(s.day.blocks()[0].title, "a");
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn set_kind_persists() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::SetKind(Kind::Scheduled), D);
        assert_eq!(s.day.blocks()[0].kind, Kind::Scheduled);
        assert_eq!(s.store.load_day(D).unwrap().blocks()[0].kind, Kind::Scheduled);
    }

    #[test]
    fn move_refusal_sets_status() {
        let (_t, mut s) = st_with_block(36, 40);
        s.day
            .add(Block { start: Slot(40), end: Slot(44), kind: Kind::Standard, title: String::new() })
            .unwrap();
        s.cursor = Slot(36);
        s.apply(Command::MoveDown, D);
        assert_eq!(s.status, "overlaps another block");
        assert_eq!(s.day.blocks()[0].start, Slot(36));
        s.apply(Command::Undo, D);
        assert_eq!(s.status, "nothing to undo");
    }

    #[test]
    fn move_follows_block_with_cursor() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::MoveDown, D);
        assert_eq!(s.day.blocks()[0].start, Slot(37));
        assert_eq!(s.selected(), Some(0));
        s.apply(Command::Grow, D);
        assert_eq!(s.day.blocks()[0].end, Slot(42));
    }

    #[test]
    fn undo_redo_delete() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::Delete, D);
        assert!(s.day.blocks().is_empty());
        s.apply(Command::Undo, D);
        assert_eq!(s.day.blocks().len(), 1);
        assert_eq!(s.store.load_day(D).unwrap().blocks().len(), 1);
        s.apply(Command::Redo, D);
        assert!(s.day.blocks().is_empty());
    }

    #[test]
    fn undo_switches_back_to_day() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::Delete, D);
        s.apply(Command::NextDay, D);
        s.apply(Command::Undo, D);
        assert_eq!(s.date, D);
        assert_eq!(s.day.blocks().len(), 1);
    }

    #[test]
    fn redo_across_days() {
        let (t, mut s) = st_with_block(36, 40);
        s.apply(Command::Delete, D);
        s.apply(Command::NextDay, D);
        s.apply(Command::Undo, D);
        assert_eq!(s.day.blocks().len(), 1);
        s.apply(Command::Redo, D);
        assert_eq!(s.date, D);
        assert!(s.day.blocks().is_empty());
        assert!(!t.path().join("2026-11-10.txt").exists());
    }

    #[test]
    fn unreadable_day_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("2026-11-10.txt");
        let bytes = [0xffu8, 0xfe, b'x', b'\n'];
        std::fs::write(&file, bytes).unwrap();
        let mut s = AppState::new(Store { dir: tmp.path().to_path_buf() }, D);
        assert!(s.status.starts_with("load failed"), "{}", s.status);
        s.cursor = Slot(10);
        s.apply(Command::NewBlock, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Confirm, D);
        assert!(s.status.starts_with("not saving"), "{}", s.status);
        s.apply(Command::NextDay, D);
        s.apply(Command::PrevDay, D);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }

    #[test]
    fn navigation_does_not_rewrite_files() {
        let (t, mut s) = st_with_block(36, 40);
        let file = t.path().join("2026-11-10.txt");
        std::fs::write(&file, "10:00-11:00 deep edited elsewhere\n").unwrap();
        s.apply(Command::NextDay, D);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "10:00-11:00 deep edited elsewhere\n");
    }

    #[test]
    fn day_navigation_saves_and_loads() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::NextDay, D);
        assert_eq!(s.date, D.succ_opt().unwrap());
        assert!(s.day.blocks().is_empty());
        assert_eq!(s.cursor, Slot(36));
        s.apply(Command::Today, D);
        assert_eq!(s.day.blocks().len(), 1);
    }

    #[test]
    fn cursor_clamps() {
        let (_t, mut s) = st();
        s.apply(Command::Home, D);
        s.apply(Command::Up, D);
        assert_eq!(s.cursor, Slot(0));
        s.apply(Command::PageDown, D);
        assert_eq!(s.cursor, Slot(4));
        s.apply(Command::End, D);
        s.apply(Command::Down, D);
        assert_eq!(s.cursor, Slot(95));
    }

    #[test]
    fn next_prev_block_jump() {
        let (_t, mut s) = st_with_block(10, 12);
        s.day
            .add(Block { start: Slot(40), end: Slot(44), kind: Kind::Deep, title: String::new() })
            .unwrap();
        s.cursor = Slot(0);
        s.apply(Command::NextBlock, D);
        assert_eq!(s.cursor, Slot(10));
        s.apply(Command::NextBlock, D);
        assert_eq!(s.cursor, Slot(40));
        s.apply(Command::PrevBlock, D);
        assert_eq!(s.cursor, Slot(10));
    }

    #[test]
    fn theme_picker_preview_confirm_cancel() {
        let (_t, mut s) = st();
        let start = s.theme;
        assert_eq!(themes()[start].name, "tokyo-night");
        s.apply(Command::OpenThemes, D);
        s.apply(Command::Down, D);
        assert_ne!(s.theme, start);
        s.apply(Command::Cancel, D);
        assert_eq!(s.theme, start);
        s.apply(Command::OpenThemes, D);
        s.apply(Command::Up, D);
        s.apply(Command::Confirm, D);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.store.load_theme().as_deref(), Some(themes()[s.theme].name));
    }

    #[test]
    fn drag_upward_normalizes() {
        let (_t, mut s) = st();
        s.drag_start(Slot(44));
        s.drag_update(Slot(40));
        s.drag_end();
        assert_eq!((s.day.blocks()[0].start, s.day.blocks()[0].end), (Slot(40), Slot(45)));
    }

    #[test]
    fn drag_clips_at_neighbor() {
        let (_t, mut s) = st_with_block(40, 44);
        s.drag_start(Slot(36));
        s.drag_update(Slot(50));
        s.drag_end();
        assert_eq!(s.day.block_at(Slot(36)).map(|i| s.day.blocks()[i].end), Some(Slot(40)));
        s.apply(Command::Cancel, D);
        s.drag_start(Slot(41));
        s.drag_end();
        assert_eq!(s.day.blocks().len(), 1);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn save_error_sets_status() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("file");
        std::fs::write(&f, "").unwrap();
        let mut s = AppState::new(Store { dir: f }, D);
        s.apply(Command::NewBlock, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Confirm, D);
        assert!(s.status.starts_with("save failed"), "{}", s.status);
        assert_eq!(s.day.blocks().len(), 1);
    }
}
