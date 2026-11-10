# Time Block Planner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A cross-platform Rust desktop app (Windows `.exe` first) with a terminal-style GUI that shows one day's timeline at a time. You can create time blocks by click-and-drag or entirely from the keyboard, tag each block as standard, scheduled or deep work, and the app saves every day as a plaintext file in a user folder.

**Architecture:** A pure, UI-free core (`model`, `storage`, `theme`, `state`) holds all behavior and is unit-tested. The keyboard and the mouse both go through one `Command` state machine in `state.rs`. A thin `eframe/egui` layer (`ui/`) only turns key and mouse events into `Command`s or drag calls, and draws `AppState` in a monospace, box-drawing "terminal" style using the active theme's palette. Themes are the Omarchy default theme set: each theme's `colors.toml` is copied into the repo and compiled into the binary. The app saves automatically after every change.

**Tech Stack:** Rust (stable, edition 2021), `eframe`/`egui` (latest stable at implementation time; pin the exact version in `Cargo.toml`), `chrono` (dates), `dirs` (user folders). Tests use the built-in `cargo test`, and storage tests use `tempfile` as a dev-dependency.

**Spec:** The user's request, reproduced verbatim below, plus their follow-up. There is no separate design doc.

> I want to create a Rust app for a time block planner. It should have a GUI but it should be built with a terminal-like aesthetic, with standard terminal themes such as catpucchin, gruvbox and hackerman. It should be multiplatform and run in WIndows with a .exe file. It should display a simple timeline per day where I can assign time blocks for specific tasks. Each timeblock can be marked as scheduled (for calendar meetings) or deep work. The normal status is neither of these (standard). The app should save the time-block plans into a simple plaintext structure in a user folder. It should support click-and drag to create events. It should show one day at a time. It must be possible to fully use and navigate the app using only the keyboard

> Follow-up: I would like more themes. Use a solid set like the ones available by default in Omarchy. I would like keybindings that are familiar to a windows/mac user where relevant, or unique to this app where no analogy is relevant.

## Global Constraints

- Language: Rust. GUI (not a TUI), with a terminal-like look: monospace font everywhere, box-drawing/ASCII borders, no rounded corners or shadows.
- Themes: all 19 Omarchy default themes, taken from `https://raw.githubusercontent.com/basecamp/omarchy/master/themes/<name>/colors.toml`: `catppuccin-latte, catppuccin, ethereal, everforest, flexoki-light, gruvbox, hackerman, kanagawa, lumon, matte-black, miasma, nord, osaka-jade, retro-82, ristretto, rose-pine, tokyo-night, vantablack, white`. Default theme: `tokyo-night`. Themes can be switched at runtime from a picker, and the choice persists. Omarchy is MIT-licensed; credit it in the README.
- Multiplatform: builds on Windows, macOS and Linux. On Windows, `cargo build --release` produces `time-block-planner.exe`, which runs without opening a console window.
- One day is shown at a time.
- Block kinds: exactly `standard` (default), `scheduled` (calendar meetings) and `deep` (deep work).
- Storage: plaintext, one file per day, in a user folder. The default folder is `<Documents>/time-block-planner/`. It can be overridden with the `TBP_DIR` environment variable or the `--dir <path>` command-line argument (the CLI argument wins).
- Every feature must be reachable with the keyboard alone. Shortcuts follow Windows/macOS conventions where an analogy exists. `Ctrl` on Windows/Linux is `Cmd` on macOS (egui `Modifiers::command`), and `Alt` is `Option`. The mouse is optional and is used only for click-and-drag creation and for clicking to move the cursor.
- Time granularity: 15-minute slots, 96 per day, `00:00`–`24:00`.
- Blocks on the same day never overlap.
- No Python; do not add global tools.

## File Format (decided here, used by Task 2)

`<dir>/YYYY-MM-DD.txt`, UTF-8, one block per line, sorted by start time:

```
09:00-10:30 deep Write quarterly report
11:00-11:30 scheduled Standup
13:00-13:15 standard
```

`HH:MM-HH:MM <kind> <title...>`. The title is the rest of the line, trimmed, and may be empty. The end time may be `24:00`. Lines starting with `#` and blank lines are ignored. Any other line that does not parse (bad syntax, minutes that are not multiples of 15, start ≥ end, or overlap with an earlier line) is kept verbatim in `Day::unparsed` and written back at the end of the file under a `# unparsed lines kept by time-block-planner` header. Hand-edited data is never lost. A day with no blocks and no unparsed lines has its file deleted.

The config lives at `<dir>/config.txt` and has one `key=value` per line. The only key so far is `theme=<name>`, where the name is an Omarchy theme name, e.g. `theme=rose-pine`.

## Keymap (decided here, used by Tasks 4 and 5)

`Ctrl` = `Cmd` on macOS, `Alt` = `Option`. The "Why" column gives the convention each key follows.

| Mode | Key | Command | Why |
|---|---|---|---|
| Normal | `↑` / `↓` | `Up` / `Down`: cursor ±15 min | universal |
| Normal | `PageUp` / `PageDown` | `PageUp` / `PageDown`: cursor ±1 h | universal |
| Normal | `Home` / `End`, `Ctrl+↑` / `Ctrl+↓` | `Home` / `End`: 00:00 / 23:45 | Windows / macOS document start/end |
| Normal | `←` / `→` | `PrevDay` / `NextDay` | one day on screen, so horizontal = days (Outlook day view) |
| Normal | `Ctrl+T` | `Today` | Apple Calendar "Today" |
| Normal | `Tab` / `Shift+Tab` | `NextBlock` / `PrevBlock`: cursor to the next/previous block's start | focus traversal |
| Normal | `Ctrl+N` | `NewBlock`: start Create at the cursor | "new event" in Outlook / Apple Calendar |
| Normal | `Shift+↓` / `Shift+↑` | `ExtendDown` / `ExtendUp`: start Create at the cursor and extend | text selection |
| Normal | `Enter`, `F2` | `EditTitle`: rename the block under the cursor | macOS Finder rename / Windows rename |
| Normal | `1` / `2` / `3` | `SetKind(Standard / Scheduled / Deep)` | app-specific |
| Normal | `Alt+↑` / `Alt+↓` | `MoveUp` / `MoveDown`: move the block 15 min | VS Code / editors "move line" |
| Normal | `Alt+Shift+↓` / `Alt+Shift+↑` | `Grow` / `Shrink`: block end ±15 min | app-specific |
| Normal | `Delete`, `Backspace` | `Delete` the block under the cursor | Windows Delete / macOS delete |
| Normal | `Ctrl+Z` | `Undo` | universal |
| Normal | `Ctrl+Shift+Z`, `Ctrl+Y` | `Redo` | macOS / Windows |
| Normal | `Ctrl+S` | `SaveNow` (the app autosaves anyway; this shows "saved") | universal |
| Normal | `Ctrl+,` | `OpenThemes`: theme picker | macOS / VS Code preferences |
| Normal | `F1`, `?` | `ToggleHelp` | Windows help |
| any | `Ctrl+Q` (and the OS close: `Alt+F4` / `Cmd+Q`) | quit | universal |
| Create | `↑` / `↓`, `Shift+↑` / `Shift+↓` | extend the selection (stops at neighbouring blocks) | text selection |
| Create | `Enter` | `Confirm`: create the block → EditTitle | |
| Create | `Esc` | `Cancel` | |
| EditTitle | text keys | `Text(s)` | |
| EditTitle | `Backspace` | `Backspace` (deletes a character, not the block) | |
| EditTitle | `Enter` / `Esc` | `Confirm` / `Cancel` (Cancel on a just-created block removes it) | |
| ThemePicker | `↑` / `↓` | live-preview the previous/next theme | |
| ThemePicker | `Enter` / `Esc` | keep the theme / revert to the original | |
| Help | `Esc`, `F1`, `?` | close | |

## Review Focus

1. **Hand-edited or corrupted day file** (bad line, off-grid `09:10`, overlapping lines). Expected: the app loads everything valid, keeps the rest verbatim, and never silently deletes it on the next save. Covered by Task 2 test `unparsed_lines_survive_roundtrip`.
2. **Typing in the title field triggers commands** (`1`/`2`/`3` changing the kind, `Backspace` deleting the block, `?` opening help). Expected: in EditTitle mode, every printable key goes into the buffer and Backspace removes a character. Covered by Task 4 test `edit_mode_swallows_commands` and Task 5 test `backspace_maps_by_mode`.
3. **Undo after a delete or a day change.** Expected: `Ctrl+Z` restores the deleted block, on the day it belongs to, and the restored day is saved. Covered by Task 4 tests `undo_redo_delete` and `undo_switches_back_to_day`.
4. **Dragging upward or across an existing block.** Expected: an upward drag works like a downward one, and the new block is clipped to the free run that starts at the press point. Pressing on an occupied slot creates nothing. Covered by Task 4 tests `drag_upward_normalizes` and `drag_clips_at_neighbor`.
5. **Monochrome or light themes** (`white`, `vantablack`, `catppuccin-latte`, `flexoki-light`). Expected: the three kinds stay distinguishable even when the palette has no distinct hues, and light themes get light egui visuals. Covered by Task 3 tests `kind_colors_pick_distinct_candidates` and `light_themes_detected`, and by the per-kind fill style in Task 5.

Also covered elsewhere: midnight edges (Task 1 `move_and_grow_refuse_out_of_bounds`, Task 2 `end_of_day_roundtrip`) and an unwritable data folder (Task 2 `save_creates_missing_dir`, Task 4 `save_error_sets_status`).

---

## File Structure

```
Cargo.toml
themes/<name>.toml   # 19 files, verbatim copies of Omarchy themes/<name>/colors.toml
src/main.rs          # entry: parse --dir, resolve dir, build eframe window, windows_subsystem attr
src/lib.rs           # pub mod model, storage, theme, state, ui
src/model.rs         # Slot, Kind, Block, Day (+ overlap-safe ops)
src/storage.rs       # parse/serialize day files, Store (load/save day, load/save config), resolve_dir
src/theme.rs         # colors.toml parser, Theme derivation, themes() registry
src/state.rs         # AppState, Mode, Command, apply(), undo/redo, drag API, autosave
src/ui/mod.rs        # eframe::App impl: event → Command mapping, frame layout
src/ui/timeline.rs   # draws timeline rows/blocks, mouse hit-testing → drag calls
src/ui/chrome.rs     # header, status bar, help overlay, theme picker, egui Visuals from Theme
.github/workflows/build.yml
README.md
```

---

### Task 1: Project scaffold and core model

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/main.rs` (temporary stub `fn main() {}`), `src/model.rs`, `.gitignore` (`/target`, `/tmp-plans`)
- Test: unit tests in `src/model.rs`

**Interfaces:**
- Produces:
  - `pub const SLOTS_PER_DAY: u16 = 96; pub const SLOT_MINUTES: u16 = 15;`
  - `#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct Slot(pub u16);` (valid range is 0..=96, where 96 = `24:00`)
  - `impl Slot { pub fn label(self) -> String /* "09:30", "24:00" */; pub fn parse(s: &str) -> Option<Slot> /* None if malformed, minutes % 15 != 0, or > 24:00 */ }`
  - `#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)] pub enum Kind { #[default] Standard, Scheduled, Deep }` with `fn as_str(self) -> &'static str` (`"standard"|"scheduled"|"deep"`) and `fn parse(&str) -> Option<Kind>`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Block { pub start: Slot, pub end: Slot, pub kind: Kind, pub title: String }` (invariant: start < end)
  - `#[derive(Debug, PartialEq, Eq)] pub enum EditError { Overlap, OutOfBounds, Empty }`
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct Day { blocks: Vec<Block> /* sorted by start, private */, pub unparsed: Vec<String> }`
  - `impl Day`:
    - `fn blocks(&self) -> &[Block]`
    - `fn block_at(&self, slot: Slot) -> Option<usize>`
    - `fn add(&mut self, b: Block) -> Result<usize, EditError>`
    - `fn remove(&mut self, idx: usize) -> Block`
    - `fn set_kind(&mut self, idx: usize, k: Kind)`
    - `fn set_title(&mut self, idx: usize, t: &str)` (stores `t.trim()` with `\n`/`\r` replaced by spaces)
    - `fn shift(&mut self, idx: usize, delta: i32) -> Result<usize, EditError>`
    - `fn resize_end(&mut self, idx: usize, delta: i32) -> Result<(), EditError>`
    - `fn free_range(&self, anchor: Slot, other: Slot) -> Option<(Slot, Slot)>`: returns `None` if `anchor` is occupied or ≥ 96. Otherwise it returns the half-open range covering anchor..=other (either direction), clipped so that it does not cross any block.
    - `fn is_empty(&self) -> bool`

- [ ] **Step 1: `cargo init --lib --name time-block-planner`, add the `src/main.rs` stub, and add dependencies `eframe`, `chrono`, `dirs`, plus dev-dependency `tempfile`. Run `cargo build`.** Expected: it compiles.

- [ ] **Step 2: Write the failing tests in `src/model.rs`**

```rust
#[test] fn slot_label_and_parse() {
    assert_eq!(Slot(38).label(), "09:30");
    assert_eq!(Slot(96).label(), "24:00");
    assert_eq!(Slot::parse("09:30"), Some(Slot(38)));
    assert_eq!(Slot::parse("24:00"), Some(Slot(96)));
    assert_eq!(Slot::parse("09:10"), None);
    assert_eq!(Slot::parse("24:15"), None);
    assert_eq!(Slot::parse("9:3"), None);
}
#[test] fn add_rejects_overlap_and_keeps_sorted() {
    let mut d = Day::default();
    d.add(blk(40, 44)).unwrap();
    assert_eq!(d.add(blk(43, 46)), Err(EditError::Overlap));
    assert_eq!(d.add(blk(36, 40)), Ok(0));   // touching is fine
    assert_eq!(d.block_at(Slot(43)), Some(1));
    assert_eq!(d.block_at(Slot(44)), None);
}
#[test] fn move_and_grow_refuse_out_of_bounds() {
    let mut d = Day::default();
    d.add(blk(95, 96)).unwrap();
    assert_eq!(d.shift(0, 1), Err(EditError::OutOfBounds));
    assert_eq!(d.resize_end(0, 1), Err(EditError::OutOfBounds));
    assert_eq!(d.resize_end(0, -1), Err(EditError::Empty));
    d.add(blk(0, 1)).unwrap();
    assert_eq!(d.shift(0, -1), Err(EditError::OutOfBounds));
}
#[test] fn shift_refuses_overlap() {
    let mut d = Day::default();
    d.add(blk(10, 12)).unwrap(); d.add(blk(12, 14)).unwrap();
    assert_eq!(d.shift(0, 1), Err(EditError::Overlap));
    assert_eq!(d.blocks()[0].start, Slot(10));
}
#[test] fn free_range_clips_and_normalizes() {
    let mut d = Day::default();
    d.add(blk(20, 24)).unwrap();
    assert_eq!(d.free_range(Slot(10), Slot(30)), Some((Slot(10), Slot(20))));
    assert_eq!(d.free_range(Slot(30), Slot(10)), Some((Slot(24), Slot(31))));
    assert_eq!(d.free_range(Slot(21), Slot(30)), None);
    assert_eq!(d.free_range(Slot(5), Slot(5)), Some((Slot(5), Slot(6))));
}
#[test] fn set_title_sanitizes() {
    let mut d = Day::default(); d.add(blk(0, 1)).unwrap();
    d.set_title(0, "  a\nb  ");
    assert_eq!(d.blocks()[0].title, "a b");
}
// helper: fn blk(s: u16, e: u16) -> Block { Block { start: Slot(s), end: Slot(e), kind: Kind::Standard, title: String::new() } }
```

- [ ] **Step 3: Run `cargo test model`.** Expected: FAIL (types are not defined).
- [ ] **Step 4: Implement `src/model.rs`. `shift` and `resize_end` build the candidate block, check bounds and overlap against every other block, and only then mutate.**
- [ ] **Step 5: Run `cargo test model`.** Expected: all 6 tests PASS.
- [ ] **Step 6: Commit** `feat: scaffold crate and core time-block model`

---

### Task 2: Plaintext storage

**Files:**
- Create: `src/storage.rs`
- Test: unit tests in `src/storage.rs`

**Interfaces:**
- Consumes: `Day`, `Block`, `Slot`, `Kind`, `Day::add` (Task 1).
- Produces:
  - `pub fn parse_day(text: &str) -> Day` (follows the File Format section exactly)
  - `pub fn serialize_day(day: &Day) -> String` (lines end with `\n`; a block with an empty title has no trailing space)
  - `pub fn resolve_dir(cli: Option<PathBuf>) -> PathBuf` (CLI arg > `TBP_DIR` env > `dirs::document_dir()` > `dirs::home_dir()`, then joined with `time-block-planner`. The CLI and env paths are used as-is.)
  - `pub struct Store { pub dir: PathBuf }` with:
    - `fn load_day(&self, date: NaiveDate) -> io::Result<Day>` (a missing file → `Ok(Day::default())`)
    - `fn save_day(&self, date: NaiveDate, day: &Day) -> io::Result<()>` (`create_dir_all`, writes `YYYY-MM-DD.txt.tmp`, then `rename`. An empty day removes the file if it exists.)
    - `fn load_theme(&self) -> Option<String>`, `fn save_theme(&self, name: &str) -> io::Result<()>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn parse_and_serialize_roundtrip() {
    let txt = "09:00-10:30 deep Write quarterly report\n11:00-11:30 scheduled Standup\n13:00-13:15 standard\n";
    let d = parse_day(txt);
    assert_eq!(d.blocks().len(), 3);
    assert_eq!(d.blocks()[0].kind, Kind::Deep);
    assert_eq!(d.blocks()[0].title, "Write quarterly report");
    assert_eq!(d.blocks()[2].title, "");
    assert_eq!(serialize_day(&d), txt);
}
#[test] fn comments_and_blank_lines_ignored() {
    let d = parse_day("# note\n\n09:00-09:15 standard x\n");
    assert_eq!(d.blocks().len(), 1);
    assert!(d.unparsed.is_empty());
}
#[test] fn unparsed_lines_survive_roundtrip() {
    let d = parse_day("09:00-10:00 deep A\n09:10-09:40 standard off-grid\n09:30-09:45 standard overlap\ngarbage\n");
    assert_eq!(d.blocks().len(), 1);
    assert_eq!(d.unparsed, vec!["09:10-09:40 standard off-grid", "09:30-09:45 standard overlap", "garbage"]);
    assert_eq!(parse_day(&serialize_day(&d)), d);
}
#[test] fn end_of_day_roundtrip() {
    let d = parse_day("23:45-24:00 standard late\n");
    assert_eq!(d.blocks()[0].end, Slot(96));
    assert_eq!(serialize_day(&d), "23:45-24:00 standard late\n");
}
#[test] fn save_creates_missing_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store { dir: tmp.path().join("nested/plans") };
    let date = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
    let d = parse_day("09:00-09:15 deep x\n");
    store.save_day(date, &d).unwrap();
    assert!(store.dir.join("2026-11-10.txt").exists());
    assert_eq!(store.load_day(date).unwrap(), d);
}
#[test] fn saving_empty_day_deletes_file() { /* save a non-empty day, then save Day::default(); assert the file no longer exists */ }
#[test] fn missing_file_loads_empty() { /* load_day on an empty tempdir == Day::default() */ }
#[test] fn theme_config_roundtrip() { /* save_theme("rose-pine"); load_theme() == Some("rose-pine"); empty dir -> None */ }
#[test] fn resolve_dir_prefers_cli() { assert_eq!(resolve_dir(Some("X:/p".into())), PathBuf::from("X:/p")); }
```

- [ ] **Step 2: Run `cargo test storage`.** Expected: FAIL.
- [ ] **Step 3: Implement `src/storage.rs`. Parse each line with `splitn(3, ' ')` into range / kind / title. If `Day::add` returns an error, the line goes to `unparsed`. `parse_day` ignores the exact `# unparsed…` header line, so the header does not pile up.**
- [ ] **Step 4: Run `cargo test storage`.** Expected: all 9 tests PASS.
- [ ] **Step 5: Commit** `feat: plaintext per-day storage and config`

---

### Task 3: Omarchy themes

**Files:**
- Create: `themes/<name>.toml` ×19, `src/theme.rs`
- Test: unit tests in `src/theme.rs`

**Interfaces:**
- Consumes: `Kind` (Task 1).
- Produces:
  - `#[derive(Copy, Clone, Debug, PartialEq)] pub struct Rgb(pub u8, pub u8, pub u8);` (no egui dependency)
  - `#[derive(Clone, Debug)] pub struct Theme { pub name: &'static str, pub bg: Rgb, pub fg: Rgb, pub dim: Rgb, pub accent: Rgb, pub standard: Rgb, pub scheduled: Rgb, pub deep: Rgb, pub selection: Rgb, pub error: Rgb, pub is_light: bool }`
  - `pub fn parse_colors(src: &str) -> Result<HashMap<String, Rgb>, String>` (flat `key = "#rrggbb"` lines; blank and `#`-comment lines skipped)
  - `pub fn derive_theme(name: &'static str, colors: &HashMap<String, Rgb>) -> Result<Theme, String>`
  - `pub fn themes() -> &'static [Theme]` (`OnceLock`; built from `const SOURCES: &[(&str, &str)] = &[("catppuccin-latte", include_str!("../themes/catppuccin-latte.toml")), …]` in the order listed in Global Constraints; panics on a parse error, which the tests rule out)
  - `pub fn theme_index(name: &str) -> usize` (case-insensitive; unknown → the index of `tokyo-night`)
  - `impl Theme { pub fn kind_color(&self, k: Kind) -> Rgb }`

Derivation rules (`derive_theme`). The Omarchy keys are `accent, cursor, foreground, background, selection_foreground, selection_background, color0..color15`.
- `bg = background`, `fg = foreground`, `accent = accent`, `selection = selection_background`, `error = color1`.
- `dim` = an even 50/50 per-channel mix of `fg` and `bg` (some themes set `color8` equal to `color0`, so neither can be used for this).
- `is_light` = relative luminance of `bg` > 0.5.
- Kind colors are picked in order, each taking the first candidate that differs from `bg` and from the kinds already picked. If none qualifies, it falls back to `fg`.
  - `standard` from `[color4, color12, color6, color14]`
  - `scheduled` from `[color3, color11, color5, color13]`
  - `deep` from `[color2, color10, color6, color14, color5]`
- A missing required key → `Err("<name>: missing <key>")`.

- [ ] **Step 1: Copy the 19 files.** Download each `https://raw.githubusercontent.com/basecamp/omarchy/master/themes/<name>/colors.toml` to `themes/<name>.toml` without changing it. Expected: 19 files exist, and each contains `background = "#…"`.

- [ ] **Step 2: Write the failing tests**

```rust
#[test] fn all_omarchy_themes_load() {
    let names: Vec<_> = themes().iter().map(|t| t.name).collect();
    assert_eq!(names, ["catppuccin-latte","catppuccin","ethereal","everforest","flexoki-light","gruvbox","hackerman","kanagawa","lumon","matte-black","miasma","nord","osaka-jade","retro-82","ristretto","rose-pine","tokyo-night","vantablack","white"]);
}
#[test] fn gruvbox_maps_exact_values() {
    let t = &themes()[theme_index("gruvbox")];
    assert_eq!(t.bg, Rgb(0x28,0x28,0x28)); assert_eq!(t.fg, Rgb(0xd4,0xbe,0x98));
    assert_eq!(t.standard, Rgb(0x7d,0xae,0xa3)); assert_eq!(t.scheduled, Rgb(0xd8,0xa6,0x57));
    assert_eq!(t.deep, Rgb(0xa9,0xb6,0x65)); assert_eq!(t.selection, Rgb(0xd6,0x5d,0x0e));
}
#[test] fn kind_colors_pick_distinct_candidates() {
    // synthetic palette where color4 == color3 == color2: falls through to the next candidates
    let mut c = base_palette(); // helper: every required key, with all colorN = #808080, bg #000000, fg #ffffff
    c.insert("color12".into(), Rgb(1,1,1)); c.insert("color11".into(), Rgb(2,2,2)); c.insert("color10".into(), Rgb(3,3,3));
    let t = derive_theme("x", &c).unwrap();
    assert_eq!(t.standard, Rgb(0x80,0x80,0x80)); assert_eq!(t.scheduled, Rgb(2,2,2)); assert_eq!(t.deep, Rgb(3,3,3));
}
#[test] fn missing_key_is_error() { let mut c = base_palette(); c.remove("background"); assert!(derive_theme("x", &c).is_err()); }
#[test] fn light_themes_detected() {
    for n in ["catppuccin-latte", "flexoki-light", "white"] { assert!(themes()[theme_index(n)].is_light, "{n}"); }
    for n in ["tokyo-night", "gruvbox", "vantablack"] { assert!(!themes()[theme_index(n)].is_light, "{n}"); }
}
#[test] fn lookup_case_insensitive_with_fallback() {
    assert_eq!(themes()[theme_index("Rose-Pine")].name, "rose-pine");
    assert_eq!(themes()[theme_index("nope")].name, "tokyo-night");
}
```

- [ ] **Step 3: Run `cargo test theme`.** Expected: FAIL.
- [ ] **Step 4: Implement `src/theme.rs` following the derivation rules.** If `light_themes_detected` fails for a theme because the upstream file is not what its name suggests, report it rather than changing the rule.
- [ ] **Step 5: Run `cargo test theme`.** Expected: PASS.
- [ ] **Step 6: Commit** `feat: Omarchy theme set compiled from colors.toml`

---

### Task 4: App state machine (commands, undo, drag, autosave)

**Files:**
- Create: `src/state.rs`
- Test: unit tests in `src/state.rs`

**Interfaces:**
- Consumes: Task 1 model, Task 2 `Store`, Task 3 `themes()`/`theme_index`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub enum Mode { Normal, Create { anchor: Slot }, EditTitle { idx: usize, original: String, is_new: bool }, ThemePicker { original: usize }, Help }`
  - `#[derive(Clone, Debug, PartialEq)] pub enum Command { Up, Down, PageUp, PageDown, Home, End, PrevDay, NextDay, Today, NextBlock, PrevBlock, NewBlock, ExtendUp, ExtendDown, EditTitle, SetKind(Kind), MoveUp, MoveDown, Grow, Shrink, Delete, Undo, Redo, SaveNow, OpenThemes, ToggleHelp, Confirm, Cancel, Text(String), Backspace }` (quitting is handled in the UI and is not a Command)
  - `pub struct AppState { pub store: Store, pub date: NaiveDate, pub day: Day, pub cursor: Slot /* 0..=95 */, pub mode: Mode, pub theme: usize, pub title_buf: String, pub status: String, history: Vec<(NaiveDate, Day)>, future: Vec<(NaiveDate, Day)> }`
  - `impl AppState`:
    - `fn new(store: Store, today: NaiveDate) -> AppState` (loads the day and the theme; on a load error it starts empty and sets `status`)
    - `fn apply(&mut self, cmd: Command, today: NaiveDate)`
    - `fn selected(&self) -> Option<usize>` (= `day.block_at(cursor)`)
    - `fn pending_range(&self) -> Option<(Slot, Slot)>` (in Create mode, `day.free_range(anchor, cursor)`)
    - `fn drag_start(&mut self, slot: Slot)`, `fn drag_update(&mut self, slot: Slot)`, `fn drag_end(&mut self)`
    - `fn save(&mut self)` (on `Err(e)`, sets `status = format!("save failed: {e}")`)

Behavior rules:
- `apply` clears `status` first, then acts according to the current mode. Commands not listed for that mode are ignored.
- **Normal mode.** The commands do what the Keymap says.
  - `ExtendDown`/`ExtendUp` on a free cursor slot enter `Create { anchor: cursor }` and then move the cursor. On an occupied slot they set the status `"slot occupied"`. `NewBlock` behaves the same way but does not move the cursor.
  - `Text`, `Backspace`, `Confirm` and `Cancel` are ignored.
  - Refused edits set the status: `"overlaps another block"`, `"out of day bounds"`, `"block can't be empty"`, `"no block here"`.
  - Changing the day saves the old day first, then loads the new one and keeps the cursor slot.
  - `SaveNow` → `save()`, then the status is `"saved"` unless the save failed.
- **Create mode.** `Up`/`Down`/`ExtendUp`/`ExtendDown` move the cursor (clamped to 0..=95). `Confirm` checkpoints, adds `Block { kind: Standard, title: "" }` over `pending_range`, saves, and enters `EditTitle { is_new: true }` with an empty buffer. `Cancel` returns to Normal.
- **EditTitle mode.** Only `Text`, `Backspace`, `Confirm` and `Cancel` act.
  - `Confirm`: if `!is_new`, checkpoint first. Then `set_title(buf)`, save, and return to Normal.
  - `Cancel` when `is_new`: pop the checkpoint from `history` and restore it, which removes the block, then save. When `!is_new`: just return to Normal (the title in `day` was never changed).
- **ThemePicker mode.** `Up`/`Down` change `theme` (wrapping) as a live preview. `Confirm` calls `store.save_theme(name)` and returns to Normal. `Cancel` restores `original`.
- **Help mode.** `ToggleHelp`/`Cancel` close it.
- **Undo.**
  - "Checkpoint" = push `(date, day.clone())` onto `history` (capped at 100, dropping the oldest) and clear `future`. Every mutating Normal command (`SetKind`, `Move*`, `Grow`, `Shrink`, `Delete`) checkpoints just before it succeeds; refused edits do not.
  - `Undo` pops `history`, pushes the current `(date, day)` onto `future`, and restores the snapshot. If the snapshot's date differs, it saves the current day first and switches `date`. Then it saves. `Redo` mirrors this. Empty stacks → status `"nothing to undo"` / `"nothing to redo"`.
- **Drag.** `drag_start` works only in Normal mode: it sets the cursor, and if the slot is free it enters `Create { anchor }`. `drag_update` works only in Create mode and sets the cursor. `drag_end` = `apply(Confirm)` in Create mode.

- [ ] **Step 1: Write the failing tests** (helpers: `st()` = `AppState::new(Store { dir: tempdir }, D)` with `D = 2026-11-10`; `st_with_block(s, e)` adds a standard block titled "a" and sets the cursor to `s`)

```rust
#[test] fn keyboard_create_flow() {
    let mut s = st(); s.cursor = Slot(36);
    s.apply(Command::ExtendDown, D); s.apply(Command::ExtendDown, D); s.apply(Command::Down, D);
    assert_eq!(s.pending_range(), Some((Slot(36), Slot(40))));
    s.apply(Command::Confirm, D);
    assert!(matches!(s.mode, Mode::EditTitle { is_new: true, .. }));
    s.apply(Command::Text("Deep sync".into()), D);
    s.apply(Command::Confirm, D);
    assert_eq!(s.mode, Mode::Normal);
    assert_eq!(s.day.blocks()[0].title, "Deep sync");
    assert_eq!(s.store.load_day(D).unwrap(), s.day);
}
#[test] fn edit_mode_swallows_commands() {
    let mut s = st_with_block(36, 40);
    s.apply(Command::EditTitle, D);
    for c in [Command::Delete, Command::SetKind(Kind::Deep), Command::NextDay, Command::ToggleHelp, Command::Undo] { s.apply(c, D); }
    assert_eq!(s.day.blocks().len(), 1); assert_eq!(s.day.blocks()[0].kind, Kind::Standard);
    assert_eq!(s.date, D); assert!(matches!(s.mode, Mode::EditTitle { .. }));
}
#[test] fn cancel_new_block_removes_it() { /* NewBlock, Confirm, Text("x"), Cancel -> blocks empty, file absent, Undo -> "nothing to undo" */ }
#[test] fn cancel_edit_restores_title() { /* block "a": EditTitle, Backspace, Text("b"), Cancel -> title "a" */ }
#[test] fn set_kind_persists() { /* SetKind(Scheduled) -> kind Scheduled and in store.load_day */ }
#[test] fn move_refusal_sets_status() { /* blocks 36-40 and 40-44, cursor 36, MoveDown -> status "overlaps another block", unchanged, Undo -> "nothing to undo" */ }
#[test] fn undo_redo_delete() {
    let mut s = st_with_block(36, 40);
    s.apply(Command::Delete, D); assert!(s.day.blocks().is_empty());
    s.apply(Command::Undo, D);  assert_eq!(s.day.blocks().len(), 1);
    assert_eq!(s.store.load_day(D).unwrap().blocks().len(), 1);
    s.apply(Command::Redo, D);  assert!(s.day.blocks().is_empty());
}
#[test] fn undo_switches_back_to_day() {
    let mut s = st_with_block(36, 40);
    s.apply(Command::Delete, D); s.apply(Command::NextDay, D);
    s.apply(Command::Undo, D);
    assert_eq!(s.date, D); assert_eq!(s.day.blocks().len(), 1);
}
#[test] fn day_navigation_saves_and_loads() {
    let mut s = st_with_block(36, 40);
    s.apply(Command::NextDay, D);
    assert_eq!(s.date, D.succ_opt().unwrap()); assert!(s.day.blocks().is_empty());
    s.apply(Command::Today, D);
    assert_eq!(s.day.blocks().len(), 1);
}
#[test] fn cursor_clamps() { /* Home then Up -> Slot(0); End then Down -> Slot(95); PageDown from 0 -> Slot(4) */ }
#[test] fn next_prev_block_jump() { /* blocks at 10 and 40, cursor 0: NextBlock -> 10, NextBlock -> 40, PrevBlock -> 10 */ }
#[test] fn theme_picker_preview_confirm_cancel() {
    let mut s = st(); let start = s.theme;
    s.apply(Command::OpenThemes, D); s.apply(Command::Down, D);
    assert_ne!(s.theme, start); s.apply(Command::Cancel, D); assert_eq!(s.theme, start);
    s.apply(Command::OpenThemes, D); s.apply(Command::Up, D); s.apply(Command::Confirm, D);
    assert_eq!(s.store.load_theme().as_deref(), Some(themes()[s.theme].name));
}
#[test] fn drag_upward_normalizes() {
    let mut s = st();
    s.drag_start(Slot(44)); s.drag_update(Slot(40)); s.drag_end();
    assert_eq!((s.day.blocks()[0].start, s.day.blocks()[0].end), (Slot(40), Slot(45)));
}
#[test] fn drag_clips_at_neighbor() {
    let mut s = st_with_block(40, 44);
    s.drag_start(Slot(36)); s.drag_update(Slot(50)); s.drag_end();
    assert_eq!(s.day.block_at(Slot(36)).map(|i| s.day.blocks()[i].end), Some(Slot(40)));
    s.apply(Command::Cancel, D);              // leave the new block's title edit -> removed
    s.drag_start(Slot(41)); s.drag_end();     // press on an occupied slot
    assert_eq!(s.day.blocks().len(), 1); assert_eq!(s.mode, Mode::Normal);
}
#[test] fn save_error_sets_status() {
    let tmp = tempfile::tempdir().unwrap(); let f = tmp.path().join("file"); std::fs::write(&f, "").unwrap();
    let mut s = AppState::new(Store { dir: f }, D);
    s.apply(Command::NewBlock, D); s.apply(Command::Confirm, D); s.apply(Command::Confirm, D);
    assert!(s.status.starts_with("save failed")); assert_eq!(s.day.blocks().len(), 1);
}
```

- [ ] **Step 2: Run `cargo test state`.** Expected: FAIL.
- [ ] **Step 3: Implement `src/state.rs` following the behavior rules.**
- [ ] **Step 4: Run `cargo test`.** Expected: all tests PASS (Tasks 1–4).
- [ ] **Step 5: Commit** `feat: state machine with undo, drag and autosave`

---

### Task 5: Terminal-style GUI (timeline, input mapping, chrome)

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/timeline.rs`, `src/ui/chrome.rs`
- Modify: `src/main.rs`, `src/lib.rs`
- Test: unit tests for the pure helpers in `src/ui/timeline.rs` and `src/ui/mod.rs`

**Interfaces:**
- Consumes: `AppState`, `Command`, `Mode`, the drag API (Task 4), `themes()`/`Theme`/`Rgb` (Task 3), `resolve_dir`/`Store` (Task 2).
- Produces:
  - `pub fn y_to_slot(y: f32, top: f32, row_h: f32) -> Slot` (in `timeline.rs`; clamped to 0..=95)
  - `pub fn key_to_command(key: egui::Key, mods: egui::Modifiers, mode: &Mode) -> Option<Command>` (in `ui/mod.rs`). Implements the Keymap's non-text keys (arrows, Page, Home/End, Tab, Enter, F1, F2, Esc, Delete, Backspace, and the `Ctrl`/`Alt` chords; `Ctrl` means `mods.command`). It is mode-aware:
    - `Backspace` → `Delete` in Normal mode, and → `Backspace` in EditTitle.
    - In EditTitle it maps only Enter, Esc and Backspace.
    - In Create, ThemePicker and Help it maps only the keys that the Keymap lists for that mode.
  - `pub fn text_to_command(text: &str, mode: &Mode) -> Option<Command>`: in Normal mode `"1"/"2"/"3"` → `SetKind` and `"?"` → `ToggleHelp`; in Help mode `"?"` → `ToggleHelp`; in EditTitle mode → `Text(text)`; otherwise `None`.
  - `pub struct PlannerApp { state: AppState }` implementing `eframe::App`
  - `pub fn visuals(theme: &Theme) -> egui::Visuals` (in `chrome.rs`: start from `Visuals::light()` or `Visuals::dark()` according to `is_light`; panel/window fill = bg, text = fg, all rounding 0, no shadows, strokes in `dim`)

Rendering decisions:
- The whole UI uses egui's built-in monospace font for every `TextStyle`.
- Header: `┌─ Tue 2026-11-10 ─ [today] ──┐` in `accent`. Status bar: mode (`NORMAL`/`CREATE`/`EDIT`/`THEME`/`HELP`), the status message (in `error` when it starts with "save failed" or is a refusal, otherwise in `fg`), the theme name, and `F1 help`.
- Timeline: a `ScrollArea` of 96 rows, 1 monospace line each. Each row shows `HH:MM │` in `dim` (full-hour rows in `fg`). The cursor row gets an `accent` outline and is kept visible with `scroll_to_rect`. The pending Create range is filled with `selection` at 50% alpha.
- Blocks: each kind has its own fill style so kinds stay distinguishable even in monochrome themes:
  - standard: outline only;
  - scheduled: `░` shade fill;
  - deep: solid fill at 35% alpha.
  All three are drawn in `kind_color`, with a full-color `▌` left bar. The first row shows the tag `[ ]`/`[S]`/`[D]` followed by the title, or `(untitled)`.
- In EditTitle mode the title is drawn from `title_buf` with a blinking `█` caret. Do not use egui `TextEdit`, so keyboard focus always stays with our handler.
- The theme picker is a centered bordered `egui::Area` listing all theme names, with the current one highlighted. The help overlay is a similar `Area` that renders the Keymap table, with `Ctrl` shown as `⌘` on macOS (`cfg!(target_os = "macos")`).
- Mouse: allocate the timeline with `Sense::click_and_drag()`. `drag_started` → `drag_start(y_to_slot(..))`, `dragged` → `drag_update`, `drag_stopped` → `drag_end`. A plain click → `drag_start`, then `apply(Cancel)` (moves the cursor only).
- Quit: `Ctrl+Q` → `ctx.send_viewport_cmd(ViewportCommand::Close)`. Closing the window still runs the last save (saving happens on every change, so nothing is pending).
- `main.rs`: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`. Parse `--dir <path>` from `std::env::args()`. Window title `time-block-planner`, initial size 520×900. `eframe::run_native`.
- Event loop: drain `ctx.input(|i| i.events.clone())`. `Event::Key { pressed: true, .. }` → `key_to_command`, and `Event::Text` → `text_to_command`. Consume the keys you handle (`i.consume_key`) so that egui's own Tab focus navigation never runs.

- [ ] **Step 1: Write the failing tests for the pure helpers**

```rust
use egui::{Key, Modifiers as M};
#[test] fn y_to_slot_maps_and_clamps() {
    assert_eq!(y_to_slot(100.0, 100.0, 20.0), Slot(0));
    assert_eq!(y_to_slot(159.0, 100.0, 20.0), Slot(2));
    assert_eq!(y_to_slot(50.0, 100.0, 20.0), Slot(0));
    assert_eq!(y_to_slot(1e6, 100.0, 20.0), Slot(95));
}
#[test] fn platform_shortcuts() {
    let n = &Mode::Normal;
    assert_eq!(key_to_command(Key::N, M::COMMAND, n), Some(Command::NewBlock));
    assert_eq!(key_to_command(Key::Z, M::COMMAND, n), Some(Command::Undo));
    assert_eq!(key_to_command(Key::Z, M::COMMAND | M::SHIFT, n), Some(Command::Redo));
    assert_eq!(key_to_command(Key::Y, M::COMMAND, n), Some(Command::Redo));
    assert_eq!(key_to_command(Key::T, M::COMMAND, n), Some(Command::Today));
    assert_eq!(key_to_command(Key::Comma, M::COMMAND, n), Some(Command::OpenThemes));
    assert_eq!(key_to_command(Key::F2, M::NONE, n), Some(Command::EditTitle));
    assert_eq!(key_to_command(Key::ArrowDown, M::ALT, n), Some(Command::MoveDown));
    assert_eq!(key_to_command(Key::ArrowDown, M::ALT | M::SHIFT, n), Some(Command::Grow));
    assert_eq!(key_to_command(Key::ArrowDown, M::SHIFT, n), Some(Command::ExtendDown));
    assert_eq!(key_to_command(Key::ArrowUp, M::COMMAND, n), Some(Command::Home));
    assert_eq!(key_to_command(Key::ArrowRight, M::NONE, n), Some(Command::NextDay));
    assert_eq!(key_to_command(Key::Tab, M::SHIFT, n), Some(Command::PrevBlock));
}
#[test] fn backspace_maps_by_mode() {
    let e = Mode::EditTitle { idx: 0, original: String::new(), is_new: false };
    assert_eq!(key_to_command(Key::Backspace, M::NONE, &Mode::Normal), Some(Command::Delete));
    assert_eq!(key_to_command(Key::Backspace, M::NONE, &e), Some(Command::Backspace));
    assert_eq!(key_to_command(Key::ArrowLeft, M::NONE, &e), None);
    assert_eq!(text_to_command("1", &e), Some(Command::Text("1".into())));
    assert_eq!(text_to_command("1", &Mode::Normal), Some(Command::SetKind(Kind::Standard)));
    assert_eq!(text_to_command("?", &e), Some(Command::Text("?".into())));
}
```

- [ ] **Step 2: Run `cargo test ui`.** Expected: FAIL.
- [ ] **Step 3: Implement `src/ui/*` and `main.rs` as described.**
- [ ] **Step 4: Run `cargo test`.** Expected: all tests PASS.
- [ ] **Step 5: Manual verification: `cargo run -- --dir ./tmp-plans`.** Check each of these:
  1. Drag from 09:00 to 10:00, type a title, press Enter. `./tmp-plans/<today>.txt` contains `09:00-10:00 standard <title>`.
  2. Keyboard only:
     - `Shift+↓`×3, Enter, type, Enter.
     - `2`/`3` change the tag and fill style.
     - `Alt+↑`/`Alt+↓` move the block, `Alt+Shift+↑`/`Alt+Shift+↓` resize it.
     - `Delete`, then `Ctrl+Z` brings the block back.
     - `←`/`→`/`Ctrl+T` change the day, `Tab` jumps between blocks.
     - `Ctrl+,` previews themes live (Esc reverts, Enter keeps).
     - `F1` opens and closes help, and `Ctrl+Q` quits.
  3. Typing `1? deep` while editing a title neither changes the kind nor opens help. Backspace removes characters, not the block.
  4. Cycle through all 19 themes. Check that light themes look light and that the three kinds can be told apart in `white` and `vantablack`.
  5. Restarting restores the theme and the blocks.
- [ ] **Step 6: Commit** `feat: terminal-style egui timeline with keyboard and drag`

---

### Task 6: Release build, CI and README

**Files:**
- Create: `.github/workflows/build.yml`, `README.md`
- Modify: `Cargo.toml` (`[profile.release] lto = true, strip = true`)

**Interfaces:**
- Consumes: the whole crate.
- Produces: `target/release/time-block-planner.exe`, and a CI artifact per OS.

- [ ] **Step 1: Run `cargo build --release` on Windows.** Expected: `target\release\time-block-planner.exe` exists, and double-clicking it opens the window with no console window.
- [ ] **Step 2: Write `.github/workflows/build.yml`.** Use a matrix of `windows-latest`, `macos-latest` and `ubuntu-latest`. Each job runs `cargo test`, then `cargo build --release`. On Ubuntu, first install `libxkbcommon-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`. Upload the binary with `actions/upload-artifact`.
- [ ] **Step 3: Write `README.md`.** Include: the build commands; the data folder and its overrides (`TBP_DIR`, `--dir`); the File Format block and the Keymap table from this plan; the theme list with the credit "Themes from Omarchy (MIT), github.com/basecamp/omarchy".
- [ ] **Step 4: Run `cargo test && cargo build --release`.** Expected: PASS and a successful build.
- [ ] **Step 5: Commit** `chore: release profile, CI matrix, README`
