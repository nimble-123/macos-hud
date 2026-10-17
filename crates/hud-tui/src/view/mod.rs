//! Rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

mod log;
mod settings;
mod updates;
mod workspaces;

use crate::i18n::{t, t_args};
use crate::settings::Language;
use crate::theme::Theme;
use crate::{Model, Tab};

/// Renders the model into a frame. Pure: reads the model, writes the frame.
pub fn view(model: &Model, frame: &mut Frame) {
    // While work runs in the background the footer gets an extra progress line.
    let busy = model.scan.is_some() || model.syncing || !model.jobs.is_empty();
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(if busy { 3 } else { 2 }),
    ])
    .horizontal_margin(2)
    .areas(frame.area());

    let language = model.settings.language;
    render_tabs(model.tab(), language, &model.theme, frame, tabs);
    let body = body.inner(Margin::new(0, 1));
    match model.tab() {
        Tab::Workspaces => workspaces::render(model, frame, body),
        Tab::Updates => updates::render(model, frame, body),
        Tab::Settings => settings::render(model, frame, body),
        Tab::Log => log::render(model, frame, body),
        tab => render_placeholder(tab, language, &model.theme, frame, body),
    }
    render_footer(model, frame, footer);
    if model.quit == crate::model::Quit::Asking {
        render_quit(model, frame, body);
    }
}

/// Asks whether to wait for running installations or to cancel them.
fn render_quit(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let [dialog] = Layout::horizontal([Constraint::Length(72)])
        .flex(Flex::Center)
        .areas(area);
    let [dialog] = Layout::vertical([Constraint::Length(9)])
        .flex(Flex::Center)
        .areas(dialog);
    let key = Style::new().fg(theme.focus).add_modifier(Modifier::BOLD);
    let label = Style::new().fg(theme.soft);
    let count = u32::try_from(model.jobs.len()).unwrap_or(u32::MAX);
    let caution = crate::icons::Status::Caution;
    let lines = vec![
        Line::raw(""),
        Line::styled(
            format!(
                "  {}",
                crate::i18n::t_count(language, "quit-running", "count", count)
            ),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(
            format!(
                "  {} {}",
                crate::icons::status_glyph(model.settings.icons, caution),
                t(language, "quit-cancel-warning")
            ),
            Style::new().fg(theme.status(caution)),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("  {} ", t(language, "key-enter")), key),
            Span::styled(format!("{}   ", t(language, "quit-wait")), label),
            Span::styled(format!("{} ", t(language, "key-a")), key),
            Span::styled(format!("{}   ", t(language, "quit-cancel")), label),
            Span::styled(format!("{} ", t(language, "key-esc")), key),
            Span::styled(t(language, "quit-back"), label),
        ]),
    ];
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(" {} ", t(language, "quit-title")),
            key,
        ));
    frame.render_widget(Clear, dialog);
    frame.render_widget(Paragraph::new(lines).block(block), dialog);
}

fn tab_message(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "tab-overview",
        Tab::Workspaces => "tab-workspaces",
        Tab::Updates => "tab-updates",
        Tab::Ai => "tab-ai",
        Tab::Log => "tab-log",
        Tab::Settings => "tab-settings",
    }
}

fn render_tabs(active: Tab, language: Language, theme: &Theme, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::with_capacity(Tab::ALL.len() * 2);
    for tab in Tab::ALL {
        let style = if tab == active {
            Style::new()
                .fg(theme.focus)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(theme.muted)
        };
        spans.push(Span::styled(t(language, tab_message(tab)), style));
        spans.push(Span::raw("    "));
    }
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_placeholder(
    active: Tab,
    language: Language,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let title = t(language, tab_message(active));
    let text = t_args(language, "tab-placeholder", &[("tab", &title)]);
    frame.render_widget(
        Paragraph::new(Line::styled(text, Style::new().fg(theme.soft))),
        area,
    );
}

fn render_footer(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let hints = footer_hints(model);
    let mut spans = Vec::with_capacity(hints.len() * 2 + 1);
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
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Running installations matter most: they change the system.
    let progress = match model.scan {
        _ if !model.jobs.is_empty() => jobs_progress(model),
        Some(scan) => scan_progress(model, scan),
        None if model.syncing => vec![Span::styled(
            t(language, "syncing"),
            Style::new().fg(theme.status(crate::icons::Status::Advisory)),
        )],
        None => Vec::new(),
    };
    let mut lines = Vec::with_capacity(2);
    if !progress.is_empty() {
        lines.push(Line::from(progress));
    }
    lines.push(Line::from(spans));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Key hints for the active tab; an open input or dialog has its own.
fn footer_hints(model: &Model) -> Vec<(&'static str, &'static str)> {
    if model.quit == crate::model::Quit::Waiting {
        return vec![("key-quit", "hint-quit-now")];
    }
    let tab = &model.settings_tab;
    if model.tab() == Tab::Settings && tab.input.is_some() {
        return vec![("key-enter", "hint-add"), ("key-esc", "hint-cancel")];
    }
    if model.tab() == Tab::Log && model.log_tab.searching {
        return vec![("key-enter", "hint-finish"), ("key-esc", "hint-cancel")];
    }
    let mut hints = Vec::with_capacity(7);
    match model.tab() {
        Tab::Updates => hints.extend([("key-arrows", "hint-select"), ("key-u", "hint-install")]),
        Tab::Workspaces => hints.extend([
            ("key-arrows", "hint-select"),
            ("key-f", "hint-fast-forward"),
        ]),
        Tab::Settings => hints.extend([
            ("key-arrows", "hint-select"),
            ("key-enter", "hint-change"),
            ("key-a", "hint-add"),
            ("key-d", "hint-remove"),
        ]),
        Tab::Log => hints.extend([
            ("key-arrows", "hint-select"),
            ("key-slash", "hint-search"),
            ("key-f", "hint-filter"),
        ]),
        Tab::Overview | Tab::Ai => {}
    }
    let refresh = if model.tab() == Tab::Settings {
        "hint-reload"
    } else {
        "hint-refresh"
    };
    hints.extend([
        ("key-r", refresh),
        ("key-tab", "hint-switch"),
        ("key-quit", "hint-quit"),
    ]);
    hints
}

/// Spinner, number of running installations and the first one's command.
fn jobs_progress(model: &Model) -> Vec<Span<'static>> {
    let theme = &model.theme;
    let accent = theme.status(crate::icons::Status::Advisory);
    let count = u32::try_from(model.jobs.len()).unwrap_or(u32::MAX);
    let message = if model.quit == crate::model::Quit::Waiting {
        "quit-waiting"
    } else {
        "jobs-running"
    };
    let mut spans = vec![Span::styled(
        format!(
            "{} {}",
            updates::spinner(model),
            crate::i18n::t_count(model.settings.language, message, "count", count)
        ),
        Style::new().fg(accent),
    )];
    if let Some(job) = model.jobs.values().next() {
        spans.push(Span::styled(
            format!("   {}", updates::command_preview(&job.target)),
            Style::new().fg(theme.muted),
        ));
    }
    spans
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const BAR_WIDTH: usize = 20;

/// Spinner, label, bar and count, e.g. `⠋ Lokaler Scan ████████░░░░ 23 / 79`.
fn scan_progress(model: &Model, scan: crate::model::ScanProgress) -> Vec<Span<'static>> {
    let theme = &model.theme;
    let accent = theme.status(crate::icons::Status::Advisory);
    let frame = SPINNER[usize::try_from(model.ticks % 10).unwrap_or(0)];
    let label = t(
        model.settings.language,
        if scan.fetch {
            "scan-sync"
        } else {
            "scan-local"
        },
    );
    let filled = (scan.done * BAR_WIDTH)
        .checked_div(scan.total)
        .unwrap_or(BAR_WIDTH);
    vec![
        Span::styled(format!("{frame} {label} "), Style::new().fg(accent)),
        Span::styled("█".repeat(filled), Style::new().fg(accent)),
        Span::styled("░".repeat(BAR_WIDTH - filled), Style::new().fg(theme.rule)),
        Span::styled(
            format!(" {} / {}", scan.done, scan.total),
            Style::new().fg(theme.muted),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::settings::UiSettings;
    use crate::{Msg, update};

    fn render(model: &Model) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(model, frame))
            .expect("BUG: test draw");
        terminal
    }

    fn german() -> Model {
        Model::new(UiSettings {
            language: Language::De,
            ..UiSettings::default()
        })
    }

    #[test]
    fn overview_is_active_on_start() {
        insta::assert_snapshot!(render(&Model::default()).backend());
    }

    #[test]
    fn tab_key_activates_workspaces() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render(&model).backend());
    }

    #[test]
    fn footer_shows_scan_progress_on_every_tab() {
        use hud_application::AppEvent;
        let mut model = german();
        update(
            &mut model,
            Msg::App(AppEvent::ScanStarted {
                total: 4,
                fetch: false,
            }),
        );
        insta::assert_snapshot!(render(&model).backend());
    }

    fn settings_tab(language: Language) -> Model {
        use hud_application::AppEvent;
        use hud_application::settings::{Settings, WorkspaceSettings};
        let mut model = Model::new(UiSettings {
            language,
            ..UiSettings::default()
        });
        model.tab = Tab::Settings;
        let workspace = WorkspaceSettings {
            path: hud_domain::WorkspacePath::try_from(std::path::PathBuf::from("/ws"))
                .expect("BUG: absolute"),
            auto_fast_forward: false,
        };
        let settings = Settings {
            workspaces: vec![workspace],
            ..Settings::default()
        };
        update(&mut model, Msg::App(AppEvent::Settings(Box::new(settings))));
        model
    }

    fn render_tall(model: &Model) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 22)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(model, frame))
            .expect("BUG: test draw");
        terminal
    }

    fn log_tab(language: Language) -> Model {
        use hud_application::AppEvent;
        use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
        let at = |text: &str| text.parse::<jiff::Timestamp>().expect("BUG: timestamp");
        let record =
            |id, kind, target: &str, trigger, outcome, summary: Option<&str>| ActionRecord {
                id: ActionId(id),
                started_at: at("2026-10-17T12:00:00Z"),
                finished_at: Some(at("2026-10-17T12:01:05Z")),
                kind,
                target: target.to_owned(),
                trigger,
                outcome: Some(outcome),
                summary: summary.map(str::to_owned),
            };
        let mut model = Model::new(UiSettings {
            language,
            ..UiSettings::default()
        });
        model.tab = Tab::Log;
        let actions = vec![
            record(
                3,
                ActionKind::Install,
                "Npm wrangler 4.81.0 → 4.151.1",
                Trigger::Manual,
                Outcome::Failed(
                    "npm failed: npm error code EACCES\nnpm error path /usr/local/lib".into(),
                ),
                None,
            ),
            record(
                2,
                ActionKind::FastForward,
                "/ws/api",
                Trigger::Auto,
                Outcome::Succeeded,
                Some("3 commits to 1a2b3c4d5e6f"),
            ),
            record(
                1,
                ActionKind::Fetch,
                "/ws/web",
                Trigger::Auto,
                Outcome::Failed("timeout".into()),
                None,
            ),
        ];
        update(&mut model, Msg::App(AppEvent::Actions(actions)));
        model
    }

    #[test]
    fn quit_dialog_in_german() {
        use hud_application::AppEvent;
        use hud_application::usecases::updates::{JobId, JobState, JobUpdate};
        let mut model = german();
        let job = JobUpdate {
            id: JobId(1),
            target: hud_domain::update::InstallTarget::AllSkills,
            state: JobState::Running,
        };
        update(&mut model, Msg::App(AppEvent::Job(job)));
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render_tall(&model).backend());
    }

    #[test]
    fn log_tab_in_german() {
        insta::assert_snapshot!(render_tall(&log_tab(Language::De)).backend());
    }

    #[test]
    fn log_tab_in_english() {
        insta::assert_snapshot!(render_tall(&log_tab(Language::En)).backend());
    }

    #[test]
    fn settings_tab_in_german() {
        insta::assert_snapshot!(render_tall(&settings_tab(Language::De)).backend());
    }

    #[test]
    fn settings_tab_in_english() {
        insta::assert_snapshot!(render_tall(&settings_tab(Language::En)).backend());
    }

    #[test]
    fn settings_remove_dialog() {
        let mut model = settings_tab(Language::De);
        for _ in 0..3 {
            update(
                &mut model,
                Msg::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            );
        }
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render_tall(&model).backend());
    }

    #[test]
    fn german_overview_uses_german_texts() {
        insta::assert_snapshot!(render(&german()).backend());
    }
}
