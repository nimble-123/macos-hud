//! Key figures on top of the AI tab: fixed horizons, no switching.

use hud_application::usecases::usage::{ContextUse, PeriodKpis, PeriodUsage};
use hud_domain::usage::{Accuracy, ModelId, Provider};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::ai::{compact, count, money};
use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::Status;
use crate::settings::Language;

const LABEL_WIDTH: usize = 20;
/// Width of bars and the activity chart.
const BAR_WIDTH: usize = 60;
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const INDENT: &str = "  ";

/// All sections for the selected period; only the context is current.
pub(super) fn lines<'a>(
    model: &Model,
    usage: &PeriodUsage,
    context: Option<&ContextUse>,
) -> Vec<Line<'a>> {
    let kpis = &usage.kpis;
    let mut lines = Vec::new();
    let mut section = |heading: Line<'a>, body: Vec<Line<'a>>| {
        lines.push(Line::raw(""));
        lines.push(heading);
        lines.extend(body);
    };
    section(
        heading(model, "kpi-consumption", None),
        consumption(model, usage),
    );
    let mut models = bar_and_legend(model, kpis);
    models.push(Line::raw(""));
    models.extend(super::ai::model_table(model, usage, Provider::Claude));
    section(heading(model, "kpi-models", None), models);
    section(
        heading(model, "kpi-effort", None),
        vec![effort(model, kpis)],
    );
    section(heading(model, "kpi-activity", None), activity(model, usage));
    if let Some(context) = context {
        section(
            heading(model, "kpi-context", Some("kpi-now")),
            context_lines(model, context),
        );
    }
    if !kpis.projects.is_empty() {
        section(
            heading(model, "kpi-projects", None),
            vec![projects(model, kpis)],
        );
    }
    if super::ai::shows_telemetry(model, usage) {
        section(
            heading(model, "telemetry", Some("ai-telemetry-note")),
            super::ai::telemetry_totals(model, usage),
        );
    }
    lines
}

fn heading<'a>(model: &Model, key: &str, note: Option<&str>) -> Line<'a> {
    let language = model.settings.language;
    let mut spans = vec![Span::styled(
        t(language, key),
        Style::new()
            .fg(model.theme.ink)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(note) = note {
        spans.push(soft(model, format!("   {}", t(language, note))));
    }
    Line::from(spans)
}

pub(super) fn label<'a>(model: &Model, key: &str) -> Span<'a> {
    Span::styled(
        format!(
            "{INDENT}{}",
            fit(&t(model.settings.language, key), LABEL_WIDTH)
        ),
        Style::new().fg(model.theme.soft),
    )
}

fn soft<'a>(model: &Model, text: String) -> Span<'a> {
    Span::styled(text, Style::new().fg(model.theme.soft))
}

fn strong<'a>(model: &Model, text: String) -> Span<'a> {
    Span::styled(
        text,
        Style::new()
            .fg(model.theme.ink)
            .add_modifier(Modifier::BOLD),
    )
}

fn marker(accuracy: Accuracy) -> &'static str {
    match accuracy {
        Accuracy::Exact => "",
        Accuracy::Estimated => "≈ ",
        Accuracy::Partial => "~ ",
        Accuracy::Unavailable => "— ",
    }
}

fn consumption<'a>(model: &Model, usage: &PeriodUsage) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let tokens = usage.tokens.value;
    let mut token_line = vec![
        label(model, "ai-tokens"),
        strong(
            model,
            format!(
                "{}{}",
                marker(usage.tokens.accuracy),
                compact(tokens.total(), language)
            ),
        ),
    ];
    for (key, value) in [
        ("ai-input", tokens.input),
        ("ai-output", tokens.output),
        (
            "ai-cache-write",
            tokens.cache_write_5m + tokens.cache_write_1h,
        ),
        ("ai-cache-read", tokens.cache_read),
    ] {
        token_line.push(soft(model, format!("   {} ", t(language, key))));
        token_line.push(Span::styled(
            compact(value, language),
            Style::new().fg(model.theme.ink),
        ));
    }
    let cost_marker = match usage.cost.accuracy {
        Accuracy::Partial => "~ ",
        _ => "≈ ",
    };
    let cost_line = vec![
        label(model, "ai-cost"),
        strong(
            model,
            format!("{cost_marker}{}", money(usage.cost.value, language)),
        ),
        soft(model, format!("      {}", super::ai::billing_label(model))),
    ];
    let mut hit_line = vec![label(model, "kpi-cache-hit")];
    match usage.kpis.cache_hit {
        Some(hit) => hit_line.push(Span::styled(
            percent(hit),
            Style::new()
                .fg(model.theme.status(Status::Advisory))
                .add_modifier(Modifier::BOLD),
        )),
        None => hit_line.push(soft(model, "—".to_owned())),
    }
    vec![
        Line::from(token_line),
        Line::from(cost_line),
        Line::from(hit_line),
    ]
}

/// Colours for the stacked bar: three distinct ones, then muted.
fn palette(model: &Model, index: usize) -> Color {
    let theme = &model.theme;
    match index {
        0 => theme.status(Status::Advisory),
        1 => theme.focus,
        2 => theme.status(Status::Caution),
        _ => theme.muted,
    }
}

fn bar_and_legend<'a>(model: &Model, kpis: &PeriodKpis) -> Vec<Line<'a>> {
    if kpis.models.is_empty() {
        return Vec::new();
    }
    let mut bar = vec![Span::raw(INDENT)];
    let mut used = 0;
    for (index, (_, share)) in kpis.models.iter().enumerate() {
        let width = ((usize::from(*share) * BAR_WIDTH + 500) / 1000).min(BAR_WIDTH - used);
        used += width;
        bar.push(Span::styled(
            "█".repeat(width),
            Style::new().fg(palette(model, index)),
        ));
    }
    bar.push(Span::styled(
        "░".repeat(BAR_WIDTH - used),
        Style::new().fg(model.theme.rule),
    ));
    let mut legend = vec![Span::raw(INDENT)];
    for (index, (id, share)) in kpis.models.iter().take(4).enumerate() {
        let gap = if index == 0 { "" } else { "      " };
        legend.push(Span::styled(
            format!("{gap}■ "),
            Style::new().fg(palette(model, index)),
        ));
        legend.push(soft(model, format!("{} ", family(id))));
        legend.push(strong(model, percent(*share)));
    }
    vec![Line::from(bar), Line::from(legend)]
}

fn effort<'a>(model: &Model, kpis: &PeriodKpis) -> Line<'a> {
    let language = model.settings.language;
    let value = |share: Option<u16>| share.map_or_else(|| "—".to_owned(), percent);
    Line::from(vec![
        label(model, "kpi-subagents"),
        strong(model, value(kpis.subagents)),
        soft(model, format!(" {}", t(language, "kpi-of-fresh"))),
        soft(model, format!("      {} ", t(language, "kpi-thinking"))),
        strong(model, value(kpis.thinking)),
        soft(model, format!(" {}", t(language, "kpi-of-output"))),
    ])
}

fn activity<'a>(model: &Model, usage: &PeriodUsage) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let kpis = &usage.kpis;
    let chart = Line::from(vec![
        Span::raw(INDENT),
        Span::styled(
            spark(&kpis.activity),
            Style::new().fg(model.theme.status(Status::Advisory)),
        ),
    ]);
    let totals = Line::from(vec![
        label(model, "kpi-totals"),
        strong(
            model,
            counted(language, "kpi-sessions", usage.sessions.value),
        ),
        soft(model, ", ".to_owned()),
        strong(model, counted(language, "kpi-responses", kpis.responses)),
        soft(model, ", ".to_owned()),
        strong(model, counted(language, "kpi-tool-calls", kpis.tool_calls)),
    ]);
    vec![chart, axis(model, usage), totals]
}

/// Start and end of the chart under its left and right edge.
fn axis<'a>(model: &Model, usage: &PeriodUsage) -> Line<'a> {
    let language = model.settings.language;
    let buckets = usage.kpis.activity.len();
    let width = (BAR_WIDTH / buckets.max(1)).max(1) * buckets;
    let (left, right) = match usage.period {
        hud_domain::usage::Period::Today => (
            t(language, "kpi-axis-midnight"),
            t(language, "kpi-axis-day-end"),
        ),
        hud_domain::usage::Period::LastMonth => (
            t(language, "kpi-axis-month-start"),
            t(language, "kpi-axis-month-end"),
        ),
        _ => (
            t_args(
                language,
                "kpi-days-ago",
                &[("count", &buckets.saturating_sub(1).to_string())],
            ),
            t(language, "kpi-today"),
        ),
    };
    let gap = width.saturating_sub(left.chars().count() + right.chars().count());
    Line::from(vec![
        Span::raw(INDENT),
        Span::styled(
            format!("{left}{}{right}", " ".repeat(gap)),
            Style::new().fg(model.theme.muted),
        ),
    ])
}

fn context_lines<'a>(model: &Model, context: &ContextUse) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let share =
        u16::try_from((u128::from(context.prompt) * 1000) / u128::from(context.window.max(1)))
            .unwrap_or(1000)
            .min(1000);
    let filled = usize::from(share) * BAR_WIDTH / 1000;
    let color = if share >= 800 {
        model.theme.status(Status::Caution)
    } else {
        model.theme.status(Status::Advisory)
    };
    let bar = Line::from(vec![
        Span::raw(INDENT),
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled(
            "░".repeat(BAR_WIDTH - filled),
            Style::new().fg(model.theme.rule),
        ),
        Span::styled(
            format!("   {}", percent(share)),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ),
        soft(
            model,
            format!(
                "   {} / {}",
                compact(context.prompt, language),
                compact(context.window, language)
            ),
        ),
    ]);
    let mut meta = vec![
        context
            .model
            .as_str()
            .trim_start_matches("claude-")
            .to_owned(),
    ];
    meta.extend(context.project.clone());
    meta.push(ago(context.idle.as_secs(), language));
    vec![
        bar,
        Line::from(vec![Span::raw(INDENT), soft(model, meta.join(", "))]),
    ]
}

fn projects<'a>(model: &Model, kpis: &PeriodKpis) -> Line<'a> {
    let mut spans = vec![Span::raw(INDENT)];
    for (index, (name, share)) in kpis.projects.iter().take(5).enumerate() {
        let gap = if index == 0 { "" } else { "      " };
        spans.push(soft(model, format!("{gap}{name} ")));
        spans.push(strong(model, percent(*share)));
    }
    Line::from(spans)
}

/// `Opus 5.5` from `claude-opus-5-5`, `Haiku 4.5` from `claude-haiku-4-5-20251001`.
fn family(model: &ModelId) -> String {
    let mut parts = model.as_str().trim_start_matches("claude-").split('-');
    let name = parts.next().unwrap_or_default();
    let mut title = name.chars();
    let name = title
        .next()
        .map(|first| first.to_uppercase().chain(title).collect::<String>())
        .unwrap_or_default();
    // A date suffix has eight digits; version parts are short.
    let version: Vec<&str> = parts.filter(|part| part.len() < 4).collect();
    if version.is_empty() {
        name
    } else {
        format!("{name} {}", version.join("."))
    }
}

/// A counted phrase with grouped digits: plural forms only differ below 1 000,
/// so larger numbers are passed as already formatted text.
fn counted(language: Language, key: &str, value: u64) -> String {
    match u32::try_from(value) {
        Ok(small) if small < 1_000 => t_count(language, key, "count", small),
        _ => t_args(language, key, &[("count", &count(value, language))]),
    }
}

/// `31 %`, or `<1 %` for anything above zero but below one percent.
fn percent(permille: u16) -> String {
    match permille {
        0 => "0 %".to_owned(),
        1..5 => "<1 %".to_owned(),
        _ => format!("{} %", (permille + 5) / 10),
    }
}

fn ago(seconds: u64, language: Language) -> String {
    let (key, amount) = match seconds {
        0..60 => return t(language, "kpi-just-now"),
        60..3_600 => ("kpi-minutes-ago", seconds / 60),
        3_600..86_400 => ("kpi-hours-ago", seconds / 3_600),
        _ => ("kpi-days-ago", seconds / 86_400),
    };
    t_args(language, key, &[("count", &amount.to_string())])
}

/// One bar per bucket, as wide as the chart allows, scaled to the busiest bucket.
fn spark(values: &[u64]) -> String {
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let cells = (BAR_WIDTH / values.len().max(1)).max(1);
    values
        .iter()
        .flat_map(|&value| {
            let block = if value == 0 {
                ' '
            } else {
                let index = usize::try_from(value * 7 / max).unwrap_or(7);
                BLOCKS[index.min(7)]
            };
            std::iter::repeat_n(block, cells)
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> ModelId {
        ModelId::try_from(text.to_owned()).expect("BUG")
    }

    #[test]
    fn families_are_short_and_drop_dates() {
        assert_eq!(family(&id("claude-opus-5-5")), "Opus 5.5");
        assert_eq!(family(&id("claude-haiku-4-5-20251001")), "Haiku 4.5");
        assert_eq!(family(&id("claude-opus-4-20250514")), "Opus 4");
    }

    #[test]
    fn small_shares_are_shown_as_below_one_percent() {
        assert_eq!(percent(3), "<1 %");
        assert_eq!(percent(305), "31 %");
        assert_eq!(percent(0), "0 %");
    }

    #[test]
    fn spark_scales_to_the_busiest_day_and_keeps_empty_days_blank() {
        assert_eq!(spark(&[0, 1, 8]).chars().count(), 60);
        assert!(spark(&[0, 1, 8]).ends_with('█'));
    }
}
