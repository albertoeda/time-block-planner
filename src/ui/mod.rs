//! eframe/egui front end: maps input to `Command`s and draws `AppState` in a terminal style.

pub mod chrome;
pub mod tasks;
pub mod timeline;

use chrono::{Local, NaiveDate};
use egui::{Event, FontFamily, Key, Modifiers, TextStyle};

use crate::model::{Kind, Slot};
use crate::state::{AppState, Command, Focus, Mode};
use crate::storage::Store;
use crate::theme::themes;

/// Maps a non-text key press to a command for the given mode (see the plan's Keymap).
/// `Ctrl` here is `Modifiers::command`, i.e. Cmd on macOS.
pub fn key_to_command(key: Key, m: Modifiers, mode: &Mode) -> Option<Command> {
    use Command as C;
    let plain = !m.command && !m.alt && !m.shift;
    let cmd_only = m.command && !m.alt && !m.shift;
    let arrows_only = !m.command && !m.alt;
    match mode {
        Mode::Normal => Some(match key {
            Key::ArrowUp | Key::ArrowDown => {
                let up = key == Key::ArrowUp;
                let pick = |a: C, b: C| if up { a } else { b };
                match (m.command, m.alt, m.shift) {
                    (true, false, false) | (false, true, true) => pick(C::Shrink, C::Grow),
                    (true, false, true) => pick(C::PrevBlock, C::NextBlock),
                    (false, true, false) => pick(C::MoveUp, C::MoveDown),
                    (false, false, true) => pick(C::ExtendUp, C::ExtendDown),
                    (false, false, false) => pick(C::Up, C::Down),
                    _ => return None,
                }
            }
            Key::ArrowLeft if m.alt && !m.command && !m.shift => C::Outdent,
            Key::ArrowRight if m.alt && !m.command && !m.shift => C::Indent,
            Key::ArrowLeft if m.shift && !m.command && !m.alt => C::PrevDay,
            Key::ArrowRight if m.shift && !m.command && !m.alt => C::NextDay,
            Key::ArrowLeft if m.alt && m.shift && !m.command => C::MoveTaskToPrevDay,
            Key::ArrowRight if m.alt && m.shift && !m.command => C::MoveTaskToNextDay,
            Key::ArrowLeft if plain => C::FocusPane(Focus::Tasks),
            Key::ArrowRight if plain => C::FocusPane(Focus::Timeline),
            Key::PageUp if plain => C::PageUp,
            Key::PageDown if plain => C::PageDown,
            Key::Home if plain => C::Home,
            Key::End if plain => C::End,
            Key::Tab if !m.command && !m.alt => {
                if m.shift {
                    C::Outdent
                } else {
                    C::Indent
                }
            }
            Key::Enter | Key::F2 if plain => C::EditTitle,
            Key::Delete | Key::Backspace if plain => C::Delete,
            Key::F1 if plain => C::ToggleHelp,
            Key::N if cmd_only => C::New,
            Key::Z if cmd_only => C::Undo,
            Key::Z if m.command && m.shift && !m.alt => C::Redo,
            Key::Y if cmd_only => C::Redo,
            Key::T if cmd_only => C::Today,
            Key::S if cmd_only => C::SaveNow,
            Key::Comma if cmd_only => C::OpenThemes,
            _ => return None,
        }),
        Mode::Create { .. } => match key {
            Key::ArrowUp if arrows_only => Some(C::Up),
            Key::ArrowDown if arrows_only => Some(C::Down),
            Key::Enter => Some(C::Confirm),
            Key::Escape => Some(C::Cancel),
            _ => None,
        },
        Mode::EditTitle { .. } | Mode::EditTask { .. } => match key {
            Key::Enter => Some(C::Confirm),
            Key::Escape => Some(C::Cancel),
            Key::Backspace => Some(C::Backspace),
            _ => None,
        },
        Mode::MoveBlock { .. } => (key == Key::Escape).then_some(C::Cancel),
        Mode::ThemePicker { .. } => match key {
            Key::ArrowUp => Some(C::Up),
            Key::ArrowDown => Some(C::Down),
            Key::Enter => Some(C::Confirm),
            Key::Escape => Some(C::Cancel),
            _ => None,
        },
        Mode::Help => matches!(key, Key::Escape | Key::F1).then_some(C::ToggleHelp),
    }
}

/// Maps typed text to a command. In title editing, all text is literal.
pub fn text_to_command(text: &str, mode: &Mode) -> Option<Command> {
    match mode {
        Mode::EditTitle { .. } | Mode::EditTask { .. } => Some(Command::Text(text.to_string())),
        Mode::Normal => match text {
            "n" => Some(Command::New),
            "N" => Some(Command::NewChild),
            "d" => Some(Command::Delete),
            "t" => Some(Command::Today),
            "x" | " " => Some(Command::ToggleDone),
            "p" => Some(Command::ScheduleTask),
            "[" => Some(Command::MoveTaskToPrevDay),
            "]" => Some(Command::MoveTaskToNextDay),
            ">" => Some(Command::Indent),
            "<" => Some(Command::Outdent),
            "1" => Some(Command::SetKind(Kind::Standard)),
            "2" => Some(Command::SetKind(Kind::Scheduled)),
            "3" => Some(Command::SetKind(Kind::Deep)),
            "?" => Some(Command::ToggleHelp),
            _ => None,
        },
        Mode::Help => (text == "?").then_some(Command::ToggleHelp),
        _ => None,
    }
}

pub struct PlannerApp {
    state: AppState,
    applied_theme: Option<usize>,
    /// Cursor position last scrolled to; scroll only when it changes so the wheel still works.
    scrolled_to: Option<(NaiveDate, Slot)>,
}

impl PlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> PlannerApp {
        cc.egui_ctx.all_styles_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.family = FontFamily::Monospace;
            }
            if let Some(body) = style.text_styles.get_mut(&TextStyle::Monospace) {
                body.size = 15.0;
            }
        });
        PlannerApp {
            state: AppState::new(store, today()),
            applied_theme: None,
            scrolled_to: None,
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let cmd = match &event {
                Event::Key {
                    key: Key::Q,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    None
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => key_to_command(*key, *modifiers, &self.state.mode),
                Event::Text(t) => text_to_command(t, &self.state.mode),
                _ => None,
            };
            if let Some(cmd) = cmd {
                self.state.apply(cmd, today());
            }
        }
        // We own the keyboard: keep egui from using Tab/arrows for focus or scrolling.
        ctx.input_mut(|i| {
            i.events
                .retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_)))
        });
    }
}

impl eframe::App for PlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);

        if self.applied_theme != Some(self.state.theme) {
            let v = chrome::visuals(&themes()[self.state.theme]);
            ctx.set_visuals_of(egui::Theme::Dark, v.clone());
            ctx.set_visuals_of(egui::Theme::Light, v);
            self.applied_theme = Some(self.state.theme);
        }
        let theme = &themes()[self.state.theme];

        chrome::status_bar(ui, &self.state, theme);

        // First frame: open with START_SLOT (08:00) at the top; afterwards follow the cursor.
        let target = (self.state.date, self.state.cursor);
        let scroll = match self.scrolled_to {
            None => timeline::Scroll::Start,
            Some(t) if t != target => timeline::Scroll::Cursor,
            Some(_) => timeline::Scroll::None,
        };
        self.scrolled_to = Some(target);
        tasks::show(ui, &mut self.state, theme);
        timeline::show(ui, &mut self.state, theme, scroll);

        match self.state.mode {
            Mode::Help => chrome::help_overlay(&ctx, theme),
            Mode::ThemePicker { .. } => chrome::theme_picker(&ctx, &self.state, theme),
            _ => {}
        }
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use egui::{Key, Modifiers as M};

    #[test]
    fn platform_shortcuts() {
        let n = &Mode::Normal;
        assert_eq!(key_to_command(Key::N, M::COMMAND, n), Some(Command::New));
        assert_eq!(key_to_command(Key::Z, M::COMMAND, n), Some(Command::Undo));
        assert_eq!(
            key_to_command(Key::Z, M::COMMAND | M::SHIFT, n),
            Some(Command::Redo)
        );
        assert_eq!(key_to_command(Key::Y, M::COMMAND, n), Some(Command::Redo));
        assert_eq!(key_to_command(Key::T, M::COMMAND, n), Some(Command::Today));
        assert_eq!(
            key_to_command(Key::S, M::COMMAND, n),
            Some(Command::SaveNow)
        );
        assert_eq!(
            key_to_command(Key::Comma, M::COMMAND, n),
            Some(Command::OpenThemes)
        );
        assert_eq!(
            key_to_command(Key::F2, M::NONE, n),
            Some(Command::EditTitle)
        );
        assert_eq!(
            key_to_command(Key::Enter, M::NONE, n),
            Some(Command::EditTitle)
        );
        assert_eq!(
            key_to_command(Key::F1, M::NONE, n),
            Some(Command::ToggleHelp)
        );
        assert_eq!(
            key_to_command(Key::ArrowDown, M::ALT, n),
            Some(Command::MoveDown)
        );
        assert_eq!(
            key_to_command(Key::ArrowDown, M::ALT | M::SHIFT, n),
            Some(Command::Grow)
        );
        assert_eq!(
            key_to_command(Key::ArrowUp, M::ALT | M::SHIFT, n),
            Some(Command::Shrink)
        );
        assert_eq!(
            key_to_command(Key::ArrowDown, M::SHIFT, n),
            Some(Command::ExtendDown)
        );
        assert_eq!(
            key_to_command(Key::ArrowUp, M::COMMAND, n),
            Some(Command::Shrink)
        );
        assert_eq!(
            key_to_command(Key::ArrowDown, M::COMMAND, n),
            Some(Command::Grow)
        );
        assert_eq!(
            key_to_command(Key::ArrowUp, M::COMMAND | M::SHIFT, n),
            Some(Command::PrevBlock)
        );
        assert_eq!(
            key_to_command(Key::ArrowRight, M::SHIFT, n),
            Some(Command::NextDay)
        );
        assert_eq!(
            key_to_command(Key::ArrowLeft, M::SHIFT, n),
            Some(Command::PrevDay)
        );
        assert_eq!(
            key_to_command(Key::Tab, M::SHIFT, n),
            Some(Command::Outdent)
        );
        assert_eq!(
            key_to_command(Key::Delete, M::NONE, n),
            Some(Command::Delete)
        );
        assert_eq!(key_to_command(Key::N, M::NONE, n), None);
    }

    #[test]
    fn modal_keys() {
        let c = &Mode::Create {
            anchor: crate::model::Slot(0),
            title: None,
        };
        assert_eq!(
            key_to_command(Key::ArrowDown, M::SHIFT, c),
            Some(Command::Down)
        );
        assert_eq!(
            key_to_command(Key::Enter, M::NONE, c),
            Some(Command::Confirm)
        );
        assert_eq!(
            key_to_command(Key::Escape, M::NONE, c),
            Some(Command::Cancel)
        );
        let t = &Mode::ThemePicker { original: 0 };
        assert_eq!(key_to_command(Key::ArrowUp, M::NONE, t), Some(Command::Up));
        assert_eq!(
            key_to_command(Key::Escape, M::NONE, &Mode::Help),
            Some(Command::ToggleHelp)
        );
        assert_eq!(text_to_command("?", &Mode::Help), Some(Command::ToggleHelp));
    }

    #[test]
    fn backspace_maps_by_mode() {
        let e = Mode::EditTitle {
            idx: 0,
            original: String::new(),
            is_new: false,
        };
        assert_eq!(
            key_to_command(Key::Backspace, M::NONE, &Mode::Normal),
            Some(Command::Delete)
        );
        assert_eq!(
            key_to_command(Key::Backspace, M::NONE, &e),
            Some(Command::Backspace)
        );
        assert_eq!(key_to_command(Key::ArrowLeft, M::NONE, &e), None);
        assert_eq!(text_to_command("1", &e), Some(Command::Text("1".into())));
        assert_eq!(
            text_to_command("1", &Mode::Normal),
            Some(Command::SetKind(Kind::Standard))
        );
        assert_eq!(
            text_to_command("3", &Mode::Normal),
            Some(Command::SetKind(Kind::Deep))
        );
        assert_eq!(text_to_command("?", &e), Some(Command::Text("?".into())));
        assert_eq!(text_to_command("z", &Mode::Normal), None);
    }

    #[test]
    fn shortcuts_only_outside_editing() {
        let n = &Mode::Normal;
        let et = &Mode::EditTask {
            idx: 0,
            original: String::new(),
            is_new: true,
        };
        for (ch, cmd) in [
            ("n", Command::New),
            ("N", Command::NewChild),
            ("x", Command::ToggleDone),
            (" ", Command::ToggleDone),
            ("p", Command::ScheduleTask),
            (">", Command::Indent),
            ("<", Command::Outdent),
            ("d", Command::Delete),
            ("t", Command::Today),
            ("[", Command::MoveTaskToPrevDay),
            ("]", Command::MoveTaskToNextDay),
        ] {
            assert_eq!(text_to_command(ch, n), Some(cmd), "{ch}");
            assert_eq!(
                text_to_command(ch, et),
                Some(Command::Text(ch.into())),
                "{ch}"
            );
        }
    }

    #[test]
    fn pane_and_outline_keys() {
        let n = &Mode::Normal;
        assert_eq!(key_to_command(Key::Tab, M::NONE, n), Some(Command::Indent));
        assert_eq!(
            key_to_command(Key::ArrowLeft, M::NONE, n),
            Some(Command::FocusPane(Focus::Tasks))
        );
        assert_eq!(
            key_to_command(Key::ArrowRight, M::NONE, n),
            Some(Command::FocusPane(Focus::Timeline))
        );
        assert_eq!(
            key_to_command(Key::ArrowDown, M::COMMAND | M::SHIFT, n),
            Some(Command::NextBlock)
        );
        assert_eq!(text_to_command("e", n), None);
        assert_eq!(text_to_command("b", n), None);
        assert_eq!(
            key_to_command(Key::ArrowRight, M::ALT, n),
            Some(Command::Indent)
        );
        assert_eq!(
            key_to_command(Key::ArrowLeft, M::ALT, n),
            Some(Command::Outdent)
        );
        assert_eq!(key_to_command(Key::Home, M::NONE, n), Some(Command::Home));
        let mv = &Mode::MoveBlock {
            idx: 0,
            grab: 0,
            target: Slot(0),
        };
        assert_eq!(
            key_to_command(Key::Escape, M::NONE, mv),
            Some(Command::Cancel)
        );
        assert_eq!(key_to_command(Key::ArrowDown, M::NONE, mv), None);
    }

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
}
