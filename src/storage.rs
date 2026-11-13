//! Plaintext persistence: one `YYYY-MM-DD.txt` per day (time blocks, then an optional `[tasks]`
//! checklist) plus `config.txt`, in a user folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::model::{Block, Day, Kind, Slot};
use crate::tasks::{Task, TaskList};

const UNPARSED_HEADER: &str = "# unparsed lines kept by time-block-planner";
const APP_DIR: &str = ".time-block-planner";
const TASKS_HEADER: &str = "[tasks]";

/// Parses a day file. Lines that are not valid, non-overlapping blocks are kept in `unparsed`.
pub fn parse_day(text: &str) -> Day {
    let mut day = Day::default();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line == UNPARSED_HEADER {
            continue;
        }
        if line.starts_with('#') {
            day.comments.push(line.to_string());
            continue;
        }
        let added = parse_block(line).map(|b| day.add(b));
        if !matches!(added, Some(Ok(_))) {
            day.unparsed.push(line.to_string());
        }
    }
    day
}

fn parse_block(line: &str) -> Option<Block> {
    let mut parts = line.splitn(3, ' ');
    let (start, end) = parts.next()?.split_once('-')?;
    let kind = Kind::parse(parts.next()?)?;
    Some(Block {
        start: Slot::parse(start)?,
        end: Slot::parse(end)?,
        kind,
        title: parts.next().unwrap_or("").trim().to_string(),
    })
}

pub fn serialize_day(day: &Day) -> String {
    let mut out = String::new();
    for c in &day.comments {
        out.push_str(c);
        out.push('\n');
    }
    for b in day.blocks() {
        out.push_str(&format!(
            "{}-{} {}",
            b.start.label(),
            b.end.label(),
            b.kind.as_str()
        ));
        if !b.title.is_empty() {
            out.push(' ');
            out.push_str(&b.title);
        }
        out.push('\n');
    }
    if !day.unparsed.is_empty() {
        out.push_str(UNPARSED_HEADER);
        out.push('\n');
        for line in &day.unparsed {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Parses a task list leniently: indent gives depth, `- [ ]`/`- [x]` markers give state.
pub fn parse_tasks(text: &str) -> TaskList {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut items = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent: usize = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(|c| if c == '\t' { 2 } else { 1 })
            .sum();
        let rest = line.trim_start_matches([' ', '\t']);
        // Editors may trim the trailing space of an empty task: `- [ ]` alone.
        let (done, rest) = if rest == "- [ ]" {
            (false, "")
        } else if rest == "- [x]" || rest == "- [X]" {
            (true, "")
        } else if let Some(r) = rest.strip_prefix("- [ ] ") {
            (false, r)
        } else if let Some(r) = rest
            .strip_prefix("- [x] ")
            .or_else(|| rest.strip_prefix("- [X] "))
        {
            (true, r)
        } else {
            (false, rest)
        };
        let depth = (indent / 2).min(u8::MAX as usize) as u8;
        items.push(Task {
            title: rest.trim().to_string(),
            done,
            depth,
        });
    }
    TaskList::from_items(items)
}

/// Renders the task list in the canonical `- [ ] title` format.
pub fn serialize_tasks(list: &TaskList) -> String {
    let mut out = String::new();
    for t in list.items() {
        out.push_str(&"  ".repeat(t.depth as usize));
        out.push_str(if t.done { "- [x] " } else { "- [ ] " });
        out.push_str(&t.title);
        out.push('\n');
    }
    out
}

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

/// Data folder: CLI arg > `TBP_DIR` > `<home>/.time-block-planner`.
pub fn resolve_dir(cli: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli {
        return p;
    }
    if let Some(p) = std::env::var_os("TBP_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

#[derive(Clone, Debug)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    fn day_path(&self, date: NaiveDate) -> PathBuf {
        self.dir.join(format!("{}.txt", date.format("%Y-%m-%d")))
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("config.txt")
    }

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

    pub fn load_theme(&self) -> Option<String> {
        let text = fs::read_to_string(self.config_path()).ok()?;
        text.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == "theme")
            .map(|(_, v)| v.trim().to_string())
    }

    pub fn save_theme(&self, name: &str) -> io::Result<()> {
        write_atomic(&self.dir, &self.config_path(), &format!("theme={name}\n"))
    }
}

fn write_atomic(dir: &Path, path: &Path, contents: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Slot};

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    #[test]
    fn parse_and_serialize_roundtrip() {
        let txt = "09:00-10:30 deep Write quarterly report\n11:00-11:30 scheduled Standup\n13:00-13:15 standard\n";
        let d = parse_day(txt);
        assert_eq!(d.blocks().len(), 3);
        assert_eq!(d.blocks()[0].kind, Kind::Deep);
        assert_eq!(d.blocks()[0].title, "Write quarterly report");
        assert_eq!(d.blocks()[2].title, "");
        assert_eq!(serialize_day(&d), txt);
    }

    #[test]
    fn comments_and_blank_lines_ignored() {
        let d = parse_day("# note\n\n09:00-09:15 standard x\n");
        assert_eq!(d.blocks().len(), 1);
        assert!(d.unparsed.is_empty());
    }

    #[test]
    fn unparsed_lines_survive_roundtrip() {
        let d = parse_day(
            "09:00-10:00 deep A\n09:10-09:40 standard off-grid\n09:30-09:45 standard overlap\ngarbage\n",
        );
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(
            d.unparsed,
            vec![
                "09:10-09:40 standard off-grid",
                "09:30-09:45 standard overlap",
                "garbage"
            ]
        );
        assert_eq!(parse_day(&serialize_day(&d)), d);
    }

    #[test]
    fn bom_is_ignored() {
        let d = parse_day("\u{feff}09:00-09:15 deep x\n");
        assert_eq!(d.blocks().len(), 1);
        assert!(d.unparsed.is_empty());
    }

    #[test]
    fn comments_survive_roundtrip() {
        let txt = "# week 46\n09:00-09:15 standard x\n";
        let d = parse_day(txt);
        assert_eq!(d.blocks().len(), 1);
        assert_eq!(serialize_day(&d), txt);
    }

    #[test]
    fn end_of_day_roundtrip() {
        let d = parse_day("23:45-24:00 standard late\n");
        assert_eq!(d.blocks()[0].end, Slot(96));
        assert_eq!(serialize_day(&d), "23:45-24:00 standard late\n");
    }

    #[test]
    fn save_creates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            dir: tmp.path().join("nested/plans"),
        };
        let d = parse_day("09:00-09:15 deep x\n");
        store.save_day(date(), &d, &TaskList::default()).unwrap();
        assert!(store.dir.join("2026-11-10.txt").exists());
        assert_eq!(store.load_day(date()).unwrap(), (d, TaskList::default()));
    }

    #[test]
    fn saving_empty_day_deletes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            dir: tmp.path().to_path_buf(),
        };
        store
            .save_day(
                date(),
                &parse_day("09:00-09:15 deep x\n"),
                &TaskList::default(),
            )
            .unwrap();
        store
            .save_day(date(), &Day::default(), &TaskList::default())
            .unwrap();
        assert!(!store.dir.join("2026-11-10.txt").exists());
    }

    #[test]
    fn missing_file_loads_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            dir: tmp.path().to_path_buf(),
        };
        assert_eq!(
            store.load_day(date()).unwrap(),
            (Day::default(), TaskList::default())
        );
    }

    #[test]
    fn theme_config_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            dir: tmp.path().join("cfg"),
        };
        assert_eq!(store.load_theme(), None);
        store.save_theme("rose-pine").unwrap();
        assert_eq!(store.load_theme().as_deref(), Some("rose-pine"));
    }

    #[test]
    fn resolve_dir_prefers_cli() {
        assert_eq!(resolve_dir(Some("X:/p".into())), PathBuf::from("X:/p"));
    }

    #[test]
    fn tasks_roundtrip() {
        let txt = "- [ ] Write report\n  - [x] Outline\n  - [ ] Draft\n- [ ] Call bank\n";
        let l = parse_tasks(txt);
        assert_eq!(l.len(), 4);
        assert!(l.items()[1].done);
        assert_eq!(l.items()[2].depth, 1);
        assert_eq!(serialize_tasks(&l), txt);
    }

    #[test]
    fn parse_tasks_bare_markers_are_empty_tasks() {
        let l = parse_tasks(
            "- [x]
- [ ]
",
        );
        let got: Vec<_> = l
            .items()
            .iter()
            .map(|t| (t.done, t.title.as_str()))
            .collect();
        assert_eq!(got, [(true, ""), (false, "")]);
    }

    #[test]
    fn parse_tasks_tolerates_hand_edits() {
        let l = parse_tasks("\u{feff}plain line\n\t\t- [X] deep jump\n\n    - [ ]   spaced  \n");
        let got: Vec<_> = l
            .items()
            .iter()
            .map(|t| (t.depth, t.done, t.title.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (0, false, "plain line"),
                (1, true, "deep jump"),
                (2, false, "spaced")
            ]
        );
    }

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
}
