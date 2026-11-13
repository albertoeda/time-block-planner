# Daily Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the task list daily and store it in the same file as that day's time blocks (one file per day), and add shortcuts that move the selected task to the previous or next day.

**Architecture:**
- One plaintext file per day, `YYYY-MM-DD.txt`. The block lines come first, then an optional `[tasks]` line, then the day's task checklist. The old global `tasks.txt` is dropped (no migration: the app has only been used for testing).
- `Store::load_day` / `save_day` read and write blocks and tasks together, so there is one file, one atomic write and one "could not be read" flag per day.
- `AppState` keeps `day` and `tasks` as separate fields (so the UI and most tests stay as they are). `load_current` loads both, and `PrevDay`/`NextDay`/`Today` switch both panes.
- The undo history stays one list. A task snapshot now carries the date(s) it belongs to, so undo returns to the right day and "move to another day" is a single undo step that restores both days.

**Tech Stack:** unchanged. Rust edition 2024, eframe/egui 0.37.0, chrono, dirs, tempfile for tests.

## Global Constraints

- **Pure core + thin UI:** `model`, `tasks`, `storage`, `state` have unit tests and no egui. Only `ui/` touches egui.
- **Single-key shortcuts** are read from `egui::Event::Text` and act only in Normal mode. While a title is edited (`EditTitle` / `EditTask`) every printable key is text.
- **No shortcut may need AltGr** (egui drops AltGr text). So every single-key shortcut also gets a chord that needs no AltGr.
- **File safety rules:** atomic write; the day file is deleted when the day has no blocks, no comments, no unparsed lines and no tasks; a file that could not be read is never overwritten.
- **Hand-editable:** lines the app can't read in the block part are kept (existing `# unparsed lines…` rule). The task part is parsed leniently (existing `parse_tasks`).
- **Undo:** one history covers block and task edits.
- **Task line format** is unchanged: `- [ ] title`, two spaces per level.
- **Test style:** state tests use `const D: NaiveDate = 2026-11-10` and a `tempfile` store.
- **Commit style:** conventional commits (`feat:`, `test:`, `docs:`), end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Verify with `cargo test` and `cargo fmt --check` (CI runs both).

## Decisions made here

| Topic | Decision |
|---|---|
| File | `<dir>/YYYY-MM-DD.txt` holds blocks and tasks |
| Task section | Starts at a line that is exactly `[tasks]` (spaces around it are ignored). Everything after it is tasks. |
| Writing | Blocks (with comments and unparsed lines) first. If there are tasks: one blank line (only when the block part is not empty), `[tasks]`, then the checklist. No `[tasks]` line when there are no tasks. |
| Old `tasks.txt` | Not read, not migrated, not deleted. |
| Move key (single key) | `[` = move task to previous day, `]` = next day (Tasks pane only) |
| Move key (chord) | `Alt+Shift+←` / `Alt+Shift+→` |
| Where the task lands | End of the other day's list, as a top-level task. Its subtasks go with it, depths rebased so the task is level 0. |
| After the move | Stay on the same day. Selection stays at the same row, so it lands on the next task. Status shows `moved to YYYY-MM-DD`. |
| Undo of a move | One step restores both days. |

Example day file:

```
# week 46
09:00-10:30 deep Write quarterly report
11:00-11:30 scheduled Standup

[tasks]
- [ ] Call the bank
  - [x] Find the number
```

## File Structure

- Modify `src/tasks.rs`: `take_subtree`, `append_subtree` (pure list operations).
- Modify `src/storage.rs`: `parse_file` / `serialize_file`, `load_day` / `save_day` carry the tasks; remove the separate tasks file.
- Modify `src/state.rs`: one load/save per day, dated task undo snapshots, two new `Command`s, `move_task_to_day`.
- Modify `src/ui/mod.rs`: key and text mapping.
- Modify `src/ui/chrome.rs`: footer hint, help line, status colour for `moved to …`.
- Modify `src/ui/tasks.rs`: test fixture only.
- Modify `README.md`: data folder and keys.

---

### Task 1: Move a subtree between lists (`TaskList`)

**Files:**
- Modify: `src/tasks.rs` (add two methods after `move_down`, add tests in `mod tests`)

**Interfaces:**
- Consumes: existing `TaskList::subtree_end(&self, i: usize) -> usize`.
- Produces:
  - `pub fn take_subtree(&mut self, i: usize) -> Vec<Task>`: removes the subtree rooted at `i`; the returned tasks have depths rebased so the root is 0.
  - `pub fn append_subtree(&mut self, sub: Vec<Task>) -> usize`: appends `sub` (root first, root depth 0) at the end; returns the root's index.

- [ ] **Step 1: Write the failing tests**

Add at the end of `mod tests` in `src/tasks.rs`:

```rust
    #[test]
    fn take_subtree_rebases_depth() {
        let mut l = list(&[(0, "a"), (1, "b"), (2, "b1"), (1, "c"), (0, "d")]);
        let sub = l.take_subtree(1);
        let got: Vec<_> = sub.iter().map(|t| (t.depth, t.title.as_str())).collect();
        assert_eq!(got, [(0, "b"), (1, "b1")]);
        assert_eq!(titles(&l), [(0, "a"), (1, "c"), (0, "d")]);
    }
    #[test]
    fn append_subtree_goes_to_the_end_at_top_level() {
        let mut l = list(&[(0, "x"), (1, "x1")]);
        let at = l.append_subtree(vec![
            Task {
                title: "b".into(),
                done: true,
                depth: 0,
            },
            Task {
                title: "b1".into(),
                done: false,
                depth: 1,
            },
        ]);
        assert_eq!(at, 2);
        assert_eq!(titles(&l), [(0, "x"), (1, "x1"), (0, "b"), (1, "b1")]);
        assert!(l.items()[2].done);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib tasks::tests`
Expected: compile error, "no method named `take_subtree`".

- [ ] **Step 3: Implement**

In `src/tasks.rs`, add inside `impl TaskList`, after `move_down`:

```rust
    /// Removes the subtree rooted at `i` and returns it, with depths rebased so `i` is 0.
    pub fn take_subtree(&mut self, i: usize) -> Vec<Task> {
        let end = self.subtree_end(i);
        let base = self.items[i].depth;
        let mut sub: Vec<Task> = self.items.drain(i..end).collect();
        sub.iter_mut().for_each(|t| t.depth -= base);
        sub
    }

    /// Appends a subtree (root first, root depth 0) at the end of the list as a top-level task.
    /// Returns the root's index.
    pub fn append_subtree(&mut self, sub: Vec<Task>) -> usize {
        let at = self.items.len();
        self.items.extend(sub);
        at
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib tasks::tests`
Expected: all `tasks::tests` PASS (existing 7 plus 2 new).

- [ ] **Step 5: Commit**

```bash
git add src/tasks.rs
git commit -m "feat: take and append task subtrees

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: One file per day for blocks and tasks (storage + state)

This task changes the `Store` API and every caller in one commit, so the crate compiles at the end of it.

**Files:**
- Modify: `src/storage.rs` (module doc, `parse_file`, `serialize_file`, `load_day`, `save_day`; remove `tasks_path`, `load_tasks`, `save_tasks`; tests)
- Modify: `src/state.rs` (`new`, `load_current`, `save`, remove `save_tasks` and `tasks_load_failed`, `Snapshot`, `checkpoint_tasks`, `edit_task`, cancel-new-task arm, `SaveNow`, `time_travel`, new `swap_tasks`; tests)
- Modify: `src/ui/tasks.rs` (test fixture only)

**Interfaces:**
- Consumes: `NaiveDate`, `Day`, `TaskList`, existing `parse_day`, `serialize_day`, `parse_tasks`, `serialize_tasks`, `write_atomic`.
- Produces (used by Task 3):
  - `pub fn parse_file(text: &str) -> (Day, TaskList)`
  - `pub fn serialize_file(day: &Day, tasks: &TaskList) -> String`
  - `Store::load_day(&self, date: NaiveDate) -> io::Result<(Day, TaskList)>`
  - `Store::save_day(&self, date: NaiveDate, day: &Day, tasks: &TaskList) -> io::Result<()>`
  - `enum Snapshot { Day(NaiveDate, Day), Tasks(Vec<(NaiveDate, TaskList)>) }`: for `Tasks`, the first entry is the day shown after undo/redo.
  - `AppState::swap_tasks(&mut self, saved: &[(NaiveDate, TaskList)]) -> Result<Snapshot, String>`
  - `AppState::save(&mut self)` now writes the day's blocks and tasks; `save_tasks` no longer exists.

#### Part A: storage

- [ ] **Step 1: Write the failing storage tests**

In `src/storage.rs` `mod tests`:

1. **Delete** the test `tasks_file_roundtrip_and_empty_deletes`.
2. **Edit** three existing tests for the new signatures:
   - `save_creates_missing_dir`: change `store.save_day(date(), &d).unwrap();` to `store.save_day(date(), &d, &TaskList::default()).unwrap();` and the last assert to `assert_eq!(store.load_day(date()).unwrap(), (d, TaskList::default()));`
   - `saving_empty_day_deletes_file`: change the two `save_day` calls to `.save_day(date(), &parse_day("09:00-09:15 deep x\n"), &TaskList::default())` and `store.save_day(date(), &Day::default(), &TaskList::default()).unwrap();`
   - `missing_file_loads_empty`: change the assert to `assert_eq!(store.load_day(date()).unwrap(), (Day::default(), TaskList::default()));`
3. **Add** these tests at the end:

```rust
    #[test]
    fn file_with_blocks_and_tasks_roundtrips() {
        let txt = "09:00-10:30 deep Write\n\n[tasks]\n- [ ] a\n  - [x] b\n";
        let (d, t) = parse_file(txt);
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(t.len(), 2);
        assert!(t.items()[1].done);
        assert_eq!(serialize_file(&d, &t), txt);
    }

    #[test]
    fn file_without_tasks_has_no_header() {
        let txt = "09:00-09:15 standard x\n";
        let (d, t) = parse_file(txt);
        assert!(t.is_empty());
        assert_eq!(serialize_file(&d, &t), txt);
    }

    #[test]
    fn tasks_only_file_roundtrips() {
        let txt = "[tasks]\n- [ ] a\n";
        let (d, t) = parse_file(txt);
        assert!(d.is_empty());
        assert_eq!(t.len(), 1);
        assert_eq!(serialize_file(&d, &t), txt);
    }

    #[test]
    fn hand_edits_around_the_tasks_header_are_tolerated() {
        let (d, t) = parse_file("\u{feff}garbage\n09:00-09:15 deep x\n  [tasks]  \nplain\n");
        assert_eq!(d.unparsed, vec!["garbage"]);
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(t.items()[0].title, "plain");
    }

    #[test]
    fn day_file_lives_while_it_has_blocks_or_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            dir: tmp.path().to_path_buf(),
        };
        let path = store.dir.join("2026-11-10.txt");
        let tasks = parse_tasks("- [ ] a\n");
        store
            .save_day(date(), &parse_day("09:00-09:15 deep x\n"), &tasks)
            .unwrap();
        // Blocks gone, tasks remain: the file stays.
        store.save_day(date(), &Day::default(), &tasks).unwrap();
        assert!(path.exists());
        assert_eq!(store.load_day(date()).unwrap(), (Day::default(), tasks));
        // Nothing left: the file goes.
        store
            .save_day(date(), &Day::default(), &TaskList::default())
            .unwrap();
        assert!(!path.exists());
    }
```

Also change the stale doc comment above `parse_tasks` from "Parses tasks.txt leniently:" to "Parses a task list leniently:".

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib storage::tests`
Expected: compile errors (`parse_file` missing, `save_day` takes 2 args).

- [ ] **Step 3: Implement storage**

In `src/storage.rs`:

(a) Replace the module doc line with:

```rust
//! Plaintext persistence: one `YYYY-MM-DD.txt` per day (time blocks, then an optional `[tasks]`
//! checklist) plus `config.txt`, in a user folder.
```

(b) After `const APP_DIR…` add:

```rust
const TASKS_HEADER: &str = "[tasks]";
```

(c) After `serialize_tasks` add:

```rust
/// Parses a whole day file: block lines first, then everything after a `[tasks]` line is the
/// day's task checklist.
pub fn parse_file(text: &str) -> (Day, TaskList) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (mut blocks, mut tasks) = (String::new(), String::new());
    let mut in_tasks = false;
    for line in text.lines() {
        if line.trim() == TASKS_HEADER {
            in_tasks = true;
            continue;
        }
        let part = if in_tasks { &mut tasks } else { &mut blocks };
        part.push_str(line);
        part.push('\n');
    }
    (parse_day(&blocks), parse_tasks(&tasks))
}

/// Renders a day file: the blocks, then (only when there are tasks) a blank line, `[tasks]`
/// and the checklist.
pub fn serialize_file(day: &Day, tasks: &TaskList) -> String {
    let mut out = serialize_day(day);
    if !tasks.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(TASKS_HEADER);
        out.push('\n');
        out.push_str(&serialize_tasks(tasks));
    }
    out
}
```

(d) In `impl Store`, replace `load_day` and `save_day`, and delete `tasks_path`, `load_tasks`, `save_tasks`:

```rust
    pub fn load_day(&self, date: NaiveDate) -> io::Result<(Day, TaskList)> {
        match fs::read_to_string(self.day_path(date)) {
            Ok(text) => Ok(parse_file(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Ok((Day::default(), TaskList::default()))
            }
            Err(e) => Err(e),
        }
    }

    /// Writes the day's blocks and tasks to one file; deletes the file when both are empty.
    pub fn save_day(&self, date: NaiveDate, day: &Day, tasks: &TaskList) -> io::Result<()> {
        let path = self.day_path(date);
        if day.is_empty() && tasks.is_empty() {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        write_atomic(&self.dir, &path, &serialize_file(day, tasks))
    }
```

- [ ] **Step 4: Run storage tests**

Run: `cargo test --lib storage::tests`
Expected: will not compile yet because `state.rs` and `ui/tasks.rs` still call the old API. Continue with Part B and run the full suite at the end of this task.

#### Part B: state

- [ ] **Step 5: Write the failing state tests**

In `src/state.rs` `mod tests`, under `const D`, add:

```rust
    const D_FILE: &str = "2026-11-10.txt";
```

and at the end of `mod tests`:

```rust
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
        assert_eq!(s.store.load_day(next).unwrap().1.items()[0].title, "tomorrow");
    }

    #[test]
    fn block_edits_keep_the_days_tasks() {
        let (_t, mut s) = st_tasks("- [ ] a\n");
        s.focus = Focus::Timeline;
        s.cursor = Slot(40);
        s.apply(Command::New, D);
        s.apply(Command::Confirm, D);
        s.apply(Command::Cancel, D);
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
```

In the first test, `st_with_block(40, 44)` makes a block `a` at 10:00-11:00 (slots 40-44) and saves it. In the second test, `Command::New` in the timeline on a free slot enters Create mode; `Confirm` creates the block and opens title editing; `Cancel` removes it again. If that sequence leaves no block, replace those three applies with `s.day.add(Block { start: Slot(40), end: Slot(44), kind: Kind::Standard, title: "b".into() }).unwrap(); s.save();`, which is all the test needs.

- [ ] **Step 6: Update the existing tests for the new API**

In `src/state.rs` `mod tests` (use Grep for `tasks.txt`, `load_tasks` and `load_day` to find each site):

1. `st_tasks`: replace `std::fs::write(tmp.path().join("tasks.txt"), txt).unwrap();` with:

```rust
        std::fs::write(tmp.path().join(D_FILE), format!("[tasks]\n{txt}")).unwrap();
```

2. Every `s.store.load_tasks().unwrap()` becomes `s.store.load_day(D).unwrap().1` (tests `new_task_and_child_flow`, `new_task_row_appends_top_level`, `new_task_row_is_reachable_from_keyboard`); in `delete_parent_removes_subtree_and_undo_restores` change `s.store.load_tasks().unwrap().len()` to `s.store.load_day(D).unwrap().1.len()`.
3. Every `s.store.load_day(D).unwrap()` that was used as a `Day` needs `.0`: in `keyboard_create_flow` (`assert_eq!(s.store.load_day(D).unwrap().0, s.day)`), `set_kind_persists` (`.0.blocks()[0].kind`), the test around line 909 (`.0.blocks().len()`), and the test around line 1328 (`.0`, compared with `s.day`).
4. `cancel_new_task_removes_it`: `t.path().join("tasks.txt")` becomes `t.path().join(D_FILE)`.
5. **Rewrite** `unreadable_tasks_never_overwritten` (one unreadable file now blocks both blocks and tasks):

```rust
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
```

6. **Delete** `both_load_errors_reported` (there is only one file now).

In `src/ui/tasks.rs` test `pane_widths`, replace the `std::fs::write(tmp.path().join("tasks.txt"), tasks_txt).unwrap();` line with:

```rust
            std::fs::write(
                tmp.path().join("2026-11-10.txt"),
                format!("[tasks]\n{tasks_txt}"),
            )
            .unwrap();
```

- [ ] **Step 7: Implement the state changes**

All in `src/state.rs`.

(a) Replace the `Snapshot` enum and its doc comment:

```rust
/// One undoable state: a day's blocks (with its date), or the task lists of one or more days.
/// For `Tasks`, the first entry is the day to show after undo/redo.
enum Snapshot {
    Day(NaiveDate, Day),
    Tasks(Vec<(NaiveDate, TaskList)>),
}
```

(b) In `struct AppState`, change the `load_failed` doc comment and delete the `tasks_load_failed` field and its doc comment:

```rust
    /// The current date's file could not be read; never write it, or we'd destroy it.
    load_failed: bool,
```

(c) In `AppState::new`, delete the line `tasks_load_failed: false,` from the struct literal, and replace everything after the struct literal's closing `};` with:

```rust
        s.load_current();
        s
    }
```

(that removes the old `match s.store.load_tasks() { … }` block and the old separate `s.load_current();`).

(d) Replace `save` and delete `save_tasks` entirely:

```rust
    pub fn save(&mut self) {
        if self.load_failed {
            self.status = format!("not saving: {} could not be read", self.date);
            return;
        }
        if let Err(e) = self.store.save_day(self.date, &self.day, &self.tasks) {
            self.status = format!("save failed: {e}");
        }
    }
```

(e) Replace every remaining `self.save_tasks();` with `self.save();` (Grep for `save_tasks`: the `EditTask` Confirm and Cancel arms, `edit_task`; the `SaveNow` and `time_travel` sites are handled below). Then replace the `SaveNow` arm in `apply_normal` with:

```rust
            Command::SaveNow => {
                self.save();
                if self.status.is_empty() {
                    self.status = "saved".into();
                }
            }
```

(f) Replace `checkpoint_tasks`:

```rust
    fn checkpoint_tasks(&mut self) {
        self.push_history(Snapshot::Tasks(vec![(self.date, self.tasks.clone())]));
    }
```

(g) In `edit_task`, replace `self.push_history(Snapshot::Tasks(before));` with:

```rust
                self.push_history(Snapshot::Tasks(vec![(self.date, before)]));
```

(h) In `apply`, the `Mode::EditTask` `Command::Cancel` arm: replace

```rust
                    if is_new && let Some(Snapshot::Tasks(list)) = self.history.pop() {
                        self.tasks = list;
                        self.save_tasks();
                    }
```

with:

```rust
                    if is_new && let Some(Snapshot::Tasks(mut saved)) = self.history.pop() {
                        self.tasks = saved.remove(0).1;
                        self.save();
                    }
```

(i) In `time_travel`, replace the whole `Snapshot::Tasks(list) => { … }` arm with:

```rust
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
```

(j) Add `swap_tasks` right after `time_travel`:

```rust
    /// Puts each saved task list in place for its date and returns the lists it replaced, as a
    /// snapshot for the opposite stack. Then shows the first entry's day. Fails, changing
    /// nothing, when another day's file can't be read.
    fn swap_tasks(&mut self, saved: &[(NaiveDate, TaskList)]) -> Result<Snapshot, String> {
        let mut current = Vec::new();
        let mut other_days = Vec::new();
        for (date, _) in saved {
            if *date == self.date {
                current.push((*date, self.tasks.clone()));
            } else {
                let (day, tasks) = self
                    .store
                    .load_day(*date)
                    .map_err(|e| format!("load failed: {e}"))?;
                current.push((*date, tasks));
                other_days.push((*date, day));
            }
        }
        for (date, list) in saved {
            if *date == self.date {
                self.tasks = list.clone();
                self.save();
            } else if let Some((_, day)) = other_days.iter().find(|(d, _)| d == date)
                && let Err(e) = self.store.save_day(*date, day, list)
            {
                self.status = format!("save failed: {e}");
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
```

(k) Replace `load_current` so it loads the day's blocks and tasks from the one file:

```rust
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
```

- [ ] **Step 8: Run the whole suite and fmt**

Run: `cargo fmt && cargo test`
Expected: all tests PASS. If something fails to compile, Grep for a missed `save_tasks`, `load_tasks` or `tasks.txt`.

- [ ] **Step 9: Commit**

```bash
git add src/storage.rs src/state.rs src/ui/tasks.rs
git commit -m "feat: keep each day's tasks in its day file

A day file now holds the time blocks, then an optional [tasks] checklist. The global
tasks.txt is gone. Task undo snapshots carry their date.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Move a task to the previous / next day (state)

**Files:**
- Modify: `src/state.rs` (`Command` enum, `apply_tasks`, new `move_task_to_day`, tests)

**Interfaces:**
- Consumes: `TaskList::take_subtree`, `TaskList::append_subtree` (Task 1); `Store::load_day(date) -> (Day, TaskList)`, `Store::save_day(date, &Day, &TaskList)`, `Snapshot::Tasks(Vec<(NaiveDate, TaskList)>)` (Task 2).
- Produces (used by Task 4): `Command::MoveTaskToPrevDay`, `Command::MoveTaskToNextDay`.

- [ ] **Step 1: Write the failing tests**

At the end of `mod tests` in `src/state.rs`:

```rust
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
        assert_eq!(crate::storage::serialize_tasks(&tasks), "- [ ] y\n- [ ] a\n");
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib state::tests::move_task`
Expected: compile error, "no variant named `MoveTaskToNextDay`".

- [ ] **Step 3: Implement**

In `src/state.rs`:

(a) In `enum Command`, after `ScheduleTask,` add:

```rust
    /// Move the selected task (and its subtasks) to the previous / next day's list.
    MoveTaskToPrevDay,
    MoveTaskToNextDay,
```

(b) In `apply_tasks`, add before `Command::Delete =>`:

```rust
            Command::MoveTaskToPrevDay => self.move_task_to_day(self.date.pred_opt()),
            Command::MoveTaskToNextDay => self.move_task_to_day(self.date.succ_opt()),
```

(c) Add this method right after `edit_task`:

```rust
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
        let saved = vec![(self.date, self.tasks.clone()), (target, other.clone())];
        let sub = self.tasks.take_subtree(idx);
        other.append_subtree(sub);
        if let Err(e) = self.store.save_day(target, &other_day, &other) {
            self.tasks = saved[0].1.clone();
            self.status = format!("save failed: {e}");
            return;
        }
        self.push_history(Snapshot::Tasks(saved));
        self.task_sel = idx.min(self.tasks.len().saturating_sub(1));
        self.save();
        if self.status.is_empty() {
            self.status = format!("moved to {target}");
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo fmt && cargo test`
Expected: all PASS, including the 6 new `move_task…` tests.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs
git commit -m "feat: move a task to the previous or next day

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Keys, hints, help and README

**Files:**
- Modify: `src/ui/mod.rs` (`key_to_command`, `text_to_command`, tests)
- Modify: `src/ui/chrome.rs` (`hints`, `HELP`, `status_bar`, test)
- Modify: `README.md`

**Interfaces:**
- Consumes: `Command::MoveTaskToPrevDay`, `Command::MoveTaskToNextDay` (Task 3).
- Produces: nothing used later.

- [ ] **Step 1: Write the failing tests**

In `src/ui/mod.rs` `mod tests`:

1. In `shortcuts_only_outside_editing`, add two rows to the array (after `("t", Command::Today),`):

```rust
            ("[", Command::MoveTaskToPrevDay),
            ("]", Command::MoveTaskToNextDay),
```

2. Add a new test at the end of `mod tests`:

```rust
    #[test]
    fn alt_shift_arrows_move_a_task_between_days() {
        let n = &Mode::Normal;
        assert_eq!(
            key_to_command(Key::ArrowLeft, M::ALT | M::SHIFT, n),
            Some(Command::MoveTaskToPrevDay)
        );
        assert_eq!(
            key_to_command(Key::ArrowRight, M::ALT | M::SHIFT, n),
            Some(Command::MoveTaskToNextDay)
        );
        // Shift+arrows alone still change the day shown.
        assert_eq!(
            key_to_command(Key::ArrowRight, M::SHIFT, n),
            Some(Command::NextDay)
        );
        let e = &Mode::EditTask {
            idx: 0,
            original: String::new(),
            is_new: false,
        };
        assert_eq!(key_to_command(Key::ArrowLeft, M::ALT | M::SHIFT, e), None);
    }
```

In `src/ui/chrome.rs` test `footer_names_pane_and_lists_its_keys`, after `assert!(keys(&s).contains(&"Space"));` add:

```rust
        assert!(keys(&s).contains(&"[ ]"));
```

and at the end of that test add:

```rust
        assert!(HELP.iter().any(|(k, _)| k.contains("Alt+Shift+←")));
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib ui::`
Expected: FAIL (the new rows return `None`; the hint and help line are missing).

- [ ] **Step 3: Implement the key mapping**

In `src/ui/mod.rs` `key_to_command`, in the `Mode::Normal` match, after the line `Key::ArrowRight if m.shift && !m.command && !m.alt => C::NextDay,` add:

```rust
            Key::ArrowLeft if m.alt && m.shift && !m.command => C::MoveTaskToPrevDay,
            Key::ArrowRight if m.alt && m.shift && !m.command => C::MoveTaskToNextDay,
```

In `text_to_command`, in the `Mode::Normal` match, after `"p" => Some(Command::ScheduleTask),` add:

```rust
            "[" => Some(Command::MoveTaskToPrevDay),
            "]" => Some(Command::MoveTaskToNextDay),
```

- [ ] **Step 4: Implement hints, help and status colour**

In `src/ui/chrome.rs`:

(a) In `hints`, in the `Focus::Tasks` list, add `("[ ]", "move day"),` after `("p", "plan"),`:

```rust
                ("p", "plan"),
                ("[ ]", "move day"),
                ("→", "time"),
```

(b) In `HELP`, after `("p", "plan task on the timeline"),` add:

```rust
    ("[ ], Alt+Shift+← →", "move task to previous / next day"),
```

(c) In `status_bar`, replace the status colour check

```rust
                        let status_color = if state.status == "saved" {
```

with:

```rust
                        let status_color = if state.status == "saved"
                            || state.status.starts_with("moved to ")
                        {
```

- [ ] **Step 5: Update the README**

In `README.md`:

(a) In the intro, change "A nested task list sits beside the timeline;" to "A nested task list for the same day sits beside the timeline;".

(b) In the "Data folder" section, replace "Each day is one plaintext file, `YYYY-MM-DD.txt`:" and its code block with:

````
Each day is one plaintext file, `YYYY-MM-DD.txt`, holding that day's time blocks and, after a `[tasks]` line, that day's task list:

```
09:00-10:30 deep Write quarterly report
11:00-11:30 scheduled Standup
13:00-13:15 standard

[tasks]
- [ ] Call the bank
  - [x] Find the number
```
````

(c) Change "If you remove every block from a day, that day's file is deleted." to "If a day has no blocks and no tasks, its file is deleted."

(d) Replace the paragraph "The task list is saved to `tasks.txt` in the same folder, as a nested Markdown checklist:" and the code block that follows it with:

```
The tasks after `[tasks]` are a nested Markdown checklist, one list per day:
```

(keep the bullet rules below it, but in the last bullet change "The app writes the file back in this normalized form, and deletes it when the list is empty." to "The app writes the tasks back in this normalized form, and leaves out the `[tasks]` line when the day has no tasks.")

(e) In the keys table, change the `Shift+←` `Shift+→` row text to `previous / next day (blocks and tasks)` in both columns, and add a new row after the `p` row:

```
| `[` `]`, `Alt+Shift+←` `Alt+Shift+→` | | move the task and its subtasks to the end of the previous / next day's list (you stay on this day; one undo restores both days) |
```

- [ ] **Step 6: Run everything**

Run: `cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all PASS, no clippy warnings. (If `cargo clippy` is not installed, skip it and say so.)

- [ ] **Step 7: Manual check**

Run: `cargo run -- --dir <scratch dir>` (use a temp folder, not the real data folder).
1. Make one block, then focus tasks (`←`) and add two tasks (`n`, type, `Enter`). Open `<scratch dir>/<today>.txt`: the block line, a blank line, `[tasks]`, then the two tasks.
2. `Shift+→`: the task list and timeline are empty (new day). `Shift+←` returns.
3. Select the first task, press `]`: it disappears, the status bar shows `moved to <tomorrow>` in normal colour.
4. `Shift+→`: the task is there. Press `Alt+Shift+←`: it goes back.
5. `Ctrl+Z`: the task returns to where it was before the last move; `Ctrl+Shift+Z` redoes.
6. In the tasks pane, `n` then type `[` and `]`: they appear as text, no move.

- [ ] **Step 8: Commit**

```bash
git add src/ui/mod.rs src/ui/chrome.rs README.md
git commit -m "feat: shortcuts, hints and docs for daily tasks

[ and ] (or Alt+Shift+Left/Right) move a task to the previous or next day.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage**
- "Tasks should also be daily": Task 2 (tasks load with the day, `Today`/`PrevDay`/`NextDay` switch both panes, dated undo).
- "Day and time blocks saved to a single file": Task 2 (`[tasks]` section in `YYYY-MM-DD.txt`; `tasks.txt` removed; one load/save/unreadable flag).
- "Shortcut to move a task to the next/previous day": Task 3 (state, one undo step, subtasks go along, other day's blocks kept) and Task 4 (`[`/`]` and `Alt+Shift+←/→`, footer hint, help, README).
- No history/migration: nothing reads or renames the old `tasks.txt`.

**Placeholder scan:** no TBD/TODO; every code step has complete code. The one conditional (Task 2 Step 5, the timeline-flow test) gives the exact fallback code.

**Type consistency:** `Store::load_day(date) -> (Day, TaskList)` and `save_day(date, &Day, &TaskList)` (Task 2) are used the same way in Task 3 and in the tests. `Snapshot::Tasks(Vec<(NaiveDate, TaskList)>)` is built in `checkpoint_tasks`, `edit_task`, `move_task_to_day` and read in `time_travel`, `swap_tasks` and the cancel arm. `Command::MoveTaskToPrevDay` / `MoveTaskToNextDay` are spelled the same in state, ui and tests. `take_subtree` / `append_subtree` match Task 1. `D_FILE` is defined in Task 2 and reused in Task 3.

**Known limits (on purpose, YAGNI):** no "copy" to another day, no moving blocks between days, no rollover of unfinished tasks, no move by more than one day at a time. A day file that can't be read blocks saving that day's blocks and tasks alike.
