//! First-run setup: choose workspaces, AI sources and a symbol set.
//!
//! A separate Elm-style model, shown before the main screen when no
//! workspace is configured. Pure like the main UI: the binary does the I/O.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::i18n::t;
use crate::icons::{Source, Status, source_glyph, status_glyph};
use crate::model::wrap_step;
use crate::msg::Msg;
use crate::settings::{IconSet, UiSettings};
use crate::theme::Theme;

const ICON_SETS: [IconSet; 3] = [IconSet::Unicode, IconSet::Nerd, IconSet::Ascii];

/// What the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupChoice {
    /// Workspace folders as shown, `~/` allowed.
    pub workspaces: Vec<String>,
    /// Symbol set.
    pub icons: IconSet,
    /// Read Claude Code's logs.
    pub claude: bool,
    /// Read Codex's logs.
    pub codex: bool,
}

/// How the setup ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupOutcome {
    /// Finished with these choices.
    Done(SetupChoice),
    /// Left without saving.
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Workspaces,
    Ai,
    Icons,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notice {
    InvalidPath,
    NeedWorkspace,
}

/// State of the setup screen.
#[derive(Debug)]
pub struct SetupModel {
    settings: UiSettings,
    theme: Theme,
    step: Step,
    /// Suggested and added folders with their selection state.
    folders: Vec<(String, bool)>,
    /// Row under the cursor; `folders.len()` is the input row.
    cursor: usize,
    input: String,
    notice: Option<Notice>,
    icon_cursor: usize,
    /// Claude Code and Codex switched on, in this order.
    sources: [bool; 2],
    /// Whether Codex has written logs on this machine.
    codex_used: bool,
    source_cursor: usize,
}

impl SetupModel {
    /// A setup screen offering `suggestions` (existing folders, `~/`-relative).
    /// Codex starts switched on only if it has been used here.
    #[must_use]
    pub fn new(settings: UiSettings, suggestions: Vec<String>, codex_used: bool) -> Self {
        let cursor = 0;
        Self {
            theme: Theme::new(settings.appearance, settings.color_depth),
            settings,
            step: Step::Workspaces,
            folders: suggestions.into_iter().map(|path| (path, false)).collect(),
            cursor,
            input: String::new(),
            notice: None,
            icon_cursor: 0,
            sources: [true, codex_used],
            codex_used,
            source_cursor: 0,
        }
    }

    fn selected(&self) -> Vec<String> {
        self.folders
            .iter()
            .filter(|(_, on)| *on)
            .map(|(path, _)| path.clone())
            .collect()
    }

    fn on_input_row(&self) -> bool {
        self.cursor == self.folders.len()
    }
}

/// Applies a message; returns how the setup ended, if it did.
pub fn update_setup(model: &mut SetupModel, msg: &Msg) -> Option<SetupOutcome> {
    let Msg::Key(key) = *msg else { return None };
    if key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        return Some(SetupOutcome::Cancelled);
    }
    match model.step {
        Step::Workspaces => on_workspaces_key(model, key),
        Step::Ai => on_ai_key(model, key),
        Step::Icons => on_icons_key(model, key),
    }
}

fn on_workspaces_key(model: &mut SetupModel, key: KeyEvent) -> Option<SetupOutcome> {
    match key.code {
        // The input row below the folders is part of the cycle.
        KeyCode::Up => model.cursor = wrap_step(model.cursor, -1, model.folders.len() + 1),
        KeyCode::Down => model.cursor = wrap_step(model.cursor, 1, model.folders.len() + 1),
        KeyCode::Char(' ') if !model.on_input_row() => {
            if let Some((_, on)) = model.folders.get_mut(model.cursor) {
                *on = !*on;
            }
        }
        KeyCode::Char(c) if model.on_input_row() => {
            model.input.push(c);
            model.notice = None;
        }
        KeyCode::Backspace if model.on_input_row() => {
            model.input.pop();
        }
        KeyCode::Enter if model.on_input_row() && !model.input.trim().is_empty() => {
            add_input(model);
        }
        KeyCode::Enter => {
            if model.selected().is_empty() {
                model.notice = Some(Notice::NeedWorkspace);
            } else {
                model.notice = None;
                model.step = Step::Ai;
            }
        }
        _ => {}
    }
    None
}

fn add_input(model: &mut SetupModel) {
    let path = model.input.trim().trim_end_matches('/').to_owned();
    if !(path.starts_with('/') || path.starts_with("~/")) {
        model.notice = Some(Notice::InvalidPath);
        return;
    }
    match model
        .folders
        .iter_mut()
        .find(|(existing, _)| *existing == path)
    {
        Some((_, on)) => *on = true,
        None => model.folders.push((path, true)),
    }
    model.input.clear();
    model.notice = None;
    model.cursor = model.folders.len();
}

fn on_ai_key(model: &mut SetupModel, key: KeyEvent) -> Option<SetupOutcome> {
    let count = model.sources.len();
    match key.code {
        KeyCode::Up => model.source_cursor = wrap_step(model.source_cursor, -1, count),
        KeyCode::Down => model.source_cursor = wrap_step(model.source_cursor, 1, count),
        KeyCode::Char(' ') => {
            if let Some(on) = model.sources.get_mut(model.source_cursor) {
                *on = !*on;
            }
        }
        KeyCode::Backspace | KeyCode::Left => model.step = Step::Workspaces,
        KeyCode::Enter => model.step = Step::Icons,
        _ => {}
    }
    None
}

fn on_icons_key(model: &mut SetupModel, key: KeyEvent) -> Option<SetupOutcome> {
    match key.code {
        KeyCode::Up => model.icon_cursor = wrap_step(model.icon_cursor, -1, ICON_SETS.len()),
        KeyCode::Down => model.icon_cursor = wrap_step(model.icon_cursor, 1, ICON_SETS.len()),
        KeyCode::Char(c @ '1'..='3') => {
            model.icon_cursor = usize::from(u8::try_from(c).unwrap_or(b'1') - b'1');
        }
        KeyCode::Backspace | KeyCode::Left => model.step = Step::Ai,
        KeyCode::Enter => {
            return Some(SetupOutcome::Done(SetupChoice {
                workspaces: model.selected(),
                icons: ICON_SETS[model.icon_cursor],
                claude: model.sources[0],
                codex: model.sources[1],
            }));
        }
        _ => {}
    }
    None
}

/// Renders the setup screen.
pub fn view_setup(model: &SetupModel, frame: &mut Frame) {
    let language = model.settings.language;
    let theme = &model.theme;
    let [title, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .horizontal_margin(2)
    .areas(frame.area());
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(
        Paragraph::new(Line::styled(
            t(language, "setup-title"),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ))
        .block(block),
        title,
    );
    let body = body.inner(ratatui::layout::Margin::new(0, 1));
    match model.step {
        Step::Workspaces => render_workspaces(model, frame, body),
        Step::Ai => render_ai(model, frame, body),
        Step::Icons => render_icons(model, frame, body),
    }
    let hints: &[(&str, &str)] = match model.step {
        Step::Workspaces | Step::Ai => &[
            ("key-arrows", "hint-move"),
            ("key-space", "hint-toggle"),
            ("key-enter", "hint-next"),
            ("key-esc", "hint-cancel"),
        ],
        Step::Icons => &[
            ("key-arrows", "hint-move"),
            ("key-enter", "hint-finish"),
            ("key-esc", "hint-cancel"),
        ],
    };
    render_hints(language, theme, hints, frame, footer);
}

fn render_workspaces(model: &SetupModel, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut lines = vec![
        Line::styled(
            t(language, "setup-step-workspaces"),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            t(language, "setup-workspaces-help"),
            Style::new().fg(theme.soft),
        ),
        Line::raw(""),
    ];
    for (index, (path, on)) in model.folders.iter().enumerate() {
        let under_cursor = index == model.cursor;
        let mark = if *on { "[x]" } else { "[ ]" };
        let style = if under_cursor {
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.ink)
        };
        lines.push(Line::from(vec![
            Span::styled(
                if under_cursor { "▌ " } else { "  " },
                Style::new().fg(theme.focus),
            ),
            Span::styled(format!("{mark} {path}"), style),
        ]));
    }
    let input_style = if model.on_input_row() {
        Style::new().fg(theme.focus)
    } else {
        Style::new().fg(theme.muted)
    };
    let cursor = if model.on_input_row() { "█" } else { "" };
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            if model.on_input_row() { "▌ " } else { "  " },
            Style::new().fg(theme.focus),
        ),
        Span::styled(format!("{} ", t(language, "setup-add-path")), input_style),
        Span::styled(
            format!("{}{cursor}", model.input),
            Style::new().fg(theme.ink),
        ),
    ]));
    if let Some(notice) = model.notice {
        let message = match notice {
            Notice::InvalidPath => "setup-path-invalid",
            Notice::NeedWorkspace => "setup-need-workspace",
        };
        let glyph = status_glyph(model.settings.icons, Status::Caution);
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("{glyph} {}", t(language, message)),
            Style::new().fg(theme.status(Status::Caution)),
        ));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_ai(model: &SetupModel, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut lines = vec![
        Line::styled(
            t(language, "setup-step-ai"),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Line::styled(t(language, "setup-ai-help"), Style::new().fg(theme.soft)),
        Line::raw(""),
    ];
    // Claude Code is the default source; for Codex the logs decide.
    let rows = [
        ("ai-claude", None),
        (
            "ai-codex",
            Some(if model.codex_used {
                "setup-ai-found"
            } else {
                "setup-ai-missing"
            }),
        ),
    ];
    for (index, (name, note)) in rows.into_iter().enumerate() {
        let under_cursor = index == model.source_cursor;
        let mark = if model.sources[index] { "[x]" } else { "[ ]" };
        let style = if under_cursor {
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.ink)
        };
        let mut spans = vec![
            Span::styled(
                if under_cursor { "▌ " } else { "  " },
                Style::new().fg(theme.focus),
            ),
            Span::styled(format!("{mark} {}", t(language, name)), style),
        ];
        if let Some(note) = note {
            spans.push(Span::styled(
                format!("   {}", t(language, note)),
                Style::new().fg(theme.soft),
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_icons(model: &SetupModel, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut lines = vec![
        Line::styled(
            t(language, "setup-step-icons"),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Line::styled(t(language, "setup-icons-help"), Style::new().fg(theme.soft)),
        Line::raw(""),
    ];
    let statuses = [
        Status::Ok,
        Status::Advisory,
        Status::Caution,
        Status::Warning,
        Status::Neutral,
    ];
    let sources = [Source::Git, Source::Homebrew, Source::Npm, Source::Ai];
    for (index, icons) in ICON_SETS.iter().enumerate() {
        let under_cursor = index == model.icon_cursor;
        let label = match icons {
            IconSet::Unicode => "setup-icons-unicode",
            IconSet::Nerd => "setup-icons-nerd",
            IconSet::Ascii => "setup-icons-ascii",
        };
        let mut spans = vec![
            Span::styled(
                if under_cursor { "▌ " } else { "  " },
                Style::new().fg(theme.focus),
            ),
            Span::styled(format!("{}  ", index + 1), Style::new().fg(theme.soft)),
        ];
        for status in statuses {
            spans.push(Span::styled(
                format!("{} ", status_glyph(*icons, status)),
                Style::new().fg(theme.status(status)),
            ));
        }
        let mut cells = statuses.len() * 2;
        for source in sources {
            if let Some(glyph) = source_glyph(*icons, source) {
                spans.push(Span::styled(
                    format!("{glyph} "),
                    Style::new().fg(theme.muted),
                ));
                cells += 2;
            }
        }
        // Every glyph takes one cell plus a space; pad so the labels line up.
        let widest = (statuses.len() + sources.len()) * 2;
        spans.push(Span::raw(" ".repeat(widest.saturating_sub(cells))));
        let style = if under_cursor {
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.ink)
        };
        spans.push(Span::styled(format!("  {}", t(language, label)), style));
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_hints(
    language: crate::settings::Language,
    theme: &Theme,
    hints: &[(&str, &str)],
    frame: &mut Frame,
    area: Rect,
) {
    let mut spans = Vec::with_capacity(hints.len() * 2);
    for (key, label) in hints {
        spans.push(Span::styled(
            t(language, key),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}   ", t(language, label)),
            Style::new().fg(theme.soft),
        ));
    }
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::settings::Language;

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn typed(model: &mut SetupModel, text: &str) {
        for c in text.chars() {
            assert!(update_setup(model, &key(KeyCode::Char(c))).is_none());
        }
    }

    fn model() -> SetupModel {
        SetupModel::new(
            UiSettings::default(),
            vec!["~/workspace".into(), "~/Developer".into()],
            false,
        )
    }

    /// Selects the first suggestion and walks past the AI step.
    fn on_icon_step(model: &mut SetupModel) {
        update_setup(model, &key(KeyCode::Char(' ')));
        update_setup(model, &key(KeyCode::Enter));
        update_setup(model, &key(KeyCode::Enter));
    }

    #[test]
    fn folder_cursor_wraps_through_the_input_row() {
        let mut model = model();
        update_setup(&mut model, &key(KeyCode::Up));
        assert!(model.on_input_row(), "up from the first folder");
        update_setup(&mut model, &key(KeyCode::Down));
        assert_eq!(model.cursor, 0, "down from the input row");
    }

    #[test]
    fn icon_cursor_wraps_at_both_ends() {
        let mut model = model();
        on_icon_step(&mut model);
        update_setup(&mut model, &key(KeyCode::Up));
        assert_eq!(
            model.icon_cursor,
            ICON_SETS.len() - 1,
            "up from the first set"
        );
        update_setup(&mut model, &key(KeyCode::Down));
        assert_eq!(model.icon_cursor, 0, "down from the last set");
    }

    #[test]
    fn selecting_a_suggestion_and_finishing_returns_the_choice() {
        let mut model = model();
        on_icon_step(&mut model);
        update_setup(&mut model, &key(KeyCode::Down));
        let outcome = update_setup(&mut model, &key(KeyCode::Enter));
        assert_eq!(
            outcome,
            Some(SetupOutcome::Done(SetupChoice {
                workspaces: vec!["~/workspace".into()],
                icons: IconSet::Nerd,
                claude: true,
                codex: false,
            }))
        );
    }

    #[test]
    fn codex_starts_on_only_where_it_was_used() {
        let used = SetupModel::new(UiSettings::default(), Vec::new(), true);
        assert_eq!(used.sources, [true, true]);
        assert_eq!(model().sources, [true, false]);
    }

    #[test]
    fn the_ai_step_switches_sources() {
        let mut model = model();
        update_setup(&mut model, &key(KeyCode::Char(' ')));
        update_setup(&mut model, &key(KeyCode::Enter));
        assert_eq!(model.step, Step::Ai);
        update_setup(&mut model, &key(KeyCode::Char(' ')));
        update_setup(&mut model, &key(KeyCode::Down));
        update_setup(&mut model, &key(KeyCode::Char(' ')));
        update_setup(&mut model, &key(KeyCode::Enter));
        let Some(SetupOutcome::Done(choice)) = update_setup(&mut model, &key(KeyCode::Enter))
        else {
            panic!("expected done");
        };
        assert!(!choice.claude);
        assert!(choice.codex);
    }

    #[test]
    fn left_goes_back_one_step() {
        let mut model = model();
        on_icon_step(&mut model);
        update_setup(&mut model, &key(KeyCode::Left));
        assert_eq!(model.step, Step::Ai);
        update_setup(&mut model, &key(KeyCode::Left));
        assert_eq!(model.step, Step::Workspaces);
    }

    #[test]
    fn typed_path_is_added_and_selected() {
        let mut model = model();
        update_setup(&mut model, &key(KeyCode::Down));
        update_setup(&mut model, &key(KeyCode::Down));
        typed(&mut model, "/Volumes/Ext/projects/");
        update_setup(&mut model, &key(KeyCode::Enter));
        assert_eq!(model.selected(), vec!["/Volumes/Ext/projects".to_owned()]);
    }

    #[test]
    fn q_in_the_input_is_text_not_quit() {
        let mut model = model();
        update_setup(&mut model, &key(KeyCode::Down));
        update_setup(&mut model, &key(KeyCode::Down));
        typed(&mut model, "~/q");
        assert_eq!(model.input, "~/q");
    }

    #[test]
    fn relative_path_is_refused_with_a_notice() {
        let mut model = model();
        model.cursor = model.folders.len();
        typed(&mut model, "projects");
        update_setup(&mut model, &key(KeyCode::Enter));
        assert_eq!(model.notice, Some(Notice::InvalidPath));
        assert_eq!(model.selected(), Vec::<String>::new());
    }

    #[test]
    fn next_step_needs_a_workspace() {
        let mut model = model();
        update_setup(&mut model, &key(KeyCode::Enter));
        assert_eq!(model.step, Step::Workspaces);
        assert_eq!(model.notice, Some(Notice::NeedWorkspace));
    }

    #[test]
    fn escape_cancels_from_any_step() {
        let mut model = model();
        assert_eq!(
            update_setup(&mut model, &key(KeyCode::Esc)),
            Some(SetupOutcome::Cancelled)
        );
    }

    #[test]
    fn number_keys_pick_the_icon_set() {
        let mut model = model();
        on_icon_step(&mut model);
        update_setup(&mut model, &key(KeyCode::Char('3')));
        let Some(SetupOutcome::Done(choice)) = update_setup(&mut model, &key(KeyCode::Enter))
        else {
            panic!("expected done");
        };
        assert_eq!(choice.icons, IconSet::Ascii);
    }

    fn rendered(model: &SetupModel) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(90, 16)).expect("BUG");
        terminal
            .draw(|frame| view_setup(model, frame))
            .expect("BUG");
        terminal
    }

    fn with_one_workspace(language: Language) -> SetupModel {
        let mut model = SetupModel::new(
            UiSettings {
                language,
                ..UiSettings::default()
            },
            vec!["~/workspace".into()],
            false,
        );
        update_setup(&mut model, &key(KeyCode::Char(' ')));
        model
    }

    const LANGUAGES: [(Language, &str); 2] =
        [(Language::De, "in_german"), (Language::En, "in_english")];

    #[test]
    fn workspaces_step_renders() {
        for (language, suffix) in LANGUAGES {
            insta::assert_snapshot!(
                format!("workspaces_step_renders_{suffix}"),
                rendered(&with_one_workspace(language)).backend()
            );
        }
    }

    #[test]
    fn ai_step_renders() {
        for (language, suffix) in LANGUAGES {
            let mut model = with_one_workspace(language);
            update_setup(&mut model, &key(KeyCode::Enter));
            insta::assert_snapshot!(
                format!("ai_step_renders_{suffix}"),
                rendered(&model).backend()
            );
        }
    }

    #[test]
    fn symbol_step_renders() {
        for (language, suffix) in LANGUAGES {
            let mut model = with_one_workspace(language);
            update_setup(&mut model, &key(KeyCode::Enter));
            update_setup(&mut model, &key(KeyCode::Enter));
            insta::assert_snapshot!(
                format!("symbol_step_renders_{suffix}"),
                rendered(&model).backend()
            );
        }
    }
}
