//! Rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

mod ai;
mod ai_kpis;
mod calendar;
mod help;
mod log;
mod overview;
mod settings;
mod updates;
mod workspaces;

use crate::i18n::t;
use crate::settings::Language;
use crate::theme::Theme;
use crate::{Model, Tab};

pub(crate) use overview::SPARK_SAMPLES;

/// While work runs in the background the footer gets an extra progress line.
fn busy(model: &Model) -> bool {
    model.scan.is_some() || model.syncing || !model.jobs.is_empty()
}

/// Tab bar, body and footer of a frame; shared with mouse hit-testing so a
/// click resolves against exactly what was drawn.
pub(crate) fn areas(model: &Model, full: Rect) -> [Rect; 3] {
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(if busy(model) { 3 } else { 2 }),
    ])
    .horizontal_margin(2)
    .areas(full);
    [tabs, body.inner(Margin::new(0, 1)), footer]
}

/// Renders the model into a frame. Pure: reads the model, writes the frame.
pub fn view(model: &Model, frame: &mut Frame) {
    let [tabs, body, footer] = areas(model, frame.area());
    let language = model.settings.language;
    render_tabs(model.tab(), language, &model.theme, frame, tabs);
    match model.tab() {
        Tab::Workspaces => workspaces::render(model, frame, body),
        Tab::Updates => updates::render(model, frame, body),
        Tab::Settings => settings::render(model, frame, body),
        Tab::Log => log::render(model, frame, body),
        Tab::Ai => ai::render(model, frame, body),
        Tab::Overview => overview::render(model, frame, body),
    }
    render_footer(model, frame, footer);
    if model.help {
        help::render(model, frame, body);
    }
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

pub(crate) fn tab_message(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "tab-overview",
        Tab::Workspaces => "tab-workspaces",
        Tab::Updates => "tab-updates",
        Tab::Ai => "tab-ai",
        Tab::Log => "tab-log",
        Tab::Settings => "tab-settings",
    }
}

/// Index of the span under column `x` of a line drawn from column `left`.
fn span_at(line: &Line<'_>, left: u16, x: u16) -> Option<usize> {
    let mut start = left;
    for (index, span) in line.spans.iter().enumerate() {
        let end = start.saturating_add(u16::try_from(span.width()).unwrap_or(u16::MAX));
        if (start..end).contains(&x) {
            return Some(index);
        }
        start = end;
    }
    None
}

/// What a click at `(x, y)` hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// A tab in the tab bar.
    Tab(Tab),
    /// A key hint in the footer, by its key's message id.
    Key(&'static str),
    /// A period in the AI tab.
    Period(hud_domain::usage::Period),
    /// A selectable row of the active tab.
    Row(usize),
}

/// Resolves a click against the layout `view` draws for `full`.
pub(crate) fn hit(model: &Model, full: Rect, x: u16, y: u16) -> Option<Hit> {
    let [tabs, body, footer] = areas(model, full);
    if y == tabs.y {
        let line = tab_line(model.tab(), model.settings.language, &model.theme);
        return span_at(&line, tabs.x, x).map(|index| Hit::Tab(Tab::ALL[index / 2]));
    }
    let hints_y = footer.y + 1 + u16::from(busy(model));
    if y == hints_y {
        let hints = footer_hints(model);
        let line = hint_line(model, &hints);
        return span_at(&line, footer.x, x).map(|index| Hit::Key(hints[index / 2].0));
    }
    if !(body.y..body.y + body.height).contains(&y) {
        return None;
    }
    let row = y - body.y;
    match model.tab() {
        Tab::Ai if row == 0 => span_at(&ai::period_bar(model), body.x, x)
            .map(|index| Hit::Period(hud_domain::usage::Period::ALL[index / 2])),
        Tab::Workspaces => workspaces::row_at(model, body, row).map(Hit::Row),
        Tab::Updates => updates::row_at(model, row).map(Hit::Row),
        Tab::Settings => settings::row_at(model, row).map(Hit::Row),
        Tab::Log => log::row_at(model, body, row).map(Hit::Row),
        Tab::Ai | Tab::Overview => None,
    }
}

fn tab_line(active: Tab, language: Language, theme: &Theme) -> Line<'static> {
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
    Line::from(spans)
}

fn render_tabs(active: Tab, language: Language, theme: &Theme, frame: &mut Frame, area: Rect) {
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(
        Paragraph::new(tab_line(active, language, theme)).block(block),
        area,
    );
}

/// Key and label per hint; each pair is one click target.
fn hint_line(model: &Model, hints: &[(&'static str, &'static str)]) -> Line<'static> {
    let language = model.settings.language;
    let theme = &model.theme;
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
    Line::from(spans)
}

fn render_footer(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let hints = hint_line(model, &footer_hints(model));
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
    lines.push(hints);
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
        Tab::Ai => {
            hints.push(("key-left-right", "hint-period"));
            let can_enable = model.telemetry.as_ref().is_some_and(|status| {
                status.claude == hud_application::ports::telemetry::ClaudeTelemetry::Disabled
            });
            if can_enable {
                hints.push(("key-t", "telemetry-enable"));
            }
        }
        Tab::Overview => {}
    }
    let refresh = if model.tab() == Tab::Settings {
        "hint-reload"
    } else {
        "hint-refresh"
    };
    hints.extend([
        ("key-r", refresh),
        ("key-tab", "hint-switch"),
        ("help-key-help", "hint-help"),
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

    fn sample_kpis() -> hud_application::usecases::usage::PeriodKpis {
        use hud_domain::usage::ModelId;
        let id = |text: &str| ModelId::try_from(text.to_owned()).expect("BUG: model id");
        hud_application::usecases::usage::PeriodKpis {
            cache_hit: Some(968),
            models: vec![
                (id("claude-opus-5-5"), 712),
                (id("claude-sonnet-5-5"), 284),
                (id("claude-haiku-4-5-20251001"), 4),
            ],
            subagents: Some(312),
            thinking: Some(371),
            activity: vec![
                0, 0, 0, 0, 0, 0, 0, 40, 120, 210, 260, 180, 90, 150, 240, 310, 280, 0, 0, 0, 0, 0,
                0, 0,
            ],
            responses: 1_751,
            tool_calls: 837,
            projects: vec![
                ("macos-hud".into(), 341),
                ("takt".into(), 142),
                ("skills-hub".into(), 71),
            ],
        }
    }

    /// A year of work: weekdays busy, weekends quiet, a holiday gap.
    fn sample_calendar() -> hud_application::usecases::usage::ActivityCalendar {
        let first_day =
            hud_application::usecases::usage::calendar_start(jiff::civil::date(2026, 10, 17));
        let days = (0..(52 * 7 + 6))
            .map(|day: u64| match (day % 7, day / 7) {
                (_, 11..=12) => 0,
                (5 | 6, _) => [0, 0, 40, 0, 120][usize::try_from(day % 5).expect("BUG")],
                _ => (day * 37 + day / 7 * 53) % 900,
            })
            .collect();
        hud_application::usecases::usage::ActivityCalendar { first_day, days }
    }

    fn sample_context() -> hud_application::usecases::usage::ContextUse {
        hud_application::usecases::usage::ContextUse {
            model: hud_domain::usage::ModelId::try_from("claude-opus-5-5".to_owned()).expect("BUG"),
            project: Some("macos-hud".into()),
            prompt: 121_000,
            window: 1_000_000,
            at: jiff::Timestamp::UNIX_EPOCH,
            idle: std::time::Duration::from_secs(360),
        }
    }

    fn sample_usage() -> hud_application::usecases::usage::UsageReport {
        use hud_application::usecases::usage::{
            ModelUsage, PeriodUsage, SourceState, SourceUsage, UsageReport,
        };
        use hud_domain::usage::{
            Accuracy, CostEstimate, Measured, ModelId, PRICES_AS_OF, Period, Provider, TokenUsage,
        };
        let tokens = |input, output, write, read| TokenUsage {
            input,
            output,
            cache_write_5m: 0,
            cache_write_1h: write,
            cache_read: read,
            reasoning: None,
        };
        let cost = |micro_usd| CostEstimate {
            micro_usd,
            prices_as_of: PRICES_AS_OF,
        };
        let model = |id: &str, responses, sessions, tokens, micro: Option<u64>| ModelUsage {
            provider: Provider::Claude,
            model: ModelId::try_from(id.to_owned()).expect("BUG: model id"),
            responses,
            sessions,
            tokens,
            cost: micro.map(cost),
        };
        let periods = Period::ALL
            .iter()
            .map(|&period| PeriodUsage {
                period,
                models: vec![
                    model(
                        "claude-opus-5-5",
                        1_537,
                        4,
                        tokens(3_120, 212_400, 12_400_000, 180_200_000),
                        Some(137_480_000),
                    ),
                    model(
                        "claude-haiku-4-5-20251001",
                        214,
                        3,
                        tokens(96_000, 18_300, 0, 0),
                        Some(187_500),
                    ),
                ],
                sessions: Measured {
                    value: 5,
                    accuracy: Accuracy::Exact,
                },
                tokens: Measured {
                    value: tokens(99_120, 230_700, 12_400_000, 180_200_000),
                    accuracy: Accuracy::Exact,
                },
                cost: Measured {
                    value: cost(137_667_500),
                    accuracy: Accuracy::Estimated,
                },
                kpis: sample_kpis(),
                telemetry: Measured {
                    value: hud_domain::usage::TelemetryTotals {
                        cost_micro_usd: 131_920_000,
                        active_user_ms: 34 * 60_000,
                        active_cli_ms: 100 * 60_000,
                        lines_added: 1_240,
                        lines_removed: 310,
                        commits: 3,
                        pull_requests: 1,
                        sessions: 5,
                    },
                    accuracy: Accuracy::Partial,
                },
            })
            .collect();
        UsageReport {
            sources: vec![
                SourceUsage {
                    state: SourceState {
                        provider: Provider::Claude,
                        error: None,
                        unreadable: 0,
                        files: 474,
                        read_ms: 239,
                    },
                    periods,
                    context: Some(sample_context()),
                    plan: None,
                },
                sample_codex(),
            ],
            calendar: sample_calendar(),
            at: sample_now(),
        }
    }

    fn sample_now() -> jiff::Timestamp {
        "2026-10-17T12:00:00Z".parse().expect("BUG: timestamp")
    }

    /// Codex with reasoning tokens and a plan that is used a little.
    fn sample_codex() -> hud_application::usecases::usage::SourceUsage {
        use hud_application::usecases::usage::{
            ModelUsage, PeriodKpis, PeriodUsage, SourceState, SourceUsage,
        };
        use hud_domain::usage::{
            Accuracy, CostEstimate, Measured, ModelId, PRICES_AS_OF, Period, PlanUsage, PlanWindow,
            Provider, TokenUsage,
        };
        let luna = ModelId::try_from("gpt-6-luna".to_owned()).expect("BUG: model id");
        let tokens = TokenUsage {
            input: 1_200_000,
            output: 40_000,
            cache_write_5m: 0,
            cache_write_1h: 0,
            cache_read: 8_800_000,
            reasoning: Some(25_000),
        };
        let cost = CostEstimate {
            micro_usd: 240_500,
            prices_as_of: PRICES_AS_OF,
        };
        let periods = Period::ALL
            .iter()
            .map(|&period| PeriodUsage {
                period,
                models: vec![ModelUsage {
                    provider: Provider::Codex,
                    model: luna.clone(),
                    responses: 212,
                    sessions: 3,
                    tokens,
                    cost: Some(cost),
                }],
                sessions: Measured {
                    value: 3,
                    accuracy: Accuracy::Exact,
                },
                tokens: Measured {
                    value: tokens,
                    accuracy: Accuracy::Exact,
                },
                cost: Measured {
                    value: cost,
                    accuracy: Accuracy::Estimated,
                },
                telemetry: Measured {
                    value: hud_domain::usage::TelemetryTotals::default(),
                    accuracy: Accuracy::Partial,
                },
                kpis: PeriodKpis {
                    cache_hit: Some(880),
                    models: vec![(luna.clone(), 1000)],
                    subagents: Some(0),
                    thinking: Some(385),
                    activity: vec![
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 12, 30, 41, 8, 0, 25, 40, 36, 20, 0, 0, 0, 0, 0,
                        0,
                    ],
                    responses: 212,
                    tool_calls: 96,
                    projects: vec![("macos-hud".into(), 1000)],
                },
            })
            .collect();
        let now = sample_now();
        let later = |minutes: i64| {
            now.checked_add(jiff::SignedDuration::from_mins(minutes))
                .expect("BUG: timestamp")
        };
        SourceUsage {
            state: SourceState {
                provider: Provider::Codex,
                error: None,
                unreadable: 0,
                files: 165,
                read_ms: 84,
            },
            periods,
            context: Some(hud_application::usecases::usage::ContextUse {
                model: luna,
                project: Some("macos-hud".into()),
                prompt: 64_000,
                window: 258_400,
                at: later(-20),
                idle: std::time::Duration::from_mins(20),
            }),
            plan: Some(PlanUsage {
                at: later(-3),
                plan: Some("plus".into()),
                primary: Some(PlanWindow {
                    minutes: Some(300),
                    used_permille: 125,
                    resets_at: later(134),
                }),
                secondary: Some(PlanWindow {
                    minutes: Some(10_080),
                    used_permille: 30,
                    resets_at: later(4 * 1440),
                }),
            }),
        }
    }

    fn ai_tab(language: Language) -> Model {
        use hud_application::AppEvent;
        let mut model = Model::new(UiSettings {
            language,
            ..UiSettings::default()
        });
        model.tab = Tab::Ai;
        update(
            &mut model,
            Msg::App(AppEvent::Usage(Box::new(sample_usage()))),
        );
        model
    }

    fn render_wide(model: &Model) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(115, 22)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(model, frame))
            .expect("BUG: test draw");
        terminal
    }

    #[test]
    fn ai_tab_in_german() {
        insta::assert_snapshot!(render_wide(&ai_tab(Language::De)).backend());
    }

    #[test]
    fn telemetry_dialog_in_german() {
        use hud_application::AppEvent;
        use hud_application::ports::telemetry::{
            ClaudeTelemetry, ReceiverState, telemetry_entries,
        };
        use hud_application::usecases::telemetry::TelemetryStatus;
        let mut model = ai_tab(Language::De);
        let status = TelemetryStatus {
            receiver: ReceiverState::Listening(4318),
            claude: ClaudeTelemetry::Disabled,
            entries: telemetry_entries(4318)
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        };
        update(&mut model, Msg::App(AppEvent::Telemetry(Box::new(status))));
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render_wide(&model).backend());
    }

    #[test]
    fn ai_tab_in_english() {
        insta::assert_snapshot!(render_wide(&ai_tab(Language::En)).backend());
    }

    fn codex_only(language: Language) -> Terminal<TestBackend> {
        use hud_application::AppEvent;
        let mut model = Model::new(UiSettings {
            language,
            ..UiSettings::default()
        });
        model.tab = Tab::Ai;
        let mut report = sample_usage();
        report.sources.remove(0);
        update(&mut model, Msg::App(AppEvent::Usage(Box::new(report))));
        let mut terminal = Terminal::new(TestBackend::new(115, 36)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(&model, frame))
            .expect("BUG: test draw");
        terminal
    }

    #[test]
    fn codex_section_with_plan_usage_in_german() {
        insta::assert_snapshot!(codex_only(Language::De).backend());
    }

    #[test]
    fn codex_section_with_plan_usage_in_english() {
        insta::assert_snapshot!(codex_only(Language::En).backend());
    }

    #[test]
    fn help_overlay_on_the_log_tab_in_german() {
        let mut model = german();
        model.tab = Tab::Log;
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render_wide(&model).backend());
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

    fn loaded_overview(repos: Vec<hud_application::usecases::workspaces::RepoReport>) -> Model {
        use hud_application::ports::system::SystemSample;
        use hud_application::usecases::workspaces::WorkspaceReport;
        use hud_application::{AppEvent, ReportSource};
        let mut model = german();
        update(&mut model, Msg::Tick(std::time::Instant::now()));
        for step in 0..40u8 {
            let sample = SystemSample {
                cpu_percent: (step % 9) * 7 + 5,
                memory_used: 10 * 1024 * 1024 * 1024 + u64::from(step) * 20_000_000,
                memory_total: 16 * 1024 * 1024 * 1024,
                processes: 512,
            };
            update(&mut model, Msg::App(AppEvent::System(sample)));
        }
        let workspace = WorkspaceReport {
            workspace: hud_domain::WorkspacePath::try_from(std::path::PathBuf::from("/ws"))
                .expect("BUG: absolute"),
            auto_fast_forward: true,
            repos: Some(repos),
            unavailable: None,
        };
        update(
            &mut model,
            Msg::App(AppEvent::Workspaces {
                reports: vec![workspace],
                source: ReportSource::Sync,
            }),
        );
        model
    }

    fn repo(
        path: &str,
        state: hud_application::usecases::workspaces::RepoState,
    ) -> hud_application::usecases::workspaces::RepoReport {
        hud_application::usecases::workspaces::RepoReport {
            repo: hud_domain::git::RepoPath::try_from(std::path::PathBuf::from(path)).expect("BUG"),
            status: None,
            state,
        }
    }

    #[test]
    fn overview_lists_only_deviations() {
        use hud_application::usecases::workspaces::RepoState;
        use hud_domain::git::SkipReason;
        let model = loaded_overview(vec![
            repo("/ws/a", RepoState::Behind(2)),
            repo("/ws/b", RepoState::Skipped(SkipReason::TrackedChanges)),
            repo(
                "/ws/c",
                RepoState::Skipped(SkipReason::Diverged {
                    ahead: 2,
                    behind: 5,
                }),
            ),
            repo("/ws/d", RepoState::UpToDate),
        ]);
        insta::assert_snapshot!(render_wide(&model).backend());
    }

    #[test]
    fn overview_shows_ai_and_git_activity_calendars() {
        use hud_application::AppEvent;
        use hud_application::usecases::workspaces::RepoState;
        let mut model = loaded_overview(vec![repo("/ws/a", RepoState::UpToDate)]);
        let mut report = sample_usage();
        report.calendar = sample_calendar();
        update(&mut model, Msg::App(AppEvent::Usage(Box::new(report))));
        let git = hud_domain::activity::ActivityCalendar {
            first_day: hud_domain::activity::calendar_start(jiff::civil::date(2026, 10, 17)),
            days: (0..(52 * 7 + 6)).map(|day: u64| day % 3).collect(),
        };
        update(&mut model, Msg::App(AppEvent::GitActivity(git)));
        let mut terminal = Terminal::new(TestBackend::new(115, 40)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(&model, frame))
            .expect("BUG: test draw");
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn overview_is_calm_when_nothing_deviates() {
        use hud_application::usecases::workspaces::RepoState;
        let model = loaded_overview(vec![repo("/ws/a", RepoState::UpToDate)]);
        insta::assert_snapshot!(render_wide(&model).backend());
    }

    fn missing_tools(language: Language) -> Terminal<TestBackend> {
        use hud_application::AppEvent;
        use hud_application::ports::updates::Availability;
        use hud_application::usecases::updates::SourceReport;
        use hud_domain::update::SourceId;
        let mut model = Model::new(UiSettings {
            language,
            ..UiSettings::default()
        });
        model.tab = Tab::Updates;
        let missing = |source| SourceReport {
            source,
            availability: Availability::Missing,
            check: None,
        };
        update(
            &mut model,
            Msg::App(AppEvent::Updates(vec![
                missing(SourceId::Homebrew),
                missing(SourceId::Skills),
            ])),
        );
        let mut terminal = Terminal::new(TestBackend::new(115, 14)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(&model, frame))
            .expect("BUG: test draw");
        terminal
    }

    #[test]
    fn a_missing_tool_says_how_to_fix_it_in_german() {
        insta::assert_snapshot!(missing_tools(Language::De).backend());
    }

    #[test]
    fn a_missing_tool_says_how_to_fix_it_in_english() {
        insta::assert_snapshot!(missing_tools(Language::En).backend());
    }

    #[test]
    fn german_overview_uses_german_texts() {
        insta::assert_snapshot!(render(&german()).backend());
    }
}
