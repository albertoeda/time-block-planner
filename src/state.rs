//! Application state machine. Keyboard (via `Command`) and mouse (via the drag API) both land
//! here; every mutation of the day is autosaved and checkpointed for undo.

use chrono::NaiveDate;

use crate::model::{Block, Day, EditError, Kind, SLOTS_PER_DAY, Slot};
use crate::storage::Store;
use crate::tasks::TaskList;
use crate::theme::{theme_index, themes};

const HISTORY_CAP: usize = 100;
const LAST_SLOT: u16 = SLOTS_PER_DAY - 1;
pub const START_SLOT: Slot = Slot(32); // 08:00

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Focus {
    Tasks,
    Timeline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Normal,
    Create {
        anchor: Slot,
        title: Option<String>,
    },
    EditTitle {
        idx: usize,
        original: String,
        is_new: bool,
    },
    EditTask {
        idx: usize,
        original: String,
        is_new: bool,
    },
    MoveBlock {
        idx: usize,
        grab: u16,
        target: Slot,
    },
    ThemePicker {
        original: usize,
    },
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
    New,
    NewChild,
    ExtendUp,
    ExtendDown,
    EditTitle,
    SetKind(Kind),
    MoveUp,
    MoveDown,
    Grow,
    Shrink,
    Delete,
    ToggleDone,
    Indent,
    Outdent,
    ScheduleTask,
    /// Move the selected task (and its subtasks) to the previous / next day's list.
    MoveTaskToPrevDay,
    MoveTaskToNextDay,
    /// Focus the given pane.
    FocusPane(Focus),
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
    pub focus: Focus,
    pub tasks: TaskList,
    pub task_sel: usize,
    history: Vec<Snapshot>,
    future: Vec<Snapshot>,
    /// The current date's file could not be read; never write it, or we'd destroy it.
    load_failed: bool,
}

/// One undoable state: a day's blocks (with its date), or the task lists of one or more days.
/// For `Tasks`, the first entry is the day to show after undo/redo.
enum Snapshot {
    Day(NaiveDate, Day),
    Tasks(Vec<(NaiveDate, TaskList)>),
}

impl AppState {
    pub fn new(store: Store, today: NaiveDate) -> AppState {
        let theme = theme_index(&store.load_theme().unwrap_or_default());
        let mut s = AppState {
            store,
            date: today,
            day: Day::default(),
            cursor: START_SLOT,
            mode: Mode::Normal,
            theme,
            title_buf: String::new(),
            status: String::new(),
            focus: Focus::Timeline,
            tasks: TaskList::default(),
            task_sel: 0,
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

    /// The selected task, or `None` when the "+ new task" row (index `len`) is selected.
    pub fn selected_task(&self) -> Option<usize> {
        (self.task_sel < self.tasks.len()).then_some(self.task_sel)
    }

    /// Whether the "+ new task" row after the last task is selected.
    pub fn new_task_row_selected(&self) -> bool {
        self.task_sel >= self.tasks.len()
    }

    pub fn pending_range(&self) -> Option<(Slot, Slot)> {
        match self.mode {
            Mode::Create { anchor, .. } => self.day.free_range(anchor, self.cursor),
            _ => None,
        }
    }

    /// While dragging a block: where it would land, as `(start, end)`.
    pub fn move_preview(&self) -> Option<(Slot, Slot)> {
        match self.mode {
            Mode::MoveBlock { idx, target, .. } => {
                let b = &self.day.blocks()[idx];
                Some((target, Slot(target.0 + (b.end.0 - b.start.0))))
            }
            _ => None,
        }
    }

    /// Where a task dropped at `slot` would land: up to one hour, clipped by neighbours and the
    /// day end. `None` when `slot` is occupied. Shared by `drop_task` and the hover preview.
    pub fn drop_range(&self, slot: Slot) -> Option<(Slot, Slot)> {
        self.day.free_range(slot, Slot((slot.0 + 3).min(LAST_SLOT)))
    }

    /// Drops a task on the timeline: a one-hour block (clipped by neighbours and the day end).
    pub fn drop_task(&mut self, title: &str, slot: Slot) {
        if self.mode != Mode::Normal {
            return;
        }
        let Some((start, end)) = self.drop_range(slot) else {
            self.status = "slot occupied".into();
            return;
        };
        self.checkpoint();
        let block = Block {
            start,
            end,
            kind: Kind::Standard,
            title: title.to_string(),
        };
        match self.day.add(block) {
            Ok(_) => {
                self.cursor = start;
                self.focus = Focus::Timeline;
                self.save();
            }
            Err(_) => {
                self.history.pop();
                self.status = "slot occupied".into();
            }
        }
    }

    /// A mouse click on task `idx`: focus the tasks pane and select it. `on_checkbox` also
    /// toggles `done` (undoable, saved). Ignored outside Normal mode or for a bad index.
    pub fn click_task(&mut self, idx: usize, on_checkbox: bool) {
        if self.mode != Mode::Normal || idx >= self.tasks.len() {
            return;
        }
        self.focus = Focus::Tasks;
        self.task_sel = idx;
        if on_checkbox {
            self.edit_task(|t, i| {
                t.toggle_done(i);
                Some(i)
            });
        }
    }

    /// The "+ new task" row: a new top-level task after the last one, ready to type.
    pub fn new_task_at_end(&mut self) {
        if self.mode != Mode::Normal {
            return;
        }
        self.focus = Focus::Tasks;
        self.insert_task(None, false);
    }

    /// Inserts an empty task (see `TaskList::insert`) and starts editing its title.
    fn insert_task(&mut self, after: Option<usize>, child: bool) {
        self.checkpoint_tasks();
        let idx = self.tasks.insert(after, child);
        self.task_sel = idx;
        self.title_buf.clear();
        self.mode = Mode::EditTask {
            idx,
            original: String::new(),
            is_new: true,
        };
    }

    pub fn save(&mut self) {
        if self.load_failed {
            self.status = format!("not saving: {} could not be read", self.date);
            return;
        }
        if let Err(e) = self.store.save_day(self.date, &self.day, &self.tasks) {
            self.status = format!("save failed: {e}");
        }
    }

    pub fn drag_start(&mut self, slot: Slot) {
        if self.mode != Mode::Normal {
            return;
        }
        self.focus = Focus::Timeline;
        self.cursor = clamp_cursor(slot.0 as i32);
        self.mode = match self.selected() {
            None => Mode::Create {
                anchor: self.cursor,
                title: None,
            },
            Some(idx) => {
                let start = self.day.blocks()[idx].start;
                Mode::MoveBlock {
                    idx,
                    grab: self.cursor.0 - start.0,
                    target: start,
                }
            }
        };
    }

    pub fn drag_update(&mut self, slot: Slot) {
        match &mut self.mode {
            Mode::Create { .. } => self.cursor = clamp_cursor(slot.0 as i32),
            Mode::MoveBlock { idx, grab, target } => {
                let b = &self.day.blocks()[*idx];
                let len = (b.end.0 - b.start.0) as i32;
                let max = SLOTS_PER_DAY as i32 - len;
                *target = Slot((slot.0 as i32 - *grab as i32).clamp(0, max) as u16);
            }
            _ => {}
        }
    }

    pub fn drag_end(&mut self) {
        match self.mode {
            Mode::Create { .. } => self.confirm_create(),
            Mode::MoveBlock { idx, target, .. } => {
                self.mode = Mode::Normal;
                let start = self.day.blocks()[idx].start;
                if target != start {
                    self.cursor = start;
                    let delta = target.0 as i32 - start.0 as i32;
                    self.edit(|d, i| d.shift(i, delta));
                }
            }
            _ => {}
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
            Mode::EditTitle {
                idx,
                original,
                is_new,
            } => match cmd {
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
                    if is_new && let Some(Snapshot::Day(_, day)) = self.history.pop() {
                        self.day = day;
                        self.save();
                    }
                    self.title_buf.clear();
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::EditTask {
                idx,
                original,
                is_new,
            } => match cmd {
                Command::Text(t) => self.title_buf.extend(t.chars().filter(|c| !c.is_control())),
                Command::Backspace => {
                    self.title_buf.pop();
                }
                Command::Confirm => {
                    if !is_new && self.title_buf.trim() != original {
                        self.checkpoint_tasks();
                    }
                    let buf = std::mem::take(&mut self.title_buf);
                    self.tasks.set_title(idx, &buf);
                    self.mode = Mode::Normal;
                    self.save();
                }
                Command::Cancel => {
                    if is_new && let Some(Snapshot::Tasks(mut saved)) = self.history.pop() {
                        self.tasks = saved.remove(0).1;
                        self.save();
                    }
                    self.title_buf.clear();
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::MoveBlock { .. } => {
                if cmd == Command::Cancel {
                    self.mode = Mode::Normal;
                }
            }
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
            Command::PrevDay => self.go_to(self.date.pred_opt()),
            Command::NextDay => self.go_to(self.date.succ_opt()),
            Command::Today => self.go_to(Some(today)),
            Command::Undo => self.time_travel(true),
            Command::Redo => self.time_travel(false),
            Command::SaveNow => {
                self.save();
                if self.status.is_empty() {
                    self.status = "saved".into();
                }
            }
            Command::OpenThemes => {
                self.mode = Mode::ThemePicker {
                    original: self.theme,
                }
            }
            Command::ToggleHelp => self.mode = Mode::Help,
            Command::FocusPane(f) => self.focus = f,
            _ => match self.focus {
                Focus::Tasks => self.apply_tasks(cmd),
                Focus::Timeline => self.apply_timeline(cmd),
            },
        }
    }

    fn apply_tasks(&mut self, cmd: Command) {
        // Rows are the tasks plus the "+ new task" row at index `len`.
        let new_row = self.tasks.len() as i32;
        let sel = self.task_sel.min(self.tasks.len()) as i32;
        let go = |s: &mut AppState, v: i32| s.task_sel = v.clamp(0, new_row) as usize;
        match cmd {
            Command::Up => go(self, sel - 1),
            Command::Down => go(self, sel + 1),
            Command::PageUp => go(self, sel - 10),
            Command::PageDown => go(self, sel + 10),
            Command::Home => go(self, 0),
            Command::End => go(self, new_row - 1),
            Command::EditTitle if self.new_task_row_selected() => self.insert_task(None, false),
            Command::New | Command::NewChild => {
                self.insert_task(self.selected_task(), cmd == Command::NewChild)
            }
            Command::ScheduleTask => {
                let Some(idx) = self.selected_task() else {
                    return;
                };
                self.focus = Focus::Timeline;
                if self.selected().is_some() {
                    self.status = "slot occupied".into();
                    return;
                }
                let title = self.tasks.items()[idx].title.clone();
                self.mode = Mode::Create {
                    anchor: self.cursor,
                    title: Some(title),
                };
            }
            Command::EditTitle => {
                let Some(idx) = self.selected_task() else {
                    return;
                };
                let original = self.tasks.items()[idx].title.clone();
                self.title_buf = original.clone();
                self.mode = Mode::EditTask {
                    idx,
                    original,
                    is_new: false,
                };
            }
            Command::MoveTaskToPrevDay => self.move_task_to_day(self.date.pred_opt()),
            Command::MoveTaskToNextDay => self.move_task_to_day(self.date.succ_opt()),
            Command::Delete => self.edit_task(|t, i| {
                t.remove(i);
                Some(i)
            }),
            Command::ToggleDone => self.edit_task(|t, i| {
                t.toggle_done(i);
                Some(i)
            }),
            Command::Indent => self.edit_task(|t, i| t.indent(i).then_some(i)),
            Command::Outdent => self.edit_task(|t, i| t.outdent(i)),
            Command::MoveUp => self.edit_task(|t, i| t.move_up(i)),
            Command::MoveDown => self.edit_task(|t, i| t.move_down(i)),
            _ => {}
        }
    }

    /// Applies an edit to the selected task. `op` returns the item's new index, or `None` to
    /// refuse. On success it checkpoints the prior list, keeps the selection on the item, saves.
    fn edit_task(&mut self, op: impl FnOnce(&mut TaskList, usize) -> Option<usize>) {
        let Some(idx) = self.selected_task() else {
            return;
        };
        let before = self.tasks.clone();
        match op(&mut self.tasks, idx) {
            Some(new_idx) => {
                self.push_history(Snapshot::Tasks(vec![(self.date, before)]));
                // Stay on a task (e.g. after deleting the last one) unless none are left.
                self.task_sel = new_idx.min(self.tasks.len().saturating_sub(1));
                self.save();
            }
            None => self.status = "can't move there".into(),
        }
    }

    /// Moves the selected task and its subtasks to the end of another day's list, staying on
    /// the current day. One undo step restores both days. The other day is written first, so
    /// a failure part-way can duplicate the task but never lose it.
    fn move_task_to_day(&mut self, target: Option<NaiveDate>) {
        let Some(idx) = self.selected_task() else {
            return;
        };
        let Some(target) = target else {
            self.status = "no such day".into();
            return;
        };
        if self.load_failed {
            self.status = format!("not saving: {} could not be read", self.date);
            return;
        }
        let (other_day, mut other) = match self.store.load_day(target) {
            Ok(loaded) => loaded,
            Err(e) => {
                self.status = format!("load failed: {e}");
                return;
            }
        };
        let before = self.tasks.clone();
        let saved = vec![(self.date, before.clone()), (target, other.clone())];
        let sub = self.tasks.take_subtree(idx);
        other.append_subtree(sub);
        if let Err(e) = self.store.save_day(target, &other_day, &other) {
            self.tasks = before;
            self.status = format!("save failed: {e}");
            return;
        }
        self.push_history(Snapshot::Tasks(saved));
        self.task_sel = idx.min(self.tasks.len().saturating_sub(1));
        self.save();
        if self.status.starts_with("save failed") {
            self.status = format!(
                "save failed (task also copied to {target}): {}",
                self.status.trim_start_matches("save failed: ")
            );
        }
        if self.status.is_empty() {
            self.status = format!("moved to {target}");
        }
    }

    fn apply_timeline(&mut self, cmd: Command) {
        match cmd {
            Command::Up => self.move_cursor(-1),
            Command::Down => self.move_cursor(1),
            Command::PageUp => self.move_cursor(-4),
            Command::PageDown => self.move_cursor(4),
            Command::Home => self.cursor = Slot(0),
            Command::End => self.cursor = Slot(LAST_SLOT),
            Command::NextBlock => {
                if let Some(b) = self.day.blocks().iter().find(|b| b.start > self.cursor) {
                    self.cursor = b.start;
                }
            }
            Command::PrevBlock => {
                if let Some(b) = self
                    .day
                    .blocks()
                    .iter()
                    .rev()
                    .find(|b| b.start < self.cursor)
                {
                    self.cursor = b.start;
                }
            }
            Command::New | Command::ExtendDown | Command::ExtendUp => {
                if self.selected().is_some() {
                    self.status = "slot occupied".into();
                    return;
                }
                self.mode = Mode::Create {
                    anchor: self.cursor,
                    title: None,
                };
                match cmd {
                    Command::ExtendDown => self.move_cursor(1),
                    Command::ExtendUp => self.move_cursor(-1),
                    _ => {}
                }
            }
            Command::EditTitle => {
                let Some(idx) = self.selected_or_status() else {
                    return;
                };
                let original = self.day.blocks()[idx].title.clone();
                self.title_buf = original.clone();
                self.mode = Mode::EditTitle {
                    idx,
                    original,
                    is_new: false,
                };
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
            _ => {}
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
        let Some(idx) = self.selected_or_status() else {
            return;
        };
        let before = self.day.clone();
        let offset = self.cursor.0 - self.day.blocks()[idx].start.0;
        match op(&mut self.day, idx) {
            Ok(new_idx) => {
                self.push_history(Snapshot::Day(self.date, before));
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
        let preset = match &self.mode {
            Mode::Create { title, .. } => title.clone(),
            _ => None,
        };
        self.mode = Mode::Normal;
        let Some((start, end)) = range else { return };
        self.checkpoint();
        let block = Block {
            start,
            end,
            kind: Kind::Standard,
            title: preset.clone().unwrap_or_default(),
        };
        match self.day.add(block) {
            Ok(idx) => {
                self.cursor = start;
                if preset.is_none() {
                    self.title_buf.clear();
                    self.mode = Mode::EditTitle {
                        idx,
                        original: String::new(),
                        is_new: true,
                    };
                }
                self.save();
            }
            Err(_) => {
                self.history.pop();
                self.status = "slot occupied".into();
            }
        }
    }

    fn checkpoint(&mut self) {
        self.push_history(Snapshot::Day(self.date, self.day.clone()));
    }

    fn checkpoint_tasks(&mut self) {
        self.push_history(Snapshot::Tasks(vec![(self.date, self.tasks.clone())]));
    }

    fn push_history(&mut self, snap: Snapshot) {
        if self.history.len() == HISTORY_CAP {
            self.history.remove(0);
        }
        self.history.push(snap);
        self.future.clear();
    }

    fn time_travel(&mut self, undo: bool) {
        let popped = if undo {
            self.history.pop()
        } else {
            self.future.pop()
        };
        let Some(snap) = popped else {
            self.status = if undo {
                "nothing to undo"
            } else {
                "nothing to redo"
            }
            .into();
            return;
        };
        let (date, day) = match snap {
            Snapshot::Day(date, day) => (date, day),
            Snapshot::Tasks(saved) => {
                match self.swap_tasks(&saved) {
                    Ok(current) => {
                        if undo {
                            self.future.push(current)
                        } else {
                            self.history.push(current)
                        }
                    }
                    Err(msg) => {
                        // Nothing was applied: put the snapshot back where it came from.
                        let back = Snapshot::Tasks(saved);
                        if undo {
                            self.history.push(back)
                        } else {
                            self.future.push(back)
                        }
                        self.status = msg;
                    }
                }
                return;
            }
        };
        // Switch to the snapshot's day first so the opposite stack records that day's state.
        // Every edit is already saved, so leaving the current day needs no save.
        if date != self.date {
            self.date = date;
            self.load_current();
        }
        let current = Snapshot::Day(self.date, self.day.clone());
        if undo {
            self.future.push(current)
        } else {
            self.history.push(current)
        }
        self.day = day;
        self.save();
    }

    /// Puts each saved task list in place for its date and returns the lists it replaced, as a
    /// snapshot for the opposite stack. Then shows the first entry's day. Fails when another
    /// day's file can't be read, or when a write fails. Days that gain tasks are written
    /// before days that lose tasks, so a failure part-way can duplicate a task but never lose
    /// it. It stops at the first failed write; the current day's list in memory changes only
    /// after its own write succeeds.
    fn swap_tasks(&mut self, saved: &[(NaiveDate, TaskList)]) -> Result<Snapshot, String> {
        debug_assert!(!saved.is_empty());
        if self.load_failed && saved.iter().any(|(date, _)| *date == self.date) {
            return Err(format!("not saving: {} could not be read", self.date));
        }
        let mut current = Vec::new();
        let mut days = Vec::new();
        for (date, _) in saved {
            if *date == self.date {
                current.push((*date, self.tasks.clone()));
                days.push(self.day.clone());
            } else {
                let (day, tasks) = self
                    .store
                    .load_day(*date)
                    .map_err(|e| format!("load failed: {e}"))?;
                current.push((*date, tasks));
                days.push(day);
            }
        }
        let mut order: Vec<usize> = (0..saved.len()).collect();
        order.sort_by_key(|&i| saved[i].1.len() < current[i].1.len());
        for i in order {
            let (date, list) = &saved[i];
            self.store
                .save_day(*date, &days[i], list)
                .map_err(|e| format!("save failed: {e}"))?;
            if *date == self.date {
                self.tasks = list.clone();
            }
        }
        let first = saved[0].0;
        if first != self.date {
            self.date = first;
            self.load_current();
        }
        self.task_sel = self.task_sel.min(self.tasks.len());
        Ok(Snapshot::Tasks(current))
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

    /// Loads the current date's blocks and tasks. A file that can't be read is left alone
    /// (never overwritten) and reported in the status line.
    fn load_current(&mut self) {
        match self.store.load_day(self.date) {
            Ok((day, tasks)) => {
                self.day = day;
                self.tasks = tasks;
                self.load_failed = false;
            }
            Err(e) => {
                self.status = format!("load failed: {e}");
                self.day = Day::default();
                self.tasks = TaskList::default();
                self.load_failed = true;
            }
        }
        self.task_sel = 0;
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
    use crate::storage::parse_tasks;
    use crate::theme::themes;

    const D: NaiveDate = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
    const D_FILE: &str = "2026-11-10.txt";

    fn st() -> (tempfile::TempDir, AppState) {
        let tmp = tempfile::tempdir().unwrap();
        let s = AppState::new(
            Store {
                dir: tmp.path().to_path_buf(),
            },
            D,
        );
        (tmp, s)
    }

    fn st_with_block(s0: u16, e0: u16) -> (tempfile::TempDir, AppState) {
        let (tmp, mut s) = st();
        s.day
            .add(Block {
                start: Slot(s0),
                end: Slot(e0),
                kind: Kind::Standard,
                title: "a".into(),
            })
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
        assert_eq!(s.store.load_day(D).unwrap().0, s.day);
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
        s.apply(Command::New, D);
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
        s.apply(Command::Text("p".into()), D);
        s.apply(Command::Cancel, D);
        assert_eq!(s.day.blocks()[0].title, "a");
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn set_kind_persists() {
        let (_t, mut s) = st_with_block(36, 40);
        s.apply(Command::SetKind(Kind::Scheduled), D);
        assert_eq!(s.day.blocks()[0].kind, Kind::Scheduled);
        assert_eq!(
            s.store.load_day(D).unwrap().0.blocks()[0].kind,
            Kind::Scheduled
        );
    }

    #[test]
    fn move_refusal_sets_status() {
        let (_t, mut s) = st_with_block(36, 40);
        s.day
            .add(Block {
                start: Slot(40),
                end: Slot(44),
                kind: Kind::Standard,
                title: String::new(),
            })
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
        assert_eq!(s.store.load_day(D).unwrap().0.blocks().len(), 1);
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
        let mut s = AppState::new(
            Store {
                dir: tmp.path().to_path_buf(),
            },
            D,
        );
        assert!(s.status.starts_with("load failed"), "{}", s.status);
        s.cursor = Slot(10);
        s.apply(Command::New, D);
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
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "10:00-11:00 deep edited elsewhere\n"
        );
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
            .add(Block {
                start: Slot(40),
                end: Slot(44),
                kind: Kind::Deep,
                title: String::new(),
            })
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
        assert_eq!(
            s.store.load_theme().as_deref(),
            Some(themes()[s.theme].name)
        );
    }

    #[test]
    fn drag_upward_normalizes() {
        let (_t, mut s) = st();
        s.drag_start(Slot(44));
        s.drag_update(Slot(40));
        s.drag_end();
        assert_eq!(
            (s.day.blocks()[0].start, s.day.blocks()[0].end),
            (Slot(40), Slot(45))
        );
    }

    #[test]
    fn drag_clips_at_neighbor() {
        let (_t, mut s) = st_with_block(40, 44);
        s.drag_start(Slot(36));
        s.drag_update(Slot(50));
        s.drag_end();
        assert_eq!(
            s.day.block_at(Slot(36)).map(|i| s.day.blocks()[i].end),
            Some(Slot(40))
        );
        s.apply(Command::Cancel, D);
        s.drag_start(Slot(41));
        s.drag_end();
        assert_eq!(s.day.blocks().len(), 1);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn save_error_sets_status() {
        let (t, mut s) = st();
        let f = t.path().join("file");
        std::fs::write(&f, "").unwrap();
        s.store.dir = f;
        s.apply(Command::New, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Confirm, D);
        assert!(s.status.starts_with("save failed"), "{}", s.status);
        assert_eq!(s.day.blocks().len(), 1);
    }

    fn st_tasks(txt: &str) -> (tempfile::TempDir, AppState) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(D_FILE), format!("[tasks]\n{txt}")).unwrap();
        let mut s = AppState::new(
            Store {
                dir: tmp.path().to_path_buf(),
            },
            D,
        );
        s.focus = Focus::Tasks;
        (tmp, s)
    }

    #[test]
    fn new_task_and_child_flow() {
        let (_t, mut s) = st();
        s.focus = Focus::Tasks;
        s.apply(Command::New, D);
        s.apply(Command::Text("Report".into()), D);
        s.apply(Command::Confirm, D);
        s.apply(Command::NewChild, D);
        s.apply(Command::Text("Outline".into()), D);
        s.apply(Command::Confirm, D);
        assert_eq!(
            s.store.load_day(D).unwrap().1,
            parse_tasks("- [ ] Report\n  - [ ] Outline\n")
        );
        assert_eq!(s.selected_task(), Some(1));
    }

    #[test]
    fn new_task_row_appends_top_level() {
        let (_t, mut s) = st_tasks(
            "- [ ] a
  - [ ] b
- [ ] c
",
        );
        s.focus = Focus::Timeline;
        s.task_sel = 0;
        s.new_task_at_end();
        assert_eq!(s.focus, Focus::Tasks);
        s.apply(Command::Text("d".into()), D);
        s.apply(Command::Confirm, D);
        assert_eq!(
            s.store.load_day(D).unwrap().1,
            parse_tasks(
                "- [ ] a
  - [ ] b
- [ ] c
- [ ] d
"
            )
        );
        assert_eq!(s.selected_task(), Some(3));
        s.apply(Command::Undo, D);
        assert_eq!(s.tasks.len(), 3);
    }

    #[test]
    fn task_edit_swallows_shortcuts() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::EditTitle, D);
        for c in [
            Command::Delete,
            Command::ToggleDone,
            Command::Indent,
            Command::ScheduleTask,
            Command::New,
            Command::FocusPane(Focus::Timeline),
        ] {
            s.apply(c, D);
        }
        assert_eq!(s.tasks.len(), 1);
        assert!(!s.tasks.items()[0].done);
        assert_eq!(s.focus, Focus::Tasks);
        assert!(matches!(s.mode, Mode::EditTask { .. }));
    }

    #[test]
    fn delete_parent_removes_subtree_and_undo_restores() {
        let (_t, mut s) = st_tasks("- [ ] p\n  - [ ] c\n- [ ] q\n");
        s.apply(Command::Delete, D);
        assert_eq!(s.tasks.len(), 1);
        s.apply(Command::Undo, D);
        assert_eq!(s.tasks.len(), 3);
        assert_eq!(s.store.load_day(D).unwrap().1.len(), 3);
    }

    #[test]
    fn undo_interleaves_tasks_and_blocks() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::ToggleDone, D);
        s.apply(Command::FocusPane(Focus::Timeline), D);
        s.cursor = Slot(10);
        s.apply(Command::New, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Undo, D);
        assert!(s.day.blocks().is_empty());
        assert!(s.tasks.items()[0].done);
        s.apply(Command::Undo, D);
        assert!(!s.tasks.items()[0].done);
    }

    #[test]
    fn cancel_new_task_removes_it() {
        let (t, mut s) = st();
        s.focus = Focus::Tasks;
        s.apply(Command::New, D);
        s.apply(Command::Text("x".into()), D);
        s.apply(Command::Cancel, D);
        assert!(s.tasks.is_empty());
        assert!(!t.path().join(D_FILE).exists());
        s.apply(Command::Undo, D);
        assert_eq!(s.status, "nothing to undo");
    }

    #[test]
    fn refused_indent_sets_status() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::Indent, D);
        assert_eq!(s.status, "can't move there");
        s.apply(Command::Undo, D);
        assert_eq!(s.status, "nothing to undo");
    }

    #[test]
    fn focus_pane_sets_focus() {
        let (_t, mut s) = st();
        assert_eq!(s.focus, Focus::Timeline);
        s.apply(Command::FocusPane(Focus::Tasks), D);
        assert_eq!(s.focus, Focus::Tasks);
        s.apply(Command::FocusPane(Focus::Tasks), D);
        assert_eq!(s.focus, Focus::Tasks);
        s.apply(Command::FocusPane(Focus::Timeline), D);
        assert_eq!(s.focus, Focus::Timeline);
    }

    #[test]
    fn new_task_row_is_reachable_from_keyboard() {
        let (_t, mut s) = st_tasks(
            "- [ ] a
- [ ] b
",
        );
        s.apply(Command::End, D);
        assert_eq!(s.selected_task(), Some(1));
        s.apply(Command::Down, D);
        assert!(s.new_task_row_selected());
        assert_eq!(s.selected_task(), None);
        s.apply(Command::Down, D);
        assert!(s.new_task_row_selected());
        // Task-only commands do nothing on the row.
        s.apply(Command::ToggleDone, D);
        s.apply(Command::Delete, D);
        assert_eq!(s.tasks.len(), 2);
        s.apply(Command::EditTitle, D);
        s.apply(Command::Text("c".into()), D);
        s.apply(Command::Confirm, D);
        assert_eq!(
            s.store.load_day(D).unwrap().1,
            parse_tasks(
                "- [ ] a
- [ ] b
- [ ] c
"
            )
        );
        assert_eq!(s.selected_task(), Some(2));
        s.apply(Command::Up, D);
        assert_eq!(s.selected_task(), Some(1));
    }

    #[test]
    fn deleting_last_task_keeps_a_task_selected() {
        let (_t, mut s) = st_tasks(
            "- [ ] a
- [ ] b
",
        );
        s.apply(Command::End, D);
        s.apply(Command::Delete, D);
        assert_eq!(s.selected_task(), Some(0));
        s.apply(Command::Delete, D);
        assert!(s.new_task_row_selected());
    }

    #[test]
    fn unreadable_day_blocks_task_saves_too() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(D_FILE);
        let bytes = [0xffu8, 0xfe];
        std::fs::write(&file, bytes).unwrap();
        let mut s = AppState::new(
            Store {
                dir: tmp.path().to_path_buf(),
            },
            D,
        );
        assert!(s.status.starts_with("load failed"), "{}", s.status);
        s.focus = Focus::Tasks;
        s.apply(Command::New, D);
        s.apply(Command::Text("x".into()), D);
        s.apply(Command::Confirm, D);
        assert!(s.status.starts_with("not saving"), "{}", s.status);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }

    #[test]
    fn starts_at_eight() {
        let (_t, s) = st();
        assert_eq!(s.cursor, Slot(32));
        assert_eq!(s.focus, Focus::Timeline);
    }

    #[test]
    fn drag_moves_block() {
        let (_t, mut s) = st_with_block(36, 40);
        s.drag_start(Slot(37));
        s.drag_update(Slot(45));
        assert_eq!(s.move_preview(), Some((Slot(44), Slot(48))));
        s.drag_end();
        assert_eq!(
            (s.day.blocks()[0].start, s.day.blocks()[0].end),
            (Slot(44), Slot(48))
        );
        assert_eq!(s.store.load_day(D).unwrap().0, s.day);
        s.apply(Command::Undo, D);
        assert_eq!(s.day.blocks()[0].start, Slot(36));
    }

    #[test]
    fn drag_move_refuses_overlap() {
        let (_t, mut s) = st_with_block(36, 40);
        s.day
            .add(Block {
                start: Slot(50),
                end: Slot(52),
                kind: Kind::Deep,
                title: String::new(),
            })
            .unwrap();
        s.drag_start(Slot(36));
        s.drag_update(Slot(49));
        s.drag_end();
        assert_eq!(s.status, "overlaps another block");
        assert_eq!(s.day.blocks()[0].start, Slot(36));
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn drag_move_clamps_to_day() {
        let (_t, mut s) = st_with_block(36, 40);
        s.drag_start(Slot(36));
        s.drag_update(Slot(95));
        assert_eq!(s.move_preview(), Some((Slot(92), Slot(96))));
        s.drag_update(Slot(0));
        s.drag_start(Slot(0));
        assert_eq!(s.move_preview(), Some((Slot(0), Slot(4))));
    }

    #[test]
    fn escape_cancels_move() {
        let (_t, mut s) = st_with_block(36, 40);
        s.drag_start(Slot(36));
        s.drag_update(Slot(50));
        s.apply(Command::Cancel, D);
        assert_eq!(s.mode, Mode::Normal);
        assert_eq!(s.day.blocks()[0].start, Slot(36));
        assert_eq!(s.move_preview(), None);
    }

    #[test]
    fn schedule_task_creates_titled_block() {
        let (_t, mut s) = st_tasks("- [ ] Write report\n");
        s.cursor = Slot(40);
        s.apply(Command::ScheduleTask, D);
        assert_eq!(s.focus, Focus::Timeline);
        s.apply(Command::Down, D);
        s.apply(Command::Confirm, D);
        assert_eq!(s.mode, Mode::Normal);
        let b = &s.day.blocks()[0];
        assert_eq!(
            (b.start, b.end, b.title.as_str()),
            (Slot(40), Slot(42), "Write report")
        );
    }

    #[test]
    fn drop_task_makes_one_hour_block_clipped() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.day
            .add(Block {
                start: Slot(42),
                end: Slot(44),
                kind: Kind::Deep,
                title: String::new(),
            })
            .unwrap();
        s.drop_task("a", Slot(40));
        let i = s.day.block_at(Slot(40)).unwrap();
        assert_eq!(
            (s.day.blocks()[i].end, s.day.blocks()[i].title.as_str()),
            (Slot(42), "a")
        );
        assert_eq!((s.cursor, s.focus), (Slot(40), Focus::Timeline));
        assert_eq!(s.day.blocks().len(), 2);
        s.drop_task("a", Slot(43));
        assert_eq!(s.status, "slot occupied");
        assert_eq!(s.day.blocks().len(), 2);
    }

    #[test]
    fn drop_task_clamps_at_day_end() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.drop_task("a", Slot(95));
        assert_eq!(
            (s.day.blocks()[0].start, s.day.blocks()[0].end),
            (Slot(95), Slot(96))
        );
    }

    #[test]
    fn drop_task_ignored_outside_normal_mode() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::New, D);
        s.drop_task("a", Slot(40));
        assert!(s.day.blocks().is_empty());
        s.apply(Command::Cancel, D);
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn click_task_selects_and_toggles() {
        let (_t, mut s) = st_tasks("- [ ] a\n- [ ] b\n");
        s.focus = Focus::Timeline;
        s.click_task(1, false);
        assert_eq!((s.focus, s.selected_task()), (Focus::Tasks, Some(1)));
        s.click_task(1, true);
        assert!(s.tasks.items()[1].done);
        s.apply(Command::Undo, D);
        assert!(!s.tasks.items()[1].done);
    }

    #[test]
    fn click_task_ignores_bad_index_and_other_modes() {
        let (_t, mut s) = st_tasks("- [ ] a\n- [ ] b\n");
        s.focus = Focus::Timeline;
        s.click_task(5, true);
        assert_eq!((s.focus, s.task_sel), (Focus::Timeline, 0));
        s.apply(Command::New, D);
        s.click_task(1, true);
        assert!(!s.tasks.items()[1].done);
        assert!(matches!(s.mode, Mode::Create { .. }));
    }

    #[test]
    fn drop_range_matches_drop_task() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.day
            .add(Block {
                start: Slot(42),
                end: Slot(44),
                kind: Kind::Deep,
                title: String::new(),
            })
            .unwrap();
        assert_eq!(s.drop_range(Slot(40)), Some((Slot(40), Slot(42))));
        assert_eq!(s.drop_range(Slot(10)), Some((Slot(10), Slot(14))));
        assert_eq!(s.drop_range(Slot(95)), Some((Slot(95), Slot(96))));
        assert_eq!(s.drop_range(Slot(43)), None);
    }

    #[test]
    fn schedule_task_on_occupied_slot_refuses() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.day
            .add(Block {
                start: Slot(40),
                end: Slot(42),
                kind: Kind::Deep,
                title: String::new(),
            })
            .unwrap();
        s.cursor = Slot(40);
        s.apply(Command::ScheduleTask, D);
        assert_eq!(s.status, "slot occupied");
        assert_eq!(s.focus, Focus::Timeline);
        assert_eq!(s.mode, Mode::Normal);
    }

    #[test]
    fn tasks_belong_to_their_day_and_share_its_file() {
        let (t, mut s) = st_with_block(40, 44);
        s.focus = Focus::Tasks;
        s.apply(Command::New, D);
        s.apply(Command::Text("today".into()), D);
        s.apply(Command::Confirm, D);
        // One file holds both.
        let text = std::fs::read_to_string(t.path().join(D_FILE)).unwrap();
        assert_eq!(text, "10:00-11:00 standard a\n\n[tasks]\n- [ ] today\n");
        s.apply(Command::NextDay, D);
        assert!(s.tasks.is_empty());
        s.apply(Command::New, D);
        s.apply(Command::Text("tomorrow".into()), D);
        s.apply(Command::Confirm, D);
        s.apply(Command::PrevDay, D);
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks.items()[0].title, "today");
        assert_eq!(s.day.blocks().len(), 1);
        let next = D.succ_opt().unwrap();
        assert_eq!(
            s.store.load_day(next).unwrap().1.items()[0].title,
            "tomorrow"
        );
    }

    #[test]
    fn block_edits_keep_the_days_tasks() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.focus = Focus::Timeline;
        s.day
            .add(Block {
                start: Slot(40),
                end: Slot(44),
                kind: Kind::Standard,
                title: "b".into(),
            })
            .unwrap();
        s.save();
        let (day, tasks) = s.store.load_day(D).unwrap();
        assert_eq!(day.blocks().len(), 1);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn day_change_resets_task_selection() {
        let (_t, mut s) = st_tasks("- [ ] a\n- [ ] b\n");
        s.task_sel = 1;
        s.apply(Command::NextDay, D);
        assert_eq!(s.task_sel, 0);
    }

    #[test]
    fn undo_returns_to_the_day_of_a_task_edit() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::ToggleDone, D);
        s.apply(Command::NextDay, D);
        s.apply(Command::Undo, D);
        assert_eq!(s.date, D);
        assert!(!s.tasks.items()[0].done);
        assert!(!s.store.load_day(D).unwrap().1.items()[0].done);
        s.apply(Command::Redo, D);
        assert!(s.tasks.items()[0].done);
    }

    fn tasks_text(s: &AppState) -> String {
        crate::storage::serialize_tasks(&s.tasks)
    }

    fn saved_tasks(s: &AppState, date: NaiveDate) -> String {
        crate::storage::serialize_tasks(&s.store.load_day(date).unwrap().1)
    }

    #[test]
    fn move_task_to_next_day_takes_subtasks_and_undoes_in_one_step() {
        let (t, mut s) = st_tasks("- [ ] a\n- [ ] b\n  - [ ] b1\n- [ ] c\n");
        let next = D.succ_opt().unwrap();
        std::fs::write(t.path().join("2026-11-11.txt"), "[tasks]\n- [ ] x\n").unwrap();
        s.task_sel = 1;
        s.apply(Command::MoveTaskToNextDay, D);
        assert_eq!(s.date, D);
        assert_eq!(tasks_text(&s), "- [ ] a\n- [ ] c\n");
        assert_eq!(s.selected_task(), Some(1));
        assert_eq!(s.status, "moved to 2026-11-11");
        assert_eq!(saved_tasks(&s, next), "- [ ] x\n- [ ] b\n  - [ ] b1\n");
        assert_eq!(saved_tasks(&s, D), "- [ ] a\n- [ ] c\n");

        s.apply(Command::Undo, D);
        assert_eq!(tasks_text(&s), "- [ ] a\n- [ ] b\n  - [ ] b1\n- [ ] c\n");
        assert_eq!(saved_tasks(&s, next), "- [ ] x\n");

        s.apply(Command::Redo, D);
        assert_eq!(tasks_text(&s), "- [ ] a\n- [ ] c\n");
        assert_eq!(saved_tasks(&s, next), "- [ ] x\n- [ ] b\n  - [ ] b1\n");
    }

    #[test]
    fn move_task_keeps_the_other_days_blocks() {
        let (t, mut s) = st_tasks("- [ ] a\n");
        std::fs::write(
            t.path().join("2026-11-11.txt"),
            "09:00-09:15 deep x\n\n[tasks]\n- [ ] y\n",
        )
        .unwrap();
        s.apply(Command::MoveTaskToNextDay, D);
        let (day, tasks) = s.store.load_day(D.succ_opt().unwrap()).unwrap();
        assert_eq!(day.blocks().len(), 1);
        assert_eq!(
            crate::storage::serialize_tasks(&tasks),
            "- [ ] y\n- [ ] a\n"
        );
    }

    #[test]
    fn move_task_to_prev_day_rebases_depth_and_deletes_empty_source() {
        let (t, mut s) = st_tasks("- [ ] p\n  - [ ] c\n");
        let prev = D.pred_opt().unwrap();
        s.task_sel = 1;
        s.apply(Command::MoveTaskToPrevDay, D);
        assert_eq!(tasks_text(&s), "- [ ] p\n");
        assert_eq!(saved_tasks(&s, prev), "- [ ] c\n");
        // Moving the last task empties today's list (no blocks), so its file goes away.
        s.task_sel = 0;
        s.apply(Command::MoveTaskToPrevDay, D);
        assert!(s.tasks.is_empty());
        assert!(!t.path().join(D_FILE).exists());
    }

    #[test]
    fn move_task_needs_a_selected_task() {
        let (t, mut s) = st_tasks("- [ ] a\n");
        s.task_sel = 1; // the "+ new task" row
        s.apply(Command::MoveTaskToNextDay, D);
        assert_eq!(tasks_text(&s), "- [ ] a\n");
        assert!(!t.path().join("2026-11-11.txt").exists());
        assert!(s.status.is_empty(), "{}", s.status);
    }

    #[test]
    fn move_task_refuses_when_other_day_is_unreadable() {
        let (t, mut s) = st_tasks("- [ ] a\n");
        let file = t.path().join("2026-11-11.txt");
        std::fs::write(&file, [0xffu8, 0xfe]).unwrap();
        s.apply(Command::MoveTaskToNextDay, D);
        assert!(s.status.starts_with("load failed"), "{}", s.status);
        assert_eq!(tasks_text(&s), "- [ ] a\n");
        assert_eq!(std::fs::read(&file).unwrap(), [0xffu8, 0xfe]);
    }

    #[test]
    fn redoing_a_move_with_a_failing_target_write_does_not_lose_the_task() {
        let (t, mut s) = st_tasks(
            "- [ ] a
- [ ] b
",
        );
        let next = D.succ_opt().unwrap();
        s.task_sel = 1;
        s.apply(Command::MoveTaskToNextDay, D);
        s.apply(Command::Undo, D);
        assert_eq!(s.future.len(), 1);
        // A directory where the temporary file goes makes every write to that day fail.
        std::fs::create_dir(t.path().join("2026-11-11.txt.tmp")).unwrap();
        s.apply(Command::Redo, D);
        assert!(s.status.starts_with("save failed"), "{}", s.status);
        assert_eq!(
            tasks_text(&s),
            "- [ ] a
- [ ] b
"
        );
        assert_eq!(
            saved_tasks(&s, D),
            "- [ ] a
- [ ] b
"
        );
        assert_eq!(saved_tasks(&s, next), "");
        assert_eq!(s.future.len(), 1);
        // Once the write works again, the same redo goes through.
        std::fs::remove_dir(t.path().join("2026-11-11.txt.tmp")).unwrap();
        s.apply(Command::Redo, D);
        assert_eq!(
            saved_tasks(&s, D),
            "- [ ] a
"
        );
        assert_eq!(
            saved_tasks(&s, next),
            "- [ ] b
"
        );
    }

    #[test]
    fn undo_of_a_task_edit_refuses_when_the_current_day_is_unreadable() {
        let (t, mut s) = st_tasks(
            "- [ ] a
",
        );
        s.history
            .push(Snapshot::Tasks(vec![(D, TaskList::default())]));
        let file = t.path().join(D_FILE);
        std::fs::write(&file, [0xffu8, 0xfe]).unwrap();
        s.load_current();
        assert!(s.load_failed);
        s.apply(Command::Undo, D);
        assert!(s.status.starts_with("not saving"), "{}", s.status);
        assert_eq!(std::fs::read(&file).unwrap(), [0xffu8, 0xfe]);
        assert_eq!(s.history.len(), 1);
        assert!(s.future.is_empty());
    }

    #[test]
    fn move_task_is_ignored_while_typing_and_in_timeline() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.apply(Command::EditTitle, D);
        s.apply(Command::MoveTaskToNextDay, D);
        assert_eq!(s.tasks.len(), 1);
        s.apply(Command::Cancel, D);
        s.focus = Focus::Timeline;
        s.apply(Command::MoveTaskToNextDay, D);
        assert_eq!(s.tasks.len(), 1);
    }
}
