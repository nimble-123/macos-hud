//! AI tab: sessions, tokens and estimated costs per period and model.

use hud_application::ports::usage::UsageError;
use hud_application::settings::Billing;
use hud_application::usecases::usage::{ModelUsage, PeriodUsage, SourceState};
use hud_domain::usage::{Accuracy, CostEstimate, Measured, Period, Provider};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, source_glyph, status_glyph};
use crate::settings::Language;

const LABEL_WIDTH: usize = 16;
const MODEL_WIDTH: usize = 28;
const NUMBER_WIDTH: usize = 13;

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut lines = vec![period_bar(model), Line::raw("")];
    let Some(report) = &model.usage else {
        lines.push(Line::styled(
            t(language, "ai-loading"),
            Style::new().fg(theme.soft),
        ));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    };
    // TODO(#31): totals cover all providers; split them once Codex is read too.
    for source in &report.sources {
        lines.push(source_heading(model, source));
        if let Some(error) = &source.error {
            lines.push(source_error(model, error));
            continue;
        }
        let Some(usage) = report.periods.iter().find(|p| p.period == model.ai_period) else {
            continue;
        };
        lines.extend(totals(model, usage, source.provider));
        lines.push(Line::raw(""));
        lines.extend(model_table(model, usage, source.provider));
        if source.unreadable > 0 {
            lines.push(Line::raw(""));
            let count = u32::try_from(source.unreadable).unwrap_or(u32::MAX);
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
    frame.render_widget(Paragraph::new(lines), area);
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
fn period_bar<'a>(model: &Model) -> Line<'a> {
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
                "   {}",
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

fn totals<'a>(model: &Model, usage: &PeriodUsage, provider: Provider) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let label = |key: &str| {
        Span::styled(
            fit(&t(language, key), LABEL_WIDTH),
            Style::new().fg(theme.soft),
        )
    };
    let value = |text: String| Span::styled(text, Style::new().fg(theme.ink));
    let responses: u64 = usage
        .models
        .iter()
        .filter(|m| m.provider == provider)
        .map(|m| m.responses)
        .sum();
    let tokens = usage.tokens.value;
    vec![
        Line::from(vec![
            label("ai-sessions"),
            value(measured(usage.sessions, |n| count(n, language))),
        ]),
        Line::from(vec![
            label("ai-responses"),
            value(count(responses, language)),
        ]),
        Line::from(vec![
            label("ai-tokens"),
            value(measured(usage.tokens, |t| compact(t.total(), language))),
            Span::styled(
                format!(
                    "   {} {}   {} {}   {} {}   {} {}",
                    t(language, "ai-input"),
                    compact(tokens.input, language),
                    t(language, "ai-output"),
                    compact(tokens.output, language),
                    t(language, "ai-cache-write"),
                    compact(tokens.cache_write_5m + tokens.cache_write_1h, language),
                    t(language, "ai-cache-read"),
                    compact(tokens.cache_read, language),
                ),
                Style::new().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            label("ai-cost"),
            cost_span(model, usage.cost),
            Span::styled(
                format!("   {}", billing_label(model)),
                Style::new().fg(theme.soft),
            ),
        ]),
    ]
}

fn model_table<'a>(model: &Model, usage: &PeriodUsage, provider: Provider) -> Vec<Line<'a>> {
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
    let header = format!(
        "  {}{}{}{}{}{}{}",
        fit(&t(language, "ai-model"), MODEL_WIDTH),
        right(&t(language, "ai-responses"), NUMBER_WIDTH),
        right(&t(language, "ai-input"), NUMBER_WIDTH),
        right(&t(language, "ai-output"), NUMBER_WIDTH),
        right(&t(language, "ai-cache-write"), NUMBER_WIDTH),
        right(&t(language, "ai-cache-read"), NUMBER_WIDTH),
        right(&t(language, "ai-cost"), NUMBER_WIDTH + 2),
    );
    let mut lines = vec![Line::styled(header, Style::new().fg(theme.soft))];
    for row in rows {
        let tokens = row.tokens;
        let cost = match row.cost {
            Some(cost) => format!("≈ {}", money(cost, language)),
            None => "—".to_owned(),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", fit(row.model.as_str(), MODEL_WIDTH)),
                Style::new().fg(theme.ink),
            ),
            Span::styled(
                format!(
                    "{}{}{}{}{}",
                    right(&count(row.responses, language), NUMBER_WIDTH),
                    right(&compact(tokens.input, language), NUMBER_WIDTH),
                    right(&compact(tokens.output, language), NUMBER_WIDTH),
                    right(
                        &compact(tokens.cache_write_5m + tokens.cache_write_1h, language),
                        NUMBER_WIDTH
                    ),
                    right(&compact(tokens.cache_read, language), NUMBER_WIDTH),
                ),
                Style::new().fg(theme.muted),
            ),
            Span::styled(right(&cost, NUMBER_WIDTH + 2), Style::new().fg(theme.ink)),
        ]));
    }
    lines
}

/// `≈` for estimates, `~` for partial values, `—` for unavailable ones.
fn measured<T: Copy>(value: Measured<T>, format: impl Fn(T) -> String) -> String {
    match value.accuracy {
        Accuracy::Exact => format(value.value),
        Accuracy::Estimated => format!("≈ {}", format(value.value)),
        Accuracy::Partial => format!("~ {}", format(value.value)),
        Accuracy::Unavailable => "—".to_owned(),
    }
}

fn cost_span<'a>(model: &Model, cost: Measured<CostEstimate>) -> Span<'a> {
    let language = model.settings.language;
    let text = match cost.accuracy {
        Accuracy::Partial => format!(
            "~ {} ({})",
            money(cost.value, language),
            t(language, "ai-partial")
        ),
        _ => measured(cost, |c| money(c, language)),
    };
    Span::styled(
        text,
        Style::new()
            .fg(model.theme.ink)
            .add_modifier(Modifier::BOLD),
    )
}

/// Subscription costs are what the same usage would cost via the API.
fn billing_label(model: &Model) -> String {
    let billing = model
        .preferences
        .as_ref()
        .map_or(Billing::Subscription, |p| p.claude.billing);
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
fn count(value: u64, language: Language) -> String {
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
    fn counts_are_grouped() {
        assert_eq!(count(19_863, Language::De), "19.863");
        assert_eq!(count(1_537, Language::En), "1,537");
        assert_eq!(count(12, Language::En), "12");
    }
}
