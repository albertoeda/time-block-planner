# time-block-planner

A one-day-at-a-time time block planner with a terminal look. It is a native GUI app written in Rust with egui, and it runs on Windows, macOS and Linux. Blocks are **standard** (the default), **scheduled** (for calendar meetings) or **deep** (deep work); every block is drawn in the theme's accent color (dim once it is over, full while it is under way, medium before it starts) and a corner shape (circle, square, triangle) shows its type. You can create them by dragging with the mouse or entirely from the keyboard. A nested task list for the same day sits beside the timeline; drag a task onto the day, or press `p`, to plan it as a block.

## Build

```sh
cargo build --release
# Windows: target\release\time-block-planner.exe (no console window)
# macOS/Linux: target/release/time-block-planner
```

On Linux, install `libxkbcommon-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev` first.

## Data folder

Plans are saved automatically after every change, to `.time-block-planner/` in your home folder (`C:\Users\<you>\.time-block-planner\` on Windows, `~/.time-block-planner/` on macOS and Linux). You can override the folder:

- `TBP_DIR=/path/to/folder`
- `time-block-planner --dir /path/to/folder` (takes priority)

Each day is one plaintext file, `YYYY-MM-DD.txt`, holding that day's time blocks and, after a `[tasks]` line, that day's task list:

```
09:00-10:30 deep Write quarterly report
11:00-11:30 scheduled Standup
13:00-13:15 standard

[tasks]
- [ ] Call the bank
  - [x] Find the number
```

`HH:MM-HH:MM <kind> <title>`, on a 15-minute grid. `24:00` is allowed as an end time. Lines starting with `#` are comments; they are kept, moved to the top of the file when the app saves that day. You can edit the files by hand. Lines the app can't read (for example, off-grid times or overlaps) are kept at the end of the file under `# unparsed lines kept by time-block-planner`; they are never deleted. If a day has no blocks and no tasks, its file is deleted.

`config.txt` in the same folder stores the chosen theme (`theme=rose-pine`).

The tasks after `[tasks]` are a nested Markdown checklist, one list per day:

- Each level of nesting is two spaces of indentation; a tab counts as two spaces.
- `- [ ] ` marks an open task and `- [x] ` (or `- [X] `) a done one. Any other non-blank line becomes an open task, with the line as its title.
- The first task is top-level, and each task can be at most one level deeper than the task above it (deeper lines are pulled up). Indentation stops at 8 levels (16 spaces).
- Blank lines and a leading byte-order mark are dropped. The app writes the tasks back in this normalized form, and leaves out the `[tasks]` line when the day has no tasks.

## Keys

`Ctrl` is `Cmd` on macOS, and `Alt` is `Option`. The window has two panes: the task list on the left and the day's timeline on the right. `←` and `→` move between them; most keys act on the focused pane. The focused pane's title is highlighted, and the footer names it (`tasks` or `time`) and lists its main keys, with `^` for `Ctrl`/`Cmd`. Single-letter keys never fire while you are typing a title.

| Key | Timeline | Tasks |
|---|---|---|
| `←` `→` | go to the tasks pane / timeline | go to the tasks pane / timeline |
| `Shift+←` `Shift+→` | previous / next day (blocks and tasks) | previous / next day (blocks and tasks) |
| `t`, `Ctrl+T` | today | today |
| `↑` `↓` | move the cursor 15 min | select the previous / next task; below the last task is `+ new task` |
| `PageUp` `PageDown` | move 1 hour | move 10 tasks |
| `Home` `End` | 00:00 / 23:45 | first / last task |
| `Ctrl+Shift+↑` `Ctrl+Shift+↓` | previous / next block | |
| `n`, `Ctrl+N` | new block at the cursor (then `↑`/`↓` to size it, `Enter` to create, `Esc` to cancel) | new task after the selected task and its subtasks, at the same level |
| `N` (`Shift+n`) | | new subtask directly under the selected task |
| `Enter`, `F2` | rename the block (`Enter` saves, `Esc` reverts) | rename the task (`Esc` on a just-created task removes it); on `+ new task`, add a task at the end |
| `d`, `Delete`, `Backspace` | delete the block | delete the task and its subtasks |
| `Alt+↑` `Alt+↓` | move the block 15 min | swap the task with its previous / next sibling |
| `Ctrl+↑` `Ctrl+↓`, `Alt+Shift+↑` `Alt+Shift+↓` | make the block shorter / longer | |
| `Shift+↑` `Shift+↓` | select a range for a new block, like selecting text | |
| `1` `2` `3` | set the type: standard / scheduled / deep | |
| `Space`, `x` | | tick / untick the task |
| `Tab` / `Shift+Tab` (also `Alt+→`, `>` / `Alt+←`, `<`) | | indent / outdent the task |
| `p` | | plan the task: switch to the timeline and start a block at the cursor, titled with the task |
| `[` `]`, `Alt+Shift+←` `Alt+Shift+→` | | move the task and its subtasks to the end of the previous / next day's list (you stay on this day; one undo restores both days) |
| `Ctrl+Z` / `Ctrl+Shift+Z`, `Ctrl+Y` | undo / redo (block and task edits share one history) | same |
| `Ctrl+S` | save (the app also saves automatically) | same |
| `Ctrl+,` | choose a theme (`↑`/`↓` previews, `Enter` keeps it, `Esc` reverts) | same |
| `F1`, `?` | help | help |
| `Ctrl+Q` | quit | quit |

Mouse:

- **Timeline:** drag in empty space to create a block, drag a block to move it (`Esc` cancels the move), and click to place the cursor.
- **Tasks pane:** click a task to select it, click its `[ ]` to tick it, and click `+ new task` below the list to add one at the end. Drag a task onto the timeline to create a block of up to one hour there (shorter if the next block starts sooner).

## Themes

catppuccin-latte, catppuccin, ethereal, everforest, flexoki-light, gruvbox, hackerman, kanagawa, lumon, matte-black, miasma, nord, osaka-jade, retro-82, ristretto, rose-pine, tokyo-night (default), vantablack, white.

Themes are from Omarchy (MIT), github.com/basecamp/omarchy. The `themes/*.toml` files are unmodified copies of each theme's `colors.toml`.
