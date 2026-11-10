# time-block-planner

A one-day-at-a-time time block planner with a terminal look. It is a native GUI app written in Rust with egui, and it runs on Windows, macOS and Linux. Blocks are **standard** (the default), **scheduled** (for calendar meetings) or **deep** (deep work). You can create them by dragging with the mouse or entirely from the keyboard.

## Build

```sh
cargo build --release
# Windows: target\release\time-block-planner.exe (no console window)
# macOS/Linux: target/release/time-block-planner
```

On Linux, install `libxkbcommon-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev` first.

## Data folder

Plans are saved automatically after every change, to `<Documents>/time-block-planner/`. You can override the folder:

- `TBP_DIR=/path/to/folder`
- `time-block-planner --dir /path/to/folder` (takes priority)

Each day is one plaintext file, `YYYY-MM-DD.txt`:

```
09:00-10:30 deep Write quarterly report
11:00-11:30 scheduled Standup
13:00-13:15 standard
```

`HH:MM-HH:MM <kind> <title>`, on a 15-minute grid. `24:00` is allowed as an end time. Lines starting with `#` are comments. You can edit the files by hand. Lines the app can't read (for example, off-grid times or overlaps) are kept at the end of the file under `# unparsed lines kept by time-block-planner`; they are never deleted. If you remove every block from a day, that day's file is deleted.

`config.txt` in the same folder stores the chosen theme (`theme=rose-pine`).

## Keys

`Ctrl` is `Cmd` on macOS, and `Alt` is `Option`.

| Key | Action |
|---|---|
| `↑` `↓` | move the cursor 15 min |
| `PageUp` `PageDown` | move 1 hour |
| `Home` `End`, `Ctrl+↑` `Ctrl+↓` | start / end of day |
| `←` `→` | previous / next day |
| `Ctrl+T` | today |
| `Tab` `Shift+Tab` | next / previous block |
| `Ctrl+N` | new block at the cursor (then `↑`/`↓` to size it, `Enter` to create, `Esc` to cancel) |
| `Shift+↑` `Shift+↓` | select a range for a new block, like selecting text |
| `Enter`, `F2` | rename the block (`Enter` saves, `Esc` reverts) |
| `1` `2` `3` | set the type: standard / scheduled / deep |
| `Alt+↑` `Alt+↓` | move the block |
| `Alt+Shift+↑` `Alt+Shift+↓` | make the block shorter / longer |
| `Delete`, `Backspace` | delete the block |
| `Ctrl+Z` / `Ctrl+Shift+Z`, `Ctrl+Y` | undo / redo |
| `Ctrl+S` | save (the app also saves automatically) |
| `Ctrl+,` | choose a theme (`↑`/`↓` previews, `Enter` keeps it, `Esc` reverts) |
| `F1`, `?` | help |
| `Ctrl+Q` | quit |

Mouse: drag on the timeline to create a block, and click to move the cursor.

## Themes

catppuccin-latte, catppuccin, ethereal, everforest, flexoki-light, gruvbox, hackerman, kanagawa, lumon, matte-black, miasma, nord, osaka-jade, retro-82, ristretto, rose-pine, tokyo-night (default), vantablack, white.

Themes are from Omarchy (MIT), github.com/basecamp/omarchy. The `themes/*.toml` files are unmodified copies of each theme's `colors.toml`.
