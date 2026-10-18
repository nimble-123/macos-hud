//! AI tab: sessions, tokens and estimated costs per period and model.

use hud_application::ports::telemetry::{ClaudeTelemetry, ReceiverState};
use hud_application::ports::usage::UsageError;
use hud_application::settings::Billing;
use hud_application::usecases::telemetry::TelemetryStatus;
use hud_application::usecases::usage::{ModelUsage, PeriodUsage, SourceState};
use hud_domain::usage::{CostEstimate, Period, Provider, TelemetryTotals};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, source_glyph, status_glyph};
use crate::model::TelemetryFlow;
use crate::settings::Language;

const LABEL_WIDTH: usize = 20;
/// The model name column grows with the window between these widths.
const MIN_MODEL_WIDTH: usize = 16;
const MAX_MODEL_WIDTH: usize = 40;
const NUMBER_WIDTH: usize = 13;

/// Period bar (two lines, fixed) and the scrolling area below it.
pub(super) fn split(area: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area)
}

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    // The period applies to the whole tab, so it stays on top while the rest scrolls.
    let [bar, rest] = split(area);
    frame.render_widget(Paragraph::new(period_bar(model)), bar);
    super::render_scrolled(
        model,
        frame,
        rest,
        lines(model, rest.width),
        model.ai_scroll,
    );
    if model.telemetry_flow == TelemetryFlow::Confirming
        && let Some(status) = &model.telemetry
    {
        render_telemetry_dialog(model, status, frame, area);
    }
}

/// Everything below the period bar for an area `width` wide: one block per source.
pub(super) fn lines<'a>(model: &Model, width: u16) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut lines = Vec::new();
    let Some(report) = &model.usage else {
        lines.push(Line::styled(
            t(language, "ai-loading"),
            Style::new().fg(theme.soft),
        ));
        return lines;
    };
    for (index, source) in report.sources.iter().enumerate() {
        let state = &source.state;
        if index > 0 {
            lines.push(Line::raw(""));
            lines.push(Line::raw(""));
        }
        lines.push(source_heading(model, state));
        if state.provider == Provider::Claude {
            lines.extend(telemetry_lines(model));
        }
        if let Some(error) = &state.error {
            lines.push(source_error(model, error));
            continue;
        }
        if let Some(usage) = source.period(model.ai_period) {
            lines.extend(super::ai_kpis::lines(model, source, usage, report, width));
        }
        if state.unreadable > 0 {
            lines.push(Line::raw(""));
            let count = u32::try_from(state.unreadable).unwrap_or(u32::MAX);
            lines.push(Line::styled(
                format!("~ {}", t_count(language, "ai-unreadable", "count", count)),
                Style::new().fg(theme.status(Status::Caution)),
            ));
        }
    }
    if report.sources.is_empty() {
        lines.push(Line::styled(
            t(language, "ai-no-sources"),
            Style::new().fg(theme.soft),
        ));
    }
    lines
}

fn period_key(period: Period) -> &'static str {
    match period {
        Period::Today => "period-today",
        Period::Last7Days => "period-7-days",
        Period::Last30Days => "period-30-days",
        Period::ThisMonth => "period-this-month",
        Period::LastMonth => "period-last-month",
    }
}

/// The five periods, the active one in focus.
pub(super) fn period_bar<'a>(model: &Model) -> Line<'a> {
    let theme = &model.theme;
    let mut spans = Vec::with_capacity(Period::ALL.len() * 2);
    for period in Period::ALL {
        let style = if period == model.ai_period {
            Style::new()
                .fg(theme.focus)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(theme.muted)
        };
        spans.push(Span::styled(
            t(model.settings.language, period_key(period)),
            style,
        ));
        spans.push(Span::raw("   "));
    }
    Line::from(spans)
}

fn source_heading<'a>(model: &Model, source: &SourceState) -> Line<'a> {
    let theme = &model.theme;
    let icon = source_glyph(model.settings.icons, Source::Ai)
        .map(|glyph| format!("{glyph} "))
        .unwrap_or_default();
    let name = match source.provider {
        Provider::Claude => "ai-claude",
        Provider::Codex => "ai-codex",
    };
    Line::from(vec![
        Span::styled(
            format!("{icon}{}", t(model.settings.language, name)),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "   {}   {}",
                t_args(
                    model.settings.language,
                    "ai-files-read",
                    &[
                        ("files", &source.files.to_string()),
                        ("ms", &source.read_ms.to_string())
                    ]
                ),
                t_args(
                    model.settings.language,
                    "ai-prices-as-of",
                    &[("date", &hud_domain::usage::PRICES_AS_OF.to_string())]
                )
            ),
            Style::new().fg(theme.soft),
        ),
    ])
}

fn source_error<'a>(model: &Model, error: &UsageError) -> Line<'a> {
    let language = model.settings.language;
    let (status, text) = match error {
        UsageError::NoLogs(path) => (
            Status::Neutral,
            t_args(language, "ai-no-logs", &[("path", path)]),
        ),
        UsageError::Io(message) => (
            Status::Warning,
            t_args(language, "ai-read-failed", &[("message", message)]),
        ),
    };
    Line::styled(
        format!("  {} {text}", status_glyph(model.settings.icons, status)),
        Style::new().fg(model.theme.status(status)),
    )
}

/// Telemetry rows appear once Claude Code sends, or data from earlier exists.
pub(super) fn shows_telemetry(model: &Model, usage: &PeriodUsage) -> bool {
    let sending = model
        .telemetry
        .as_ref()
        .is_some_and(|status| status.claude == ClaudeTelemetry::Enabled);
    sending || usage.telemetry.value != TelemetryTotals::default()
}

pub(super) fn telemetry_totals<'a>(model: &Model, usage: &PeriodUsage) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let totals = usage.telemetry.value;
    let label = |key: &str| super::ai_kpis::label(model, key);
    let partial = |text: String| Span::styled(format!("~ {text}"), Style::new().fg(theme.ink));
    let muted = |text: String| Span::styled(text, Style::new().fg(theme.muted));
    let active = totals.active_user_ms + totals.active_cli_ms;
    vec![
        Line::from(vec![
            label("ai-active-time"),
            partial(duration(active)),
            muted(format!(
                "   {} {}   {} {}",
                t(language, "ai-active-user"),
                duration(totals.active_user_ms),
                t(language, "ai-active-cli"),
                duration(totals.active_cli_ms),
            )),
        ]),
        Line::from(vec![
            label("ai-code"),
            partial(t_args(
                language,
                "ai-lines",
                &[
                    ("added", &count(totals.lines_added, language)),
                    ("removed", &count(totals.lines_removed, language)),
                ],
            )),
            muted(format!(
                "   {}   {}",
                t_count(
                    language,
                    "ai-commits",
                    "count",
                    u32::try_from(totals.commits).unwrap_or(u32::MAX)
                ),
                t_count(
                    language,
                    "ai-pull-requests",
                    "count",
                    u32::try_from(totals.pull_requests).unwrap_or(u32::MAX)
                ),
            )),
        ]),
        Line::from(vec![
            label("ai-cost-claude"),
            partial(money(
                CostEstimate {
                    micro_usd: totals.cost_micro_usd,
                    prices_as_of: hud_domain::usage::PRICES_AS_OF,
                },
                language,
            )),
        ]),
    ]
}

/// `2 h 14 min`, `14 min` or `45 s`.
fn duration(ms: u64) -> String {
    let seconds = ms / 1_000;
    let (hours, minutes) = (seconds / 3_600, seconds % 3_600 / 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds} s"),
        (0, m) => format!("{m} min"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// A number column of the model table.
struct Column<'m> {
    header: String,
    width: usize,
    value: Box<dyn Fn(&ModelUsage) -> String + 'm>,
}

/// Models with their numbers; narrow windows drop the less important
/// columns first, wide ones give the model name more room.
pub(super) fn model_table<'a>(
    model: &Model,
    usage: &PeriodUsage,
    provider: Provider,
    width: u16,
) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let rows: Vec<&ModelUsage> = usage
        .models
        .iter()
        .filter(|m| m.provider == provider)
        .collect();
    if rows.is_empty() {
        return vec![Line::styled(
            format!("  {}", t(language, "ai-no-usage")),
            Style::new().fg(theme.muted),
        )];
    }
    // Codex hardly writes to the cache but reports reasoning separately.
    let (fourth, fourth_value): (&str, fn(&hud_domain::usage::TokenUsage) -> u64) = match provider {
        Provider::Claude => ("ai-cache-write", |t| t.cache_write_5m + t.cache_write_1h),
        Provider::Codex => ("ai-reasoning", |t| t.reasoning.unwrap_or(0)),
    };
    let column = |key: &str, width: usize, value: Box<dyn Fn(&ModelUsage) -> String>| Column {
        header: t(language, key),
        width,
        value,
    };
    let mut columns = vec![
        column(
            "ai-responses",
            NUMBER_WIDTH,
            Box::new(move |row| count(row.responses, language)),
        ),
        column(
            "ai-input",
            NUMBER_WIDTH,
            Box::new(move |row| compact(row.tokens.input, language)),
        ),
        column(
            "ai-output",
            NUMBER_WIDTH,
            Box::new(move |row| compact(row.tokens.output, language)),
        ),
        column(
            fourth,
            NUMBER_WIDTH,
            Box::new(move |row| compact(fourth_value(&row.tokens), language)),
        ),
        column(
            "ai-cache-read",
            NUMBER_WIDTH,
            Box::new(move |row| compact(row.tokens.cache_read, language)),
        ),
        column(
            "ai-cost",
            NUMBER_WIDTH + 2,
            Box::new(move |row| match row.cost {
                Some(cost) => format!("≈ {}", money(cost, language)),
                None => "—".to_owned(),
            }),
        ),
    ];
    let available = usize::from(width).saturating_sub(2);
    let numbers = |columns: &[Column<'_>]| columns.iter().map(|c| c.width).sum::<usize>();
    // Output and cost always stay; the others go in this order.
    for header in [
        t(language, fourth),
        t(language, "ai-responses"),
        t(language, "ai-input"),
        t(language, "ai-cache-read"),
    ] {
        if MIN_MODEL_WIDTH + numbers(&columns) <= available {
            break;
        }
        columns.retain(|column| column.header != header);
    }
    let name_width = available
        .saturating_sub(numbers(&columns))
        .clamp(MIN_MODEL_WIDTH, MAX_MODEL_WIDTH);
    let mut header = format!("  {}", fit(&t(language, "ai-model"), name_width));
    for column in &columns {
        header.push_str(&right(&column.header, column.width));
    }
    let mut lines = vec![Line::styled(header, Style::new().fg(theme.soft))];
    for row in rows {
        let (numbers, cost) = columns.split_at(columns.len() - 1);
        let figures: String = numbers
            .iter()
            .map(|column| right(&(column.value)(row), column.width))
            .collect();
        let cost = &cost[0];
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", fit(row.model.as_str(), name_width)),
                Style::new().fg(theme.ink),
            ),
            Span::styled(figures, Style::new().fg(theme.muted)),
            Span::styled(
                right(&(cost.value)(row), cost.width),
                Style::new().fg(theme.ink),
            ),
        ]));
    }
    lines
}

/// Subscription costs are what the same usage would cost via the API.
pub(super) fn billing_label(model: &Model, provider: Provider) -> String {
    let billing = model
        .preferences
        .as_ref()
        .map_or(Billing::Subscription, |p| match provider {
            Provider::Claude => p.claude.billing,
            Provider::Codex => p.codex.billing,
        });
    t(
        model.settings.language,
        match billing {
            Billing::Subscription => "ai-api-equivalent",
            Billing::Api => "ai-estimate",
        },
    )
}

/// `$41.20` or `41,20 $`.
pub(crate) fn money(cost: CostEstimate, language: Language) -> String {
    let cents = (cost.micro_usd + 5_000) / 10_000;
    let (dollars, rest) = (cents / 100, cents % 100);
    match language {
        Language::De => format!("{},{rest:02} $", group(dollars, '.')),
        Language::En => format!("${}.{rest:02}", group(dollars, ',')),
    }
}

/// Thousands grouped: `19 863` style per language.
pub(super) fn count(value: u64, language: Language) -> String {
    group(
        value,
        match language {
            Language::De => '.',
            Language::En => ',',
        },
    )
}

fn group(value: u64, separator: char) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// `38,2 M` or `38.2M`: one decimal, units k, M, B.
pub(crate) fn compact(value: u64, language: Language) -> String {
    let (scaled, unit) = match value {
        0..1_000 => return value.to_string(),
        1_000..1_000_000 => (value * 10 / 1_000, "k"),
        1_000_000..1_000_000_000 => (value * 10 / 1_000_000, "M"),
        _ => (value / 100_000_000, "B"),
    };
    let (whole, tenth) = (scaled / 10, scaled % 10);
    match language {
        Language::De => format!("{whole},{tenth} {}", if unit == "B" { "Mrd" } else { unit }),
        Language::En => format!("{whole}.{tenth}{unit}"),
    }
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

fn right(text: &str, width: usize) -> String {
    format!("{text:>width$}")
}

/// Receiver and Claude Code setup, with the next step if there is one.
fn telemetry_lines<'a>(model: &Model) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let Some(status) = &model.telemetry else {
        return Vec::new();
    };
    let icons = model.settings.icons;
    let (state, text) = match (&status.receiver, &status.claude) {
        (ReceiverState::Off, _) => (Status::Neutral, t(language, "telemetry-off")),
        (ReceiverState::Unavailable { port, reason }, _) => (
            Status::Warning,
            t_args(
                language,
                "telemetry-port-busy",
                &[("port", &port.to_string()), ("reason", reason)],
            ),
        ),
        (ReceiverState::Listening(port), ClaudeTelemetry::Enabled) => (
            Status::Ok,
            t_args(
                language,
                "telemetry-enabled",
                &[("port", &port.to_string())],
            ),
        ),
        (ReceiverState::Listening(_), ClaudeTelemetry::Disabled) => {
            (Status::Advisory, t(language, "telemetry-disabled"))
        }
        (ReceiverState::Listening(_), ClaudeTelemetry::Conflict(keys)) => (
            Status::Caution,
            t_args(
                language,
                "telemetry-conflict",
                &[("keys", &keys.join(", "))],
            ),
        ),
        (ReceiverState::Listening(_), ClaudeTelemetry::Unreadable(reason)) => (
            Status::Warning,
            t_args(language, "telemetry-unreadable", &[("reason", reason)]),
        ),
    };
    let mut first = vec![
        Span::styled(
            fit(&t(language, "telemetry"), LABEL_WIDTH),
            Style::new().fg(theme.soft),
        ),
        Span::styled(
            format!("{} {text}", status_glyph(icons, state)),
            Style::new().fg(theme.status(state)),
        ),
    ];
    if status.claude == ClaudeTelemetry::Disabled
        && matches!(status.receiver, ReceiverState::Listening(_))
    {
        first.push(Span::styled(
            format!("   {} ", t(language, "key-t")),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ));
        first.push(Span::styled(
            t(language, "telemetry-enable"),
            Style::new().fg(theme.soft),
        ));
    }
    let mut lines = vec![Line::from(first)];
    if model.telemetry_flow == TelemetryFlow::Requested && status.claude == ClaudeTelemetry::Enabled
    {
        lines.push(Line::styled(
            format!(
                "{}{}",
                " ".repeat(LABEL_WIDTH),
                t(language, "telemetry-restart")
            ),
            Style::new().fg(theme.soft),
        ));
    }
    lines
}

/// Shows exactly what hud adds to Claude Code's settings before it does.
fn render_telemetry_dialog(model: &Model, status: &TelemetryStatus, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let height = u16::try_from(status.entries.len()).unwrap_or(0) + 9;
    let [dialog] = Layout::horizontal([Constraint::Length(92)])
        .flex(Flex::Center)
        .areas(area);
    let [dialog] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(dialog);
    let key = Style::new().fg(theme.focus).add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  {}", t(language, "telemetry-dialog-body")),
            Style::new().fg(theme.ink),
        ),
        Line::raw(""),
    ];
    lines.extend(status.entries.iter().map(|(name, value)| {
        Line::styled(
            format!("    \"{name}\": \"{value}\""),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        )
    }));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("  {}", t(language, "telemetry-dialog-note")),
        Style::new().fg(theme.soft),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("  {} ", t(language, "key-enter")), key),
        Span::styled(
            format!("{}   ", t(language, "telemetry-dialog-write")),
            Style::new().fg(theme.soft),
        ),
        Span::styled(format!("{} ", t(language, "key-esc")), key),
        Span::styled(t(language, "confirm-cancel"), Style::new().fg(theme.soft)),
    ]));
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(" {} ", t(language, "telemetry-dialog-title")),
            key,
        ));
    frame.render_widget(Clear, dialog);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        dialog,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_numbers_follow_the_language() {
        assert_eq!(compact(999, Language::De), "999");
        assert_eq!(compact(38_240_000, Language::De), "38,2 M");
        assert_eq!(compact(38_240_000, Language::En), "38.2M");
        assert_eq!(compact(12_345, Language::En), "12.3k");
        assert_eq!(compact(2_500_000_000, Language::De), "2,5 Mrd");
    }

    #[test]
    fn money_rounds_to_cents_and_groups_thousands() {
        let cost = |micro_usd| CostEstimate {
            micro_usd,
            prices_as_of: hud_domain::usage::PRICES_AS_OF,
        };
        assert_eq!(money(cost(41_204_999), Language::De), "41,20 $");
        assert_eq!(money(cost(41_205_000), Language::En), "$41.21");
        assert_eq!(money(cost(1_234_000_000), Language::En), "$1,234.00");
        assert_eq!(money(cost(1_234_000_000), Language::De), "1.234,00 $");
    }

    #[test]
    fn durations_use_the_largest_sensible_units() {
        assert_eq!(duration(45_000), "45 s");
        assert_eq!(duration(14 * 60_000), "14 min");
        assert_eq!(duration(134 * 60_000), "2 h 14 min");
    }

    #[test]
    fn counts_are_grouped() {
        assert_eq!(count(19_863, Language::De), "19.863");
        assert_eq!(count(1_537, Language::En), "1,537");
        assert_eq!(count(12, Language::En), "12");
    }
}
