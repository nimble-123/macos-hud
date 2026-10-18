//! Log tab: actions newest first, with filter, search and a detail pane.

use hud_domain::action::{ActionKind, ActionRecord, Outcome, Trigger};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::Model;
use crate::i18n::{t, t_args};
use crate::icons::{Status, status_glyph};
use crate::log_tab::{Filter, selected, visible};

const TIME_WIDTH: usize = 18;
const KIND_WIDTH: usize = 15;
const TRIGGER_WIDTH: usize = 10;
const RESULT_WIDTH: usize = 26;
/// Lines of the detail pane, including its top rule.
const DETAIL_HEIGHT: u16 = 7;
/// Lines the list keeps when the details are expanded: header and five rows.
const EXPANDED_LIST: u16 = 6;

/// Height of the detail pane; expanded it leaves the list a few rows.
fn detail_height(model: &Model, area: Rect) -> u16 {
    if model.log_tab.expanded {
        area.height
            .saturating_sub(2 + EXPANDED_LIST)
            .max(DETAIL_HEIGHT)
    } else {
        DETAIL_HEIGHT
    }
}

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let [bar, list, detail] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(detail_height(model, area)),
    ])
    .areas(area);
    render_bar(model, frame, bar);
    let records = visible(model);
    render_list(model, &records, frame, list);
    if let Some(record) = selected(model) {
        render_detail(model, record, frame, detail);
    }
}

/// Filter and search above the list.
fn render_bar(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let tab = &model.log_tab;
    let filter = t(
        language,
        match tab.filter {
            Filter::All => "log-filter-all",
            Filter::Auto => "log-filter-auto",
            Filter::Manual => "log-filter-manual",
            Filter::Problems => "log-filter-problems",
        },
    );
    let mut spans = vec![
        Span::styled(
            format!("{} ", t(language, "log-filter")),
            Style::new().fg(theme.soft),
        ),
        Span::styled(filter, Style::new().fg(theme.ink)),
    ];
    if tab.searching || !tab.query.is_empty() {
        let query_style = if tab.searching {
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.ink)
        };
        spans.push(Span::styled(
            format!("    {} ", t(language, "log-search")),
            Style::new().fg(theme.soft),
        ));
        spans.push(Span::styled(tab.query.clone(), query_style));
        if tab.searching {
            spans.push(Span::styled("█", Style::new().fg(theme.focus)));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Log entry under line `y` of the tab's area: below the filter bar (two
/// lines) and the column header, with the same window as the list.
pub(super) fn row_at(model: &Model, area: Rect, y: u16) -> Option<usize> {
    let list_y = y.checked_sub(3)?;
    let list_height = area.height.saturating_sub(2 + detail_height(model, area));
    let rows = usize::from(list_height.saturating_sub(1));
    let start = model
        .log_tab
        .selected
        .saturating_sub(rows.saturating_sub(1));
    let index = start + usize::from(list_y);
    (usize::from(list_y) < rows && index < visible(model).len()).then_some(index)
}

fn render_list(model: &Model, records: &[&ActionRecord], frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    if records.is_empty() {
        let key = if model.actions.is_empty() {
            "log-empty"
        } else {
            "log-no-match"
        };
        frame.render_widget(
            Paragraph::new(Line::styled(t(language, key), Style::new().fg(theme.soft))),
            area,
        );
        return;
    }
    let header = Line::styled(
        format!(
            "  {}{}{}{}{}",
            fit(&t(language, "log-col-time"), TIME_WIDTH),
            fit(&t(language, "log-col-action"), KIND_WIDTH),
            fit(&t(language, "log-col-trigger"), TRIGGER_WIDTH),
            fit(&t(language, "log-col-result"), RESULT_WIDTH),
            t(language, "log-col-target"),
        ),
        Style::new().fg(theme.soft),
    );
    // Keep the cursor in view: the window ends at the selected row once it
    // would fall below the bottom edge.
    let rows = usize::from(area.height.saturating_sub(1));
    let start = model
        .log_tab
        .selected
        .saturating_sub(rows.saturating_sub(1));
    let mut lines = vec![header];
    lines.extend(
        records
            .iter()
            .enumerate()
            .skip(start)
            .take(rows)
            .map(|(index, record)| row(model, record, index == model.log_tab.selected)),
    );
    frame.render_widget(Paragraph::new(lines), area);
}

fn row<'a>(model: &Model, record: &ActionRecord, selected: bool) -> Line<'a> {
    let theme = &model.theme;
    let (status, result) = result(model, record);
    let target_style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink)
    };
    Line::from(vec![
        Span::styled(
            if selected { "▌ " } else { "  " },
            Style::new().fg(theme.focus),
        ),
        Span::styled(
            fit(&time(model, record.started_at), TIME_WIDTH),
            Style::new().fg(theme.muted),
        ),
        Span::styled(
            fit(&kind(model, record.kind), KIND_WIDTH),
            Style::new().fg(theme.ink),
        ),
        Span::styled(
            fit(&trigger(model, record.trigger), TRIGGER_WIDTH),
            Style::new().fg(theme.muted),
        ),
        Span::styled(
            fit(
                &format!("{} {result}", status_glyph(model.settings.icons, status)),
                RESULT_WIDTH,
            ),
            Style::new().fg(theme.status(status)),
        ),
        Span::styled(record.target.clone(), target_style),
    ])
}

/// Target, result, duration and the end of the output of the selected action.
fn render_detail(model: &Model, record: &ActionRecord, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme.rule));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let label = |key: &str| Span::styled(fit(&t(language, key), 14), Style::new().fg(theme.soft));
    let (status, result) = result(model, record);
    let mut lines = vec![
        Line::from(vec![
            label("log-detail-target"),
            Span::styled(record.target.clone(), Style::new().fg(theme.ink)),
        ]),
        Line::from(vec![
            label("log-col-result"),
            Span::styled(result, Style::new().fg(theme.status(status))),
            Span::styled(duration(model, record), Style::new().fg(theme.muted)),
        ]),
    ];
    if let Some(summary) = &record.summary {
        lines.push(Line::from(vec![
            label("log-detail-summary"),
            Span::styled(summary.clone(), Style::new().fg(theme.ink)),
        ]));
    }
    let output = match &record.outcome {
        Some(Outcome::Failed(text) | Outcome::Skipped(text)) if !text.is_empty() => Some(text),
        _ => None,
    };
    if let Some(output) = output {
        // Only the end fits; it is also where tools explain what went wrong.
        let room = usize::from(inner.height).saturating_sub(lines.len());
        let all: Vec<&str> = output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let tail = &all[all.len().saturating_sub(room)..];
        for (index, text) in tail.iter().enumerate() {
            let first = if index == 0 {
                label("log-detail-output")
            } else {
                Span::raw(" ".repeat(14))
            };
            lines.push(Line::from(vec![
                first,
                Span::styled((*text).to_owned(), Style::new().fg(theme.muted)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Status and short text in the UI language; summaries are technical and
/// stay in the detail pane.
fn result(model: &Model, record: &ActionRecord) -> (Status, String) {
    let language = model.settings.language;
    match &record.outcome {
        None => (Status::Advisory, t(language, "outcome-running")),
        Some(Outcome::Succeeded) => (Status::Ok, t(language, "outcome-succeeded")),
        Some(Outcome::Failed(_)) => (Status::Warning, t(language, "outcome-failed")),
        Some(Outcome::TimedOut) => (Status::Warning, t(language, "outcome-timed-out")),
        Some(Outcome::Skipped(_)) => (Status::Caution, t(language, "outcome-skipped")),
        Some(Outcome::Cancelled) => (Status::Caution, t(language, "outcome-cancelled")),
    }
}

fn duration(model: &Model, record: &ActionRecord) -> String {
    let Some(finished) = record.finished_at else {
        return String::new();
    };
    let seconds = finished.duration_since(record.started_at).as_secs();
    let text = if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min {} s", seconds / 60, seconds % 60)
    };
    format!(
        "   {}",
        t_args(
            model.settings.language,
            "log-duration",
            &[("duration", &text)]
        )
    )
}

fn time(model: &Model, at: jiff::Timestamp) -> String {
    at.to_zoned(model.time_zone.clone())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

fn kind(model: &Model, kind: ActionKind) -> String {
    t(
        model.settings.language,
        match kind {
            ActionKind::Fetch => "kind-fetch",
            ActionKind::FastForward => "kind-fast-forward",
            ActionKind::Install => "kind-install",
        },
    )
}

fn trigger(model: &Model, trigger: Trigger) -> String {
    t(
        model.settings.language,
        match trigger {
            Trigger::Auto => "trigger-auto",
            Trigger::Manual => "trigger-manual",
        },
    )
}

fn fit(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count < width {
        format!("{text}{}", " ".repeat(width - count))
    } else {
        let mut cut: String = text.chars().take(width.saturating_sub(2)).collect();
        cut.push_str("… ");
        cut
    }
}
