# Task Manager & Block Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a nested task manager in a left pane, with tasks that can be turned into time blocks. Blocks can be moved by dragging. Shortcuts become single keys that never fire while typing. The view opens at 08:00 and shows a current-time line. Blocks are drawn solid, with a shape in the corner showing their type.

**Architecture:**
- Same layering as today: a pure core (`model`, `tasks`, `storage`, `theme`, `state`) with unit tests, and a thin egui layer in `ui/`.
- Tasks are a flat `Vec<Task>` in display order, each with a `depth`. Nesting falls out of the order and depths, so a subtree is a contiguous slice.
- `AppState` gains a `Focus` (which pane has the keyboard). Commands like `Up`, `New` and `Delete` act on the focused pane.
- Undo becomes one history of `Snapshot`s, each either a day or the task list.

**Tech Stack:** unchanged. Rust edition 2024, eframe/egui 0.37.0, chrono, dirs, and tempfile for tests.

**Spec:** the existing app plan, `docs/superpowers/plans/2026-11-10-time-block-planner.md` (its Global Constraints still apply), plus this request, reproduced verbatim:

> Add a simple task manager, as well. It should allow nested tasks and creating a time block from a task. Also, it should be possible to drag time blocks to a different time. The task manager should be on the left of the UI and the time block planner should be to the right. I prefer single-key shortcuts so long as they don't trigger while you're typing. Keep familair shortcuts like ctrl+z, but n for new is ok. Start the window at 8 am. Show a line for the current time. I don't like the bands created by the highlighting of calendar blocks. Make all blocks more solid, and use a different indicator (Like a different shape in the corner of the event) to highlight the event type.

## Global Constraints

- **Layout:** the tasks pane is on the left and the timeline on the right. Exactly one pane has keyboard focus at a time, and `Tab` switches between them.
- **Single-key shortcuts:** they act only in Normal mode. While a title is being edited (`EditTitle` / `EditTask`), every printable key is text.
  - Letter and symbol shortcuts are read from `egui::Event::Text`, never from `Event::Key`, so they follow the keyboard layout and never fire twice.
  - No shortcut needs AltGr, because egui drops AltGr text (issue #2).
- **Familiar chords stay:** `Ctrl+Z`, `Ctrl+Shift+Z`/`Ctrl+Y`, `Ctrl+S`, `Ctrl+N`, `Ctrl+T`, `Ctrl+,`, `Ctrl+Q`, `F1`, `F2`.
- **Starting position:** on launch the cursor is at 08:00 (`Slot(32)`), and the timeline is scrolled so the 08:00 row is at the top.
- **Current-time line:** drawn only when the day shown is today, at the exact minute, and repainted at least once a minute.
- **Block look:**
  - no `░` shading, no tags, and no cursor band inside blocks;
  - every block is an opaque fill;
  - the type is shown by a shape in the block's top-right corner: standard ● circle, scheduled ■ square, deep ▲ triangle.
- **Task storage:** `<dir>/tasks.txt`, plaintext and hand-editable (format below). Same safety rules as day files:
  - atomic write;
  - deleted when the list is empty;
  - never written while the file is unreadable.
- **Undo:** one history covers both block and task edits.

## tasks.txt format (decided here, used by Task 2)

```
- [ ] Write report
  - [x] Outline
  - [ ] Draft
- [ ] Call bank
```

- **Nesting:** two spaces per level. A tab counts as two spaces.
- **Markers:** `- [ ] ` means open and `- [x] ` (or `[X]`) means done.
- **Forgiving parsing:** any other non-blank line becomes an open task whose title is the trimmed line, so nothing typed by hand is lost. The depth is clamped to at most the previous task's depth + 1 (the first task gets 0) and to at most `MAX_DEPTH = 8`.
- **Cleanup:** a leading BOM and blank lines are dropped. Writing always uses the normalized form shown above.

## Keymap (decided here; replaces the old table)

`Ctrl` = Cmd on macOS. "T" means the key acts in the tasks pane, "B" in the timeline (blocks) pane; "both" means either pane.

| Key | Command | Pane |
|---|---|---|
| `Tab` | `SwitchPane` | both |
| `←` / `→` | `PrevDay` / `NextDay` | both |
| `t`, `Ctrl+T` | `Today` | both |
| `↑` / `↓` | `Up` / `Down`: move the slot cursor, or the selected task | both |
| `PageUp` / `PageDown` | B: ±4 slots · T: ±10 tasks | both |
| `Home` / `End` | B: 00:00 / 23:45 · T: first / last task | both |
| `Ctrl+↑` / `Ctrl+↓` | `PrevBlock` / `NextBlock` | B |
| `n`, `Ctrl+N` | `New`: block at the cursor · task after the selected task's subtree, at the same depth | both |
| `N` (Shift+n) | `NewChild`: subtask directly under the selected task | T |
| `Enter`, `F2`, `e` | `EditTitle`: rename the block or task | both |
| `d`, `Delete`, `Backspace` | `Delete`: a block, or a task with its subtree | both |
| `Alt+↑` / `Alt+↓` | `MoveUp` / `MoveDown`: move the block 15 min · swap the task with its sibling | both |
| `Alt+Shift+↑` / `Alt+Shift+↓` | `Shrink` / `Grow` | B |
| `Shift+↑` / `Shift+↓` | `ExtendUp` / `ExtendDown` | B |
| `1` `2` `3` | `SetKind` | B |
| `x`, `Space` | `ToggleDone` | T |
| `Alt+→`, `>` / `Alt+←`, `<` | `Indent` / `Outdent` | T |
| `b` | `ScheduleTask`: switch to the timeline and start creating a block at the cursor, titled with the task | T |
| `Ctrl+Z` / `Ctrl+Shift+Z`, `Ctrl+Y` | `Undo` / `Redo` | both |
| `Ctrl+S`, `Ctrl+,`, `F1`/`?`, `Ctrl+Q` | save, themes, help, quit (unchanged) | both |

Modes:
- **Create**, **EditTitle**, **ThemePicker** and **Help** work as before.
- **EditTask** works like EditTitle. Esc on a just-created task removes it.
- **MoveBlock** (mouse only) is cancelled by `Esc`.

Mouse:
- **Timeline:** drag in empty space to create a block; drag a block to move it; click to place the cursor.
- **Tasks pane:** click a task to select it; click its `[ ]` to toggle it. Drag a task onto the timeline to create a 1-hour block there.

## Review Focus

1. **Deleting a parent task.** Expected: the whole subtree is removed, and one `Ctrl+Z` brings all of it back. Covered by Task 3 test `delete_parent_removes_subtree_and_undo_restores`.
2. **Typing a task title that contains shortcut characters** (`n d x b > <`, space, `1`). Expected: they all become text. Covered by Task 3 test `task_edit_swallows_shortcuts` and Task 5 test `shortcuts_only_outside_editing`.
3. **Dragging a block onto another block or past midnight.** Expected: an overlap is refused with a status message and the block stays put; the target is clamped inside the day. Covered by Task 4 tests `drag_move_refuses_overlap` and `drag_move_clamps_to_day`.
4. **Hand-edited or unreadable `tasks.txt`.** Expected: odd indentation and plain lines become tasks, and an unreadable file is never overwritten. Covered by Task 2 test `parse_tasks_tolerates_hand_edits` and Task 3 test `unreadable_tasks_never_overwritten`.
5. **Outdenting a task that has later siblings.** Expected: those siblings stay with the original parent and are not adopted by the moved task. Covered by Task 1 test `outdent_keeps_following_siblings`.

---

## File Structure

```
src/tasks.rs           # NEW Task, TaskList, tree ops on a flat depth list
src/storage.rs         # + parse_tasks, serialize_tasks, Store::load_tasks/save_tasks
src/theme.rs           # + pub luminance, Theme::block_fill, Theme::text_on
src/state.rs           # + Focus, Snapshot undo, task commands, MoveBlock, schedule/drop task, START_SLOT
src/ui/mod.rs          # keymap rewrite, two-pane layout, initial 08:00 scroll
src/ui/timeline.rs     # solid blocks, corner markers, selection outline, now line, move ghost, task drop target
src/ui/tasks.rs        # NEW tasks pane: rows, selection, checkbox click, drag source
src/ui/chrome.rs       # help text update
README.md              # keymap + tasks.txt format
```

---

### Task 1: Task list model

**Files:**
- Create: `src/tasks.rs`. Modify: `src/lib.rs` (`pub mod tasks;`).
- Test: unit tests in `src/tasks.rs`

**Interfaces:**
- Produces:
  - `pub const MAX_DEPTH: u8 = 8;`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Task { pub title: String, pub done: bool, pub depth: u8 }`
  - `#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct TaskList { items: Vec<Task> }`
  - `impl TaskList`:
    - `fn from_items(items: Vec<Task>) -> TaskList` (normalizes depths: the first item is 0, and each is ≤ the previous + 1 and ≤ `MAX_DEPTH`)
    - `fn items(&self) -> &[Task]`, `fn len(&self) -> usize`, `fn is_empty(&self) -> bool`
    - `fn subtree_end(&self, i: usize) -> usize` (exclusive: the first index after `i` whose depth is ≤ `items[i].depth`)
    - `fn insert(&mut self, after: Option<usize>, child: bool) -> usize`: inserts an open, untitled task and returns its index.
      - `None`: appended at depth 0.
      - Sibling: inserted at `subtree_end(after)`, with the same depth as `after`.
      - Child: inserted at `after + 1`, at `after`'s depth + 1, capped at `MAX_DEPTH`.
    - `fn remove(&mut self, i: usize) -> usize` (removes the subtree and returns how many items were removed)
    - `fn set_title(&mut self, i: usize, t: &str)` (same cleanup as `Day::set_title`: trim, and newlines become spaces)
    - `fn toggle_done(&mut self, i: usize)`
    - `fn indent(&mut self, i: usize) -> bool`: allowed only when a previous sibling exists. Scan back from `i-1` over deeper items; the first item that is not deeper must have the same depth. Adds 1 to the depth of the whole subtree. Returns `false` if there is no previous sibling or the subtree would exceed `MAX_DEPTH`.
    - `fn outdent(&mut self, i: usize) -> Option<usize>`: `None` at depth 0. Otherwise it takes the subtree out, subtracts 1 from each depth, and reinserts it just after the end of the parent's (original) subtree. Returns the new index.
    - `fn move_up(&mut self, i: usize) -> Option<usize>` / `fn move_down(&mut self, i: usize) -> Option<usize>`: swaps the subtree with the previous/next sibling's subtree. `None` if there is no such sibling. Returns `i`'s new index.

- [ ] **Step 1: Write the failing tests.** The helper `list(&[(depth, title)])` builds the list with `from_items`, and `titles(&l)` returns `Vec<(u8, &str)>`.

```rust
#[test] fn from_items_normalizes_depth() {
    let l = list(&[(2, "a"), (5, "b"), (1, "c")]);
    assert_eq!(titles(&l), [(0, "a"), (1, "b"), (1, "c")]);
}
#[test] fn insert_sibling_goes_after_subtree() {
    let mut l = list(&[(0, "a"), (1, "a1"), (0, "b")]);
    assert_eq!(l.insert(Some(0), false), 2);
    assert_eq!(l.items()[2].depth, 0);
    assert_eq!(l.insert(Some(0), true), 1);
    assert_eq!(l.items()[1].depth, 1);
    assert_eq!(l.insert(None, false), l.len() - 1);
}
#[test] fn remove_takes_subtree() {
    let mut l = list(&[(0, "a"), (1, "a1"), (2, "a1x"), (0, "b")]);
    assert_eq!(l.remove(0), 3);
    assert_eq!(titles(&l), [(0, "b")]);
}
#[test] fn indent_needs_previous_sibling() {
    let mut l = list(&[(0, "a"), (0, "b"), (1, "b1")]);
    assert!(!l.indent(0));
    assert!(l.indent(1));
    assert_eq!(titles(&l), [(0, "a"), (1, "b"), (2, "b1")]);
    assert!(!l.indent(2)); // b1 has no previous sibling at depth 2
}
#[test] fn outdent_keeps_following_siblings() {
    let mut l = list(&[(0, "p"), (1, "a"), (2, "a1"), (1, "b"), (0, "q")]);
    assert_eq!(l.outdent(1), Some(2));
    assert_eq!(titles(&l), [(0, "p"), (1, "b"), (0, "a"), (1, "a1"), (0, "q")]);
    assert_eq!(l.outdent(2), None);
}
#[test] fn move_swaps_sibling_subtrees() {
    let mut l = list(&[(0, "a"), (1, "a1"), (0, "b")]);
    assert_eq!(l.move_down(0), Some(1));
    assert_eq!(titles(&l), [(0, "b"), (0, "a"), (1, "a1")]);
    assert_eq!(l.move_up(1), Some(0));
    assert_eq!(l.move_up(0), None);
    assert_eq!(l.move_down(2), None); // list is a, a1, b again; b is the last sibling
}
#[test] fn toggle_and_title() {
    let mut l = list(&[(0, "a")]);
    l.toggle_done(0); assert!(l.items()[0].done);
    l.set_title(0, " x\ny "); assert_eq!(l.items()[0].title, "x y");
}
```

- [ ] **Step 2: Run `cargo test --lib tasks`.** Expected: FAIL (does not compile).
- [ ] **Step 3: Implement `src/tasks.rs` with the interfaces above.**
- [ ] **Step 4: Run `cargo test --lib tasks`.** Expected: 7 passed.
- [ ] **Step 5: Commit** `feat: nested task list model`

---

### Task 2: tasks.txt storage

**Files:**
- Modify: `src/storage.rs`
- Test: unit tests in `src/storage.rs`

**Interfaces:**
- Consumes: `Task`, `TaskList::from_items` (Task 1).
- Produces:
  - `pub fn parse_tasks(text: &str) -> TaskList`
  - `pub fn serialize_tasks(list: &TaskList) -> String`
  - `Store::load_tasks(&self) -> io::Result<TaskList>` (a missing file → empty list)
  - `Store::save_tasks(&self, list: &TaskList) -> io::Result<()>` (uses `write_atomic`; an empty list removes the file)

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn tasks_roundtrip() {
    let txt = "- [ ] Write report\n  - [x] Outline\n  - [ ] Draft\n- [ ] Call bank\n";
    let l = parse_tasks(txt);
    assert_eq!(l.len(), 4);
    assert!(l.items()[1].done);
    assert_eq!(l.items()[2].depth, 1);
    assert_eq!(serialize_tasks(&l), txt);
}
#[test] fn parse_tasks_tolerates_hand_edits() {
    let l = parse_tasks("\u{feff}plain line\n\t\t- [X] deep jump\n\n    - [ ]   spaced  \n");
    let got: Vec<_> = l.items().iter().map(|t| (t.depth, t.done, t.title.as_str())).collect();
    assert_eq!(got, [(0, false, "plain line"), (1, true, "deep jump"), (2, false, "spaced")]);
}
#[test] fn tasks_file_roundtrip_and_empty_deletes() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store { dir: tmp.path().join("d") };
    assert!(store.load_tasks().unwrap().is_empty());
    let l = parse_tasks("- [ ] a\n");
    store.save_tasks(&l).unwrap();
    assert_eq!(store.load_tasks().unwrap(), l);
    store.save_tasks(&TaskList::default()).unwrap();
    assert!(!store.dir.join("tasks.txt").exists());
}
```

- [ ] **Step 2: Run `cargo test --lib storage`.** Expected: FAIL (does not compile).
- [ ] **Step 3: Implement the functions.** Parse each line in three steps:
  1. Count the leading indent (spaces, plus 2 per tab) and set depth = indent / 2.
  2. Strip a leading `- [ ] ` / `- [x] ` / `- [X] ` and set `done` accordingly.
  3. Trim the rest to get the title.

  Then `TaskList::from_items` clamps the depths.
- [ ] **Step 4: Run `cargo test --lib storage`.** Expected: all storage tests pass.
- [ ] **Step 5: Commit** `feat: tasks.txt persistence`

---

### Task 3: State — focus, task commands, unified undo

**Files:**
- Modify: `src/state.rs`
- Test: unit tests in `src/state.rs` (existing tests are updated for the renames below)

**Interfaces:**
- Consumes: Task 1 `TaskList` ops; Task 2 `load_tasks` / `save_tasks`.
- Produces:
  - `#[derive(Copy, Clone, Debug, PartialEq, Eq)] pub enum Focus { Tasks, Timeline }` (initial value: `Timeline`)
  - `Command` changes:
    - Rename `NewBlock` → `New`.
    - Add `NewChild, ToggleDone, Indent, Outdent, ScheduleTask, SwitchPane`.
  - `Mode` changes:
    - `Create { anchor: Slot, title: Option<String> }`
    - add `EditTask { idx: usize, original: String, is_new: bool }`
    - add `MoveBlock { idx: usize, grab: u16, target: Slot }` (used in Task 4)
  - New `AppState` fields:
    - `pub focus: Focus`, `pub tasks: TaskList`, `pub task_sel: usize`
    - `tasks_load_failed: bool`
    - the history type becomes `Vec<Snapshot>`, where `enum Snapshot { Day(NaiveDate, Day), Tasks(TaskList) }` is private
  - `pub fn selected_task(&self) -> Option<usize>` (`None` if `tasks` is empty, otherwise `task_sel` clamped to `len-1`)
  - `pub fn save_tasks(&mut self)` (refuses with `"not saving: tasks.txt could not be read"` when `tasks_load_failed`; an error becomes `"save failed: {e}"`)

Behavior rules (Normal mode, `focus == Tasks`):
- **Moving the selection:** `Up`/`Down` ±1, `PageUp`/`PageDown` ±10, `Home`/`End`; all clamped.
- **New tasks:** `New` → `insert(selected, false)` and `NewChild` → `insert(selected, true)`. Then select the new task and enter `EditTask { is_new: true }`. Checkpoint the tasks before inserting.
- **Editing:**
  - `EditTitle` → `EditTask { is_new: false }`, with the buffer set to the title.
  - `Delete`, `ToggleDone`, `Indent`, `Outdent`, `MoveUp` and `MoveDown` checkpoint the tasks, apply the change, keep the selection on the moved item, and save the tasks.
  - A refused change (indent or move returning `false`/`None`) sets the status `"can't move there"` and records no checkpoint.
- **Ignored here:** timeline-only commands (`SetKind`, `Extend*`, `Grow`, `Shrink`, `Prev/NextBlock`).

Shared commands with `focus == Timeline` behave as before, except `ToggleDone`, `Indent`, `Outdent`, `NewChild` and `ScheduleTask`, which are ignored. `SwitchPane` toggles the focus in either pane. `PrevDay`, `NextDay`, `Today`, `Undo`, `Redo`, `SaveNow`, `OpenThemes` and `ToggleHelp` work in both panes.

EditTask mode mirrors EditTitle:
- `Text`/`Backspace` edit the buffer.
- `Confirm` sets the title. If `!is_new` and the title changed, it checkpoints first. Then it saves the tasks.
- `Cancel` with `is_new` pops the checkpoint, which restores the list without the new task. Cancel without `is_new` simply leaves edit mode.

Undo/Redo pops a `Snapshot`:
- `Day` works exactly as today.
- `Tasks` pushes the current tasks onto the opposite stack, restores the snapshot, clamps `task_sel`, and saves the tasks.

`AppState::new` loads the tasks. On error it starts with an empty list, sets `tasks_load_failed` and shows `"load failed: {e}"`.

- [ ] **Step 1: Write the failing tests.** The helper `st_tasks(txt)` writes `tasks.txt` into the tempdir, then builds the state with `focus = Focus::Tasks`.

```rust
#[test] fn new_task_and_child_flow() {
    let (_t, mut s) = st(); s.focus = Focus::Tasks;
    s.apply(Command::New, D); s.apply(Command::Text("Report".into()), D); s.apply(Command::Confirm, D);
    s.apply(Command::NewChild, D); s.apply(Command::Text("Outline".into()), D); s.apply(Command::Confirm, D);
    assert_eq!(s.store.load_tasks().unwrap(), parse_tasks("- [ ] Report\n  - [ ] Outline\n"));
    assert_eq!(s.selected_task(), Some(1));
}
#[test] fn task_edit_swallows_shortcuts() {
    let (_t, mut s) = st_tasks("- [ ] a\n");
    s.apply(Command::EditTitle, D);
    for c in [Command::Delete, Command::ToggleDone, Command::Indent, Command::ScheduleTask, Command::New, Command::SwitchPane] { s.apply(c, D); }
    assert_eq!(s.tasks.len(), 1); assert!(!s.tasks.items()[0].done);
    assert_eq!(s.focus, Focus::Tasks); assert!(matches!(s.mode, Mode::EditTask { .. }));
}
#[test] fn delete_parent_removes_subtree_and_undo_restores() {
    let (_t, mut s) = st_tasks("- [ ] p\n  - [ ] c\n- [ ] q\n");
    s.apply(Command::Delete, D);
    assert_eq!(s.tasks.len(), 1);
    s.apply(Command::Undo, D);
    assert_eq!(s.tasks.len(), 3);
    assert_eq!(s.store.load_tasks().unwrap().len(), 3);
}
#[test] fn undo_interleaves_tasks_and_blocks() {
    let (_t, mut s) = st_tasks("- [ ] a\n");
    s.apply(Command::ToggleDone, D);                       // task edit
    s.apply(Command::SwitchPane, D); s.cursor = Slot(10);
    s.apply(Command::New, D); s.apply(Command::Confirm, D); s.apply(Command::Confirm, D); // block edit
    s.apply(Command::Undo, D); assert!(s.day.blocks().is_empty()); assert!(s.tasks.items()[0].done);
    s.apply(Command::Undo, D); assert!(!s.tasks.items()[0].done);
}
#[test] fn cancel_new_task_removes_it() { /* New, Text("x"), Cancel -> tasks empty; Undo -> "nothing to undo" */ }
#[test] fn refused_indent_sets_status() { /* "- [ ] a\n": Indent -> status "can't move there"; Undo -> "nothing to undo" */ }
#[test] fn switch_pane_toggles_focus() { /* Timeline -> SwitchPane -> Tasks -> SwitchPane -> Timeline */ }
#[test] fn unreadable_tasks_never_overwritten() {
    // write [0xff, 0xfe] to tasks.txt; AppState::new -> status starts "load failed";
    // focus Tasks; New + Text + Confirm -> status starts "not saving"; file bytes unchanged
}
```

- [ ] **Step 2: Run `cargo test --lib state`.** Expected: FAIL (does not compile).
- [ ] **Step 3: Implement the interfaces and rules above.** Rename `NewBlock` → `New` in the existing tests. Existing assertions on `Mode::Create` use `matches!`, so they are unaffected.
- [ ] **Step 4: Run `cargo test --lib`.** Expected: all pass. The ui tests may fail to compile because of the renames; fix them by renaming `NewBlock` → `New` and adding `title: None` to `Mode::Create`. Their full update comes in Task 5.
- [ ] **Step 5: Commit** `feat: task commands, pane focus, unified undo`

---

### Task 4: State — drag-to-move blocks, block from task, 08:00 start

**Files:**
- Modify: `src/state.rs`
- Test: unit tests in `src/state.rs`

**Interfaces:**
- Consumes: Task 3 `Focus`, `Mode::{Create, MoveBlock}`, `selected_task`, `checkpoint`.
- Produces:
  - `const START_SLOT: Slot = Slot(32);` replaces `START_OF_WORKDAY` and is the initial cursor
  - `pub fn move_preview(&self) -> Option<(Slot, Slot)>` (in MoveBlock: `(target, target + len)`)
  - `pub fn drop_task(&mut self, task_idx: usize, slot: Slot)`
  - Changed drag API behavior (signatures unchanged):
    - `drag_start` on an occupied slot (Normal mode) → `MoveBlock { idx, grab: slot - start, target: start }`, and sets `focus = Timeline`. On a free slot → `Create { anchor, title: None }`, and sets `focus = Timeline`.
    - `drag_update` in MoveBlock → `target = clamp(slot - grab, 0, 96 - len)`.
    - `drag_end` in MoveBlock → if `target == start`, back to Normal. Otherwise the cursor moves onto the block and `shift(idx, target - start)` runs through the existing `edit` path: one checkpoint, saved, refusal → status `"overlaps another block"`. Then back to Normal.
    - `Cancel` in MoveBlock → Normal, with nothing changed.
  - `ScheduleTask` (Tasks focus, a task selected) sets `focus = Timeline`. If the cursor slot is free it enters `Create { anchor: cursor, title: Some(task title) }`; otherwise the status is `"slot occupied"`. `Confirm` in Create with `title: Some(t)` adds the block titled `t` and returns to Normal mode (no title edit).
  - `drop_task` uses `free_range(slot, Slot(slot.0 + 3))` (a 1-hour block, clipped). `None` → status `"slot occupied"`. Otherwise it checkpoints, adds a Standard block with the task's title, sets the cursor to its start and `focus = Timeline`, and saves.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn starts_at_eight() { let (_t, s) = st(); assert_eq!(s.cursor, Slot(32)); assert_eq!(s.focus, Focus::Timeline); }
#[test] fn drag_moves_block() {
    let (_t, mut s) = st_with_block(36, 40);
    s.drag_start(Slot(37)); s.drag_update(Slot(45));
    assert_eq!(s.move_preview(), Some((Slot(44), Slot(48))));
    s.drag_end();
    assert_eq!((s.day.blocks()[0].start, s.day.blocks()[0].end), (Slot(44), Slot(48)));
    assert_eq!(s.store.load_day(D).unwrap(), s.day);
    s.apply(Command::Undo, D); assert_eq!(s.day.blocks()[0].start, Slot(36));
}
#[test] fn drag_move_refuses_overlap() {
    let (_t, mut s) = st_with_block(36, 40);
    s.day.add(Block { start: Slot(50), end: Slot(52), kind: Kind::Deep, title: String::new() }).unwrap();
    s.drag_start(Slot(36)); s.drag_update(Slot(49)); s.drag_end();
    assert_eq!(s.status, "overlaps another block");
    assert_eq!(s.day.blocks()[0].start, Slot(36)); assert_eq!(s.mode, Mode::Normal);
}
#[test] fn drag_move_clamps_to_day() {
    let (_t, mut s) = st_with_block(36, 40);
    s.drag_start(Slot(36)); s.drag_update(Slot(95));
    assert_eq!(s.move_preview(), Some((Slot(92), Slot(96))));
    s.drag_update(Slot(0)); s.drag_start(Slot(0));
    assert_eq!(s.move_preview(), Some((Slot(0), Slot(4))));
}
#[test] fn escape_cancels_move() { /* drag_start(36), drag_update(50), apply(Cancel) -> Normal, block still at 36 */ }
#[test] fn schedule_task_creates_titled_block() {
    let (_t, mut s) = st_tasks("- [ ] Write report\n");
    s.cursor = Slot(40);
    s.apply(Command::ScheduleTask, D);
    assert_eq!(s.focus, Focus::Timeline);
    s.apply(Command::Down, D); s.apply(Command::Confirm, D);
    assert_eq!(s.mode, Mode::Normal);
    let b = &s.day.blocks()[0];
    assert_eq!((b.start, b.end, b.title.as_str()), (Slot(40), Slot(42), "Write report"));
}
#[test] fn drop_task_makes_one_hour_block_clipped() {
    let (_t, mut s) = st_tasks("- [ ] a\n");
    s.day.add(Block { start: Slot(42), end: Slot(44), kind: Kind::Deep, title: String::new() }).unwrap();
    s.drop_task(0, Slot(40));
    assert_eq!(s.day.block_at(Slot(40)).map(|i| s.day.blocks()[i].end), Some(Slot(42)));
    s.drop_task(0, Slot(43)); assert_eq!(s.status, "slot occupied");
}
```

(In `drag_move_clamps_to_day`, `drag_start(Slot(0))` while in MoveBlock is ignored, because `drag_start` only acts in Normal mode. It is there to prove that.)

- [ ] **Step 2: Run `cargo test --lib state`.** Expected: the new tests FAIL.
- [ ] **Step 3: Implement.** Update the existing test `drag_clips_at_neighbor`: pressing on an occupied slot and releasing without moving is still a no-op that ends in `Mode::Normal`, so its assertions remain true.
- [ ] **Step 4: Run `cargo test --lib`.** Expected: all pass.
- [ ] **Step 5: Commit** `feat: drag blocks to move, schedule tasks as blocks, open at 08:00`

---

### Task 5: Keymap — single-key shortcuts

**Files:**
- Modify: `src/ui/mod.rs` (`key_to_command`, `text_to_command`, tests)

**Interfaces:**
- Consumes: Task 3 and Task 4 `Command`/`Mode`.
- Produces (signatures unchanged):
  - `key_to_command(key, mods, mode)`, implementing the non-text rows of the Keymap. Changes from today:
    - `Tab` → `SwitchPane` (Shift ignored).
    - `Ctrl+↑`/`Ctrl+↓` → `PrevBlock`/`NextBlock`.
    - `Alt+←`/`Alt+→` → `Outdent`/`Indent`.
    - `Ctrl+N` → `New`.
    - In `EditTask` mode it behaves like `EditTitle`.
    - In `MoveBlock` mode, only `Escape` → `Cancel` is mapped.
  - `text_to_command(text, mode)`:
    - In Normal mode: `n`→`New`, `N`→`NewChild`, `e`→`EditTitle`, `d`→`Delete`, `t`→`Today`, `x` and `" "`→`ToggleDone`, `b`→`ScheduleTask`, `>`→`Indent`, `<`→`Outdent`, `1`/`2`/`3`→`SetKind`, `?`→`ToggleHelp`.
    - In `EditTitle` and `EditTask` mode it returns `Text`.
    - In Help mode, `?` → `ToggleHelp`.

- [ ] **Step 1: Update and add the failing tests**

```rust
#[test] fn shortcuts_only_outside_editing() {
    let n = &Mode::Normal;
    let et = &Mode::EditTask { idx: 0, original: String::new(), is_new: true };
    for (ch, cmd) in [("n", Command::New), ("N", Command::NewChild), ("x", Command::ToggleDone), (" ", Command::ToggleDone),
                      ("b", Command::ScheduleTask), (">", Command::Indent), ("<", Command::Outdent), ("d", Command::Delete),
                      ("e", Command::EditTitle), ("t", Command::Today)] {
        assert_eq!(text_to_command(ch, n), Some(cmd), "{ch}");
        assert_eq!(text_to_command(ch, et), Some(Command::Text(ch.into())), "{ch}");
    }
}
#[test] fn pane_and_outline_keys() {
    let n = &Mode::Normal;
    assert_eq!(key_to_command(Key::Tab, M::NONE, n), Some(Command::SwitchPane));
    assert_eq!(key_to_command(Key::ArrowDown, M::COMMAND, n), Some(Command::NextBlock));
    assert_eq!(key_to_command(Key::ArrowRight, M::ALT, n), Some(Command::Indent));
    assert_eq!(key_to_command(Key::ArrowLeft, M::ALT, n), Some(Command::Outdent));
    let mv = &Mode::MoveBlock { idx: 0, grab: 0, target: Slot(0) };
    assert_eq!(key_to_command(Key::Escape, M::NONE, mv), Some(Command::Cancel));
    assert_eq!(key_to_command(Key::ArrowDown, M::NONE, mv), None);
}
```

In `platform_shortcuts`, change the expectations for `Ctrl+↑` (now `PrevBlock`) and `Shift+Tab` (now `SwitchPane`), and rename `NewBlock` → `New`. In `backspace_maps_by_mode`, `text_to_command("x", Normal)` is now `Some(ToggleDone)`; change that assertion to `"z"` → `None`.

- [ ] **Step 2: Run `cargo test --lib ui`.** Expected: the new and changed assertions FAIL.
- [ ] **Step 3: Implement the mapping changes.**
- [ ] **Step 4: Run `cargo test --lib`.** Expected: all pass.
- [ ] **Step 5: Commit** `feat: single-key shortcuts that never fire while typing`

---

### Task 6: Timeline look — solid blocks, corner shapes, now line, move ghost

**Files:**
- Modify: `src/theme.rs`, `src/ui/timeline.rs`, `src/ui/mod.rs` (initial scroll)
- Test: unit tests in `src/theme.rs` and `src/ui/timeline.rs`

**Interfaces:**
- Consumes: Task 4 `move_preview`, `drop_task`, `START_SLOT`.
- Produces:
  - `theme.rs`:
    - `pub fn luminance(c: Rgb) -> f32` (made public)
    - `Theme::block_fill(&self, k: Kind) -> Rgb`: an opaque per-channel mix of 75% `kind_color(k)` and 25% `bg`
    - `Theme::text_on(&self, fill: Rgb) -> Rgb`: returns whichever of `fg`/`bg` has the higher WCAG contrast ratio against `fill`
  - `timeline.rs`:
    - `#[derive(Debug, PartialEq, Eq, Clone, Copy)] pub enum Marker { Circle, Square, Triangle }`
    - `pub fn marker(k: Kind) -> Marker` (Standard→Circle, Scheduled→Square, Deep→Triangle)
    - `pub fn now_offset(t: chrono::NaiveTime) -> f32` (fractional slots since midnight = seconds / 900)
    - `#[derive(Clone, Copy)] pub struct TaskDrag(pub usize);` (the drag-and-drop payload, used in Task 7)

Rendering rules:
- **Block body:**
  - Fill the block rect, shrunk 1 px top and bottom so adjacent blocks show a seam, with `block_fill(kind)`, fully opaque.
  - Draw the title clipped to the block (`painter.with_clip_rect`) in `text_on(fill)`.
  - Remove the `░` shading, the left bar and the `[ ]`/`[S]`/`[D]` tags.
- **Corner marker:** draw the shape in the top-right corner, inset by `char_w * 0.5`, with size `row_h * 0.45`, in `text_on(fill)`:
  - Circle: `circle_filled`
  - Square: `rect_filled`
  - Triangle: `convex_polygon`, pointing up
- **Selection:**
  - Cursor inside a block: outline the whole block, 2 px in `accent`.
  - Cursor on a free slot: outline that row as today.
  - The gutter `▶` is always drawn.
- **Gridlines:** draw the hour lines before the blocks, so the opaque blocks hide them. That removes the bands.
- **Move ghost:** in MoveBlock mode, draw `move_preview()` as a 1.5 px `accent` outline, and draw the original block at 50% opacity.
- **Now line:**
  - Only when `state.date == today`.
  - At y = top + `now_offset(Local::now().time()) * row_h`: a 2 px line in `theme.error`, spanning the body, with a 4 px circle at the gutter edge.
  - Call `request_repaint_after` for the seconds until the next minute.
- **Task drop target:** if `response.dnd_release_payload::<TaskDrag>()` returns `Some`, call `state.drop_task(payload.0, y_to_slot(pointer y))`. While a `TaskDrag` payload is being hovered over the timeline, outline the 4-slot drop target.
- **Opening at 08:00:** in `ui/mod.rs`, on the first frame scroll so row `START_SLOT` is at the top (`scroll_to_rect(.., Some(Align::Min))`). Set `scrolled_to` so that the cursor-follow scroll doesn't override it.

- [ ] **Step 1: Write the failing tests**

```rust
// theme.rs
#[test] fn block_fill_is_opaque_mix() {
    let t = &themes()[theme_index("gruvbox")];
    assert_eq!(t.block_fill(Kind::Deep), Rgb(0x89, 0x93, 0x56)); // per channel (0.75*a + 0.25*b).round(): a9b665 over 282828
}
#[test] fn text_on_picks_contrast() {
    for t in themes() { for k in [Kind::Standard, Kind::Scheduled, Kind::Deep] {
        let f = t.block_fill(k); let x = t.text_on(f);
        let other = if x == t.fg { t.bg } else { t.fg };
        assert!(contrast(x, f) >= contrast(other, f), "{} {:?}", t.name, k);
    } }
}
// timeline.rs
#[test] fn markers_are_distinct() {
    assert_eq!([marker(Kind::Standard), marker(Kind::Scheduled), marker(Kind::Deep)], [Marker::Circle, Marker::Square, Marker::Triangle]);
}
#[test] fn now_offset_in_slots() {
    assert_eq!(now_offset(NaiveTime::from_hms_opt(8, 30, 0).unwrap()), 34.0);
    assert_eq!(now_offset(NaiveTime::from_hms_opt(0, 7, 30).unwrap()), 0.5);
}
```

(`contrast(a, b)` is the WCAG ratio `(L1 + 0.05) / (L2 + 0.05)`. Make it `pub(crate)` in `theme.rs` and use it from `text_on`.)

- [ ] **Step 2: Run `cargo test --lib`.** Expected: FAIL.
- [ ] **Step 3: Implement the helpers, then the rendering rules.**
- [ ] **Step 4: Run `cargo test --lib`.** Expected: all pass.
- [ ] **Step 5: Manual check with `cargo run -- --dir ./tmp-plans`:**
  - blocks are solid with no bands;
  - the shapes are visible in the `white` and `vantablack` themes;
  - the red now-line sits at the current time;
  - the app opens with 08:00 at the top;
  - dragging a block moves it, and Esc during the drag cancels.
- [ ] **Step 6: Commit** `feat: solid blocks with corner type markers, now line, drag ghost`

---

### Task 7: Tasks pane and two-pane layout

**Files:**
- Create: `src/ui/tasks.rs`. Modify: `src/ui/mod.rs`, `src/ui/chrome.rs` (help text), `README.md`
- Test: unit test in `src/ui/tasks.rs` for the pure helper

**Interfaces:**
- Consumes: `AppState` (`tasks`, `task_sel`, `focus`, `mode`, `title_buf`), the Task 3 commands, Task 6 `TaskDrag`.
- Produces:
  - `pub fn task_row_text(task: &Task) -> (String, String)`: returns `(prefix, title)`, where the prefix is `"  " × depth + "[x] "` or `"[ ] "`, and an empty title is shown as `"(untitled)"`.
  - `pub fn show(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme)`
  - `AppState::click_task(&mut self, idx: usize, on_checkbox: bool)` (add this in `state.rs`, with a test): sets `focus = Tasks` and `task_sel = idx`. With `on_checkbox` it toggles `done` as an undoable, saved edit.

Layout and rendering:
- `egui::Panel::left("tasks").resizable(true).default_size(320.0)` is shown before the central timeline. The header and status bar stay full width.
- The pane header is `┌─ tasks ─` and the timeline header is the date line. Each is drawn in `accent` when its pane has focus and in `dim` otherwise. The status bar mode label adds `TASK` for `EditTask` and `MOVE` for `MoveBlock`.
- Each row is one monospace line, allocated with `Sense::click_and_drag()`.
  - **Text:** the prefix is drawn in `dim` and the title in `fg`. Done tasks show the title in `dim` with strikethrough (`TextFormat { strikethrough: Stroke::new(1.0, dim), .. }`).
  - **While editing:** the edited row shows `title_buf` plus a blinking `█`.
  - **Selection:** the selected row gets a 1.5 px outline, `accent` if the pane is focused and `dim` if not. Scroll to the selection when it changes.
- Mouse:
  - **Click:** `click_task(idx, pointer x within the "[ ]" glyph span)`.
  - **Dragging a task:**
    - On drag start, `egui::DragAndDrop::set_payload(ctx, TaskDrag(idx))`.
    - While dragging, paint the task title beside the pointer on `ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("task-drag")))`.
- **Empty list:** show the hint `n: new task` in `dim`.
- **Clicking the timeline** sets `focus = Timeline`. This already happens through `drag_start`; a plain click also goes through `drag_start`.
- Update the `HELP` table in `chrome.rs` and the README Keys section to the new Keymap. Add the `tasks.txt` format to the README.

- [ ] **Step 1: Write the failing tests**

```rust
// ui/tasks.rs
#[test] fn row_text_indents_and_marks() {
    let t = Task { title: "Draft".into(), done: true, depth: 2 };
    assert_eq!(task_row_text(&t), ("    [x] ".into(), "Draft".into()));
    let e = Task { title: String::new(), done: false, depth: 0 };
    assert_eq!(task_row_text(&e), ("[ ] ".into(), "(untitled)".into()));
}
// state.rs
#[test] fn click_task_selects_and_toggles() {
    let (_t, mut s) = st_tasks("- [ ] a\n- [ ] b\n");
    s.focus = Focus::Timeline;
    s.click_task(1, false); assert_eq!((s.focus, s.selected_task()), (Focus::Tasks, Some(1)));
    s.click_task(1, true); assert!(s.tasks.items()[1].done);
    s.apply(Command::Undo, D); assert!(!s.tasks.items()[1].done);
}
```

- [ ] **Step 2: Run `cargo test --lib`.** Expected: FAIL.
- [ ] **Step 3: Implement the pane, the layout, the help text and the README.**
- [ ] **Step 4: Run `cargo test` and `cargo build --release`.** Expected: all tests pass and the release build succeeds.
- [ ] **Step 5: Manual check with `cargo run -- --dir ./tmp-plans`, keyboard only:**
  1. `Tab` to the tasks pane, `n` "Report" Enter, `N` "Outline" Enter. `x` ticks the task, `<` outdents it, `Alt+↑` moves it.
  2. Typing `n x b d` inside a title stays text.
  3. `b` on a task jumps to the timeline; `↓` and then Enter creates a titled block.
  4. With the mouse: drag a task onto the timeline to make a 1-hour block, drag a block to a new time, and click a `[ ]` to toggle it.
  5. `Ctrl+Z` steps back through both task and block edits.
  6. Restarting restores the tasks from `tasks.txt`.
- [ ] **Step 6: Commit** `feat: tasks pane with nested tasks, drag-to-schedule, two-pane layout`
