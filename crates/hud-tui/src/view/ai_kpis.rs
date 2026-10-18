//! Key figures on top of the AI tab: fixed horizons, no switching.

use hud_application::usecases::usage::{PeriodKpis, PeriodUsage, SourceUsage, UsageReport};
use hud_domain::usage::{Accuracy, ModelId, PlanUsage, PlanWindow, Provider};
use jiff::Timestamp;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::ai::{compact, count, money};
use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::Status;
use crate::settings::Language;

const LABEL_WIDTH: usize = 20;
/// Columns a plan bar leaves for its label and the text after it.
const BAR_MARGIN: usize = 54;

/// Width of all bars and the activity chart: what the window leaves after
/// label and trailing text, so the longest line still fits, within 10 to 80.
fn bar_width(width: u16) -> usize {
    usize::from(width).saturating_sub(BAR_MARGIN).clamp(10, 80)
}
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const INDENT: &str = "  ";

/// All sections for the selected period; only plan usage is current.
pub(super) fn lines<'a>(
    model: &Model,
    source: &SourceUsage,
    usage: &PeriodUsage,
    report: &UsageReport,
    width: u16,
) -> Vec<Line<'a>> {
    let provider = source.state.provider;
    let bar = bar_width(width);
    let kpis = &usage.kpis;
    let mut lines = Vec::new();
    let mut section = |heading: Line<'a>, body: Vec<Line<'a>>| {
        lines.push(Line::raw(""));
        lines.push(heading);
        lines.extend(body);
    };
    section(
        heading(model, "kpi-consumption", None),
        consumption(model, usage, provider, width),
    );
    let mut models = bar_and_legend(model, kpis, bar, usize::from(width));
    models.push(Line::raw(""));
    models.extend(super::ai::model_table(model, usage, provider, width));
    section(heading(model, "kpi-models", None), models);
    section(
        heading(model, "kpi-effort", None),
        vec![effort(model, kpis, provider)],
    );
    section(
        heading(model, "kpi-activity", None),
        activity(model, usage, bar),
    );
    if let Some(plan) = &source.plan {
        section(
            plan_heading(model, plan, report.at),
            plan_lines(model, plan, report.at, bar),
        );
    }
    if !kpis.projects.is_empty() {
        section(
            heading(model, "kpi-projects", None),
            vec![projects(model, kpis)],
        );
    }
    if provider == Provider::Claude && super::ai::shows_telemetry(model, usage) {
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
    label_text(model, &t(model.settings.language, key))
}

fn label_text<'a>(model: &Model, text: &str) -> Span<'a> {
    Span::styled(
        format!("{INDENT}{}", fit(text, LABEL_WIDTH)),
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

fn consumption<'a>(
    model: &Model,
    usage: &PeriodUsage,
    provider: Provider,
    width: u16,
) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let tokens = usage.tokens.value;
    let reasoning = tokens.reasoning.map(|value| ("ai-reasoning", value));
    let token_line = vec![
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
    let mut kinds = Vec::new();
    for (key, value) in [
        ("ai-input", tokens.input),
        ("ai-output", tokens.output),
        (
            "ai-cache-write",
            tokens.cache_write_5m + tokens.cache_write_1h,
        ),
        ("ai-cache-read", tokens.cache_read),
    ]
    .into_iter()
    // Codex does not write to the cache; it reports reasoning instead.
    .filter(|(key, _)| !(provider == Provider::Codex && *key == "ai-cache-write"))
    .chain(reasoning)
    {
        kinds.push([
            soft(model, format!("{} ", t(language, key))),
            Span::styled(compact(value, language), Style::new().fg(model.theme.ink)),
        ]);
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
        soft(
            model,
            format!("      {}", super::ai::billing_label(model, provider)),
        ),
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
    let mut lines = wrapped(token_line, kinds, usize::from(width));
    lines.push(Line::from(cost_line));
    lines.push(Line::from(hit_line));
    lines
}

/// `first`, then the `parts` three columns apart; a part that would pass
/// `width` starts a new line under the values.
fn wrapped<'a>(first: Vec<Span<'a>>, parts: Vec<[Span<'a>; 2]>, width: usize) -> Vec<Line<'a>> {
    let length = |spans: &[Span<'_>]| spans.iter().map(Span::width).sum::<usize>();
    let indent = INDENT.len() + LABEL_WIDTH;
    let mut lines = Vec::new();
    let mut current = first;
    for part in parts {
        let gap = if length(&current) > indent { "   " } else { "" };
        if length(&current) + gap.len() + length(&part) > width && length(&current) > indent {
            lines.push(Line::from(std::mem::take(&mut current)));
            current.push(Span::raw(" ".repeat(indent)));
        } else {
            current.push(Span::raw(gap));
        }
        current.extend(part);
    }
    lines.push(Line::from(current));
    lines
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

fn bar_and_legend<'a>(
    model: &Model,
    kpis: &PeriodKpis,
    bar_width: usize,
    width: usize,
) -> Vec<Line<'a>> {
    if kpis.models.is_empty() {
        return Vec::new();
    }
    let mut bar = vec![Span::raw(INDENT)];
    let mut used = 0;
    for (index, (_, share)) in kpis.models.iter().enumerate() {
        let width = ((usize::from(*share) * bar_width + 500) / 1000).min(bar_width - used);
        used += width;
        bar.push(Span::styled(
            "█".repeat(width),
            Style::new().fg(palette(model, index)),
        ));
    }
    bar.push(Span::styled(
        "░".repeat(bar_width - used),
        Style::new().fg(model.theme.rule),
    ));
    // At most four models, and only as many as fit next to each other.
    let mut legend = vec![Span::raw(INDENT)];
    let mut used = INDENT.len();
    for (index, (id, share)) in kpis.models.iter().take(4).enumerate() {
        let gap = if index == 0 { "" } else { "      " };
        let entry = [
            Span::styled(format!("{gap}■ "), Style::new().fg(palette(model, index))),
            soft(model, format!("{} ", family(id))),
            strong(model, percent(*share)),
        ];
        let entry_width: usize = entry.iter().map(Span::width).sum();
        if index > 0 && used + entry_width > width {
            break;
        }
        used += entry_width;
        legend.extend(entry);
    }
    vec![Line::from(bar), Line::from(legend)]
}

fn effort<'a>(model: &Model, kpis: &PeriodKpis, provider: Provider) -> Line<'a> {
    let language = model.settings.language;
    let value = |share: Option<u16>| share.map_or_else(|| "—".to_owned(), percent);
    // Codex logs say nothing about subagents.
    if provider == Provider::Codex {
        return Line::from(vec![
            label(model, "kpi-reasoning"),
            strong(model, value(kpis.thinking)),
            soft(model, format!(" {}", t(language, "kpi-of-output"))),
        ]);
    }
    Line::from(vec![
        label(model, "kpi-subagents"),
        strong(model, value(kpis.subagents)),
        soft(model, format!(" {}", t(language, "kpi-of-fresh"))),
        soft(model, format!("      {} ", t(language, "kpi-thinking"))),
        strong(model, value(kpis.thinking)),
        soft(model, format!(" {}", t(language, "kpi-of-output"))),
    ])
}

fn activity<'a>(model: &Model, usage: &PeriodUsage, width: usize) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let kpis = &usage.kpis;
    let chart = Line::from(vec![
        Span::raw(INDENT),
        Span::styled(
            spark(&kpis.activity, width),
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
    vec![chart, axis(model, usage, width), totals]
}

/// Start and end of the chart under its left and right edge.
fn axis<'a>(model: &Model, usage: &PeriodUsage, chart: usize) -> Line<'a> {
    let language = model.settings.language;
    let buckets = usage.kpis.activity.len();
    let width = (chart / buckets.max(1)).max(1) * buckets;
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

/// `Abo-Auslastung   Plan Plus, Stand vor 3 min`.
fn plan_heading<'a>(model: &Model, plan: &PlanUsage, now: Timestamp) -> Line<'a> {
    let language = model.settings.language;
    let ago = ago(seconds_between(plan.at, now), language);
    let note = match &plan.plan {
        Some(name) => t_args(
            language,
            "kpi-plan-note",
            &[("plan", &title_case(name)), ("ago", &ago)],
        ),
        None => t_args(language, "kpi-plan-as-of", &[("ago", &ago)]),
    };
    Line::from(vec![
        Span::styled(
            t(language, "kpi-plan"),
            Style::new()
                .fg(model.theme.ink)
                .add_modifier(Modifier::BOLD),
        ),
        soft(model, format!("   {note}")),
    ])
}

/// One bar per rate-limit window; a window past its reset only says so.
fn plan_lines<'a>(
    model: &Model,
    plan: &PlanUsage,
    now: Timestamp,
    bar_width: usize,
) -> Vec<Line<'a>> {
    let language = model.settings.language;
    [
        (plan.primary, "plan-window-primary"),
        (plan.secondary, "plan-window-secondary"),
    ]
    .into_iter()
    .filter_map(|(window, fallback)| window.map(|window| (window, fallback)))
    .map(|(window, fallback)| {
        let name = label_text(model, &window_name(language, &window, fallback));
        let time = clock(model, window.resets_at, now);
        if !window.is_current(now) {
            return Line::from(vec![
                name,
                soft(
                    model,
                    t_args(language, "plan-reset-done", &[("time", &time)]),
                ),
            ]);
        }
        let used = window.used_permille.min(1000);
        let filled = usize::from(used) * bar_width / 1000;
        let color = if used >= 900 {
            model.theme.status(Status::Caution)
        } else {
            model.theme.status(Status::Advisory)
        };
        Line::from(vec![
            name,
            Span::styled("█".repeat(filled), Style::new().fg(color)),
            Span::styled(
                "░".repeat(bar_width - filled),
                Style::new().fg(model.theme.rule),
            ),
            Span::styled(
                format!("   {}", percent(used)),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
            soft(
                model,
                format!("   {}", t_args(language, "plan-resets", &[("time", &time)])),
            ),
        ])
    })
    .collect()
}

/// `5 Stunden`, `7 Tage`; without a length the window's role.
fn window_name(language: Language, window: &PlanWindow, fallback: &str) -> String {
    match window.minutes {
        Some(minutes) if minutes > 0 && minutes % 1440 == 0 => {
            t_count(language, "plan-window-days", "count", minutes / 1440)
        }
        Some(minutes) if minutes > 0 && minutes % 60 == 0 => {
            t_count(language, "plan-window-hours", "count", minutes / 60)
        }
        Some(minutes) if minutes > 0 => t_count(language, "plan-window-minutes", "count", minutes),
        _ => t(language, fallback),
    }
}

/// Local time; with the date when it is not the day of `now`.
fn clock(model: &Model, at: Timestamp, now: Timestamp) -> String {
    let local = at.to_zoned(model.time_zone.clone());
    let today = now.to_zoned(model.time_zone.clone()).date();
    if local.date() == today {
        return local.strftime("%H:%M").to_string();
    }
    match model.settings.language {
        Language::De => local.strftime("%d.%m. %H:%M").to_string(),
        Language::En => local.strftime("%m/%d %H:%M").to_string(),
    }
}

fn seconds_between(earlier: Timestamp, later: Timestamp) -> u64 {
    u64::try_from(later.duration_since(earlier).as_secs()).unwrap_or(0)
}

/// `plus` → `Plus`.
fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
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

/// `Opus 5.5` from `claude-opus-5-5`, `Haiku 4.5` from `claude-haiku-4-5-20251001`,
/// `GPT-5.6 Luna` from `gpt-5.6-luna`.
fn family(model: &ModelId) -> String {
    if let Some(rest) = model.as_str().strip_prefix("gpt-") {
        let mut parts = rest.split('-');
        let version = parts.next().unwrap_or_default();
        let words: Vec<String> = parts.map(title_case).collect();
        return [format!("GPT-{version}")]
            .into_iter()
            .chain(words)
            .collect::<Vec<_>>()
            .join(" ");
    }
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
fn spark(values: &[u64], width: usize) -> String {
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let cells = (width / values.len().max(1)).max(1);
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
    fn openai_families_keep_their_version_and_name() {
        assert_eq!(family(&id("gpt-5.6-luna")), "GPT-5.6 Luna");
        assert_eq!(family(&id("gpt-5-codex")), "GPT-5 Codex");
    }

    #[test]
    fn plan_windows_are_named_by_their_length() {
        let window = |minutes| PlanWindow {
            minutes,
            used_permille: 0,
            resets_at: Timestamp::UNIX_EPOCH,
        };
        let name = |minutes| window_name(Language::De, &window(minutes), "plan-window-primary");
        assert_eq!(name(Some(300)), "5 Stunden");
        assert_eq!(name(Some(10_080)), "7 Tage");
        assert_eq!(name(Some(90)), "90 min");
        assert_eq!(name(None), "Hauptfenster");
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
        assert_eq!(spark(&[0, 1, 8], 60).chars().count(), 60);
        assert!(spark(&[0, 1, 8], 60).ends_with('█'));
    }
}
