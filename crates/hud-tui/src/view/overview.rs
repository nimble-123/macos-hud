//! Overview: machine load with sparklines, then only what deviates (Dark Cockpit).

use hud_application::ports::system::SystemSample;
use hud_application::settings::Billing;
use hud_application::usecases::workspaces::RepoState;
use hud_domain::git::SkipReason;
use hud_domain::update::SourceId;
use hud_domain::usage::Period;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, source_glyph, status_glyph};
use crate::settings::Language;
use crate::theme::Activity;

const LABEL_WIDTH: usize = 12;
/// One block per sample: 60 samples every 2 s are the last two minutes.
pub(crate) const SPARK_SAMPLES: usize = 60;
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// From here on a load value is shown in `caution`.
const HIGH_LOAD: u8 = 90;

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let lines = lines(model, area.width);
    super::render_scrolled(model, frame, area, lines, model.overview_scroll);
}

/// Everything the overview shows, top to bottom, for an area `width` wide.
pub(super) fn lines<'a>(model: &Model, width: u16) -> Vec<Line<'a>> {
    let mut lines = system_lines(model);
    lines.push(Line::raw(""));
    let deviations = deviation_lines(model);
    if deviations.is_empty() {
        lines.push(calm_line(model));
    } else {
        lines.extend(deviations);
    }
    if let Some(line) = ai_today(model) {
        lines.push(Line::raw(""));
        lines.push(line);
    }
    let width = usize::from(width);
    let calendars = [
        model
            .usage
            .as_ref()
            .map(|report| (Activity::Ai, &report.calendar)),
        model
            .git_activity
            .as_ref()
            .map(|calendar| (Activity::Git, calendar)),
    ];
    for (activity, calendar) in calendars.into_iter().flatten() {
        let calendar = super::calendar::lines(model, activity, calendar, width);
        if !calendar.is_empty() {
            lines.push(Line::raw(""));
            lines.extend(calendar);
        }
    }
    lines
}

fn system_lines<'a>(model: &Model) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    // With Nerd Font icons each value gets its symbol in front of the name.
    let label = |key: &str, source: Source| {
        let name = t(language, key);
        let text = source_glyph(model.settings.icons, source)
            .map_or_else(|| name.clone(), |glyph| format!("{glyph} {name}"));
        Span::styled(fit(&text, LABEL_WIDTH), Style::new().fg(theme.soft))
    };
    let Some(latest) = model.load.back().copied() else {
        return vec![Line::styled(
            t(language, "overview-measuring"),
            Style::new().fg(theme.soft),
        )];
    };
    let load = |percent: u8| {
        let color = if percent >= HIGH_LOAD {
            theme.status(Status::Caution)
        } else {
            theme.ink
        };
        Span::styled(format!("{percent:>3} %   "), Style::new().fg(color))
    };
    let spark = |values: Vec<u8>| Span::styled(sparkline(&values), Style::new().fg(theme.muted));
    let cpu: Vec<u8> = model.load.iter().map(|s| s.cpu_percent).collect();
    let memory: Vec<u8> = model
        .load
        .iter()
        .map(SystemSample::memory_percent)
        .collect();
    vec![
        Line::from(vec![
            label("overview-cpu", Source::Cpu),
            load(latest.cpu_percent),
            spark(cpu),
        ]),
        Line::from(vec![
            label("overview-memory", Source::Memory),
            load(latest.memory_percent()),
            spark(memory),
            Span::styled(
                format!(
                    "   {} / {}",
                    gigabytes(latest.memory_used, language),
                    gigabytes(latest.memory_total, language)
                ),
                Style::new().fg(theme.muted),
            ),
        ]),
        Line::from(vec![
            label("overview-processes", Source::Processes),
            Span::styled(latest.processes.to_string(), Style::new().fg(theme.ink)),
        ]),
    ]
}

/// Repositories and updates that need attention, one line per area.
fn deviation_lines<'a>(model: &Model) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let repos = repo_counts(model);
    let parts: Vec<(Status, String)> = [
        (Status::Advisory, "overview-repos-behind", repos.behind),
        (Status::Caution, "overview-repos-skipped", repos.skipped),
        (Status::Warning, "overview-repos-diverged", repos.diverged),
        (Status::Warning, "overview-repos-errors", repos.errors),
        (
            Status::Warning,
            "overview-workspaces-unavailable",
            repos.unavailable,
        ),
    ]
    .into_iter()
    .filter(|(_, _, count)| *count > 0)
    .map(|(status, key, count)| {
        (
            status,
            t_count(model.settings.language, key, "count", count),
        )
    })
    .collect();
    if !parts.is_empty() {
        lines.push(status_line(model, parts));
    }
    if let Some(line) = updates_line(model) {
        lines.push(line);
    }
    lines
}

#[derive(Default)]
struct RepoCounts {
    behind: u32,
    skipped: u32,
    diverged: u32,
    errors: u32,
    unavailable: u32,
}

fn repo_counts(model: &Model) -> RepoCounts {
    let mut counts = RepoCounts::default();
    for workspace in &model.workspaces {
        if workspace.repos.is_none() {
            counts.unavailable += 1;
        }
    }
    for repo in model.repo_rows() {
        match &repo.state {
            RepoState::UpToDate | RepoState::FastForwarded(_) => {}
            // Opted out means behind and waiting for a manual fast-forward.
            RepoState::Behind(_) | RepoState::Skipped(SkipReason::OptedOut) => counts.behind += 1,
            RepoState::Skipped(SkipReason::Diverged { .. }) => counts.diverged += 1,
            RepoState::Skipped(_) => counts.skipped += 1,
            RepoState::Error(_) => counts.errors += 1,
        }
    }
    counts
}

/// `◆ 11 Updates (Homebrew 1, npm 10)`; pinned packages do not count.
fn updates_line<'a>(model: &Model) -> Option<Line<'a>> {
    let language = model.settings.language;
    let per_source: Vec<(SourceId, u32)> = model
        .updates
        .iter()
        .filter_map(|report| {
            let check = report.check.as_ref()?.as_ref().ok()?;
            let count = check.candidates.iter().filter(|c| !c.pinned).count();
            (count > 0).then(|| (report.source, u32::try_from(count).unwrap_or(u32::MAX)))
        })
        .collect();
    let total: u32 = per_source.iter().map(|(_, count)| count).sum();
    if total == 0 {
        return None;
    }
    let sources = per_source
        .iter()
        .map(|(source, count)| {
            let name = t(
                language,
                match source {
                    SourceId::Homebrew => "src-homebrew",
                    SourceId::Npm => "src-npm",
                    SourceId::Skills => "src-skills",
                },
            );
            format!("{name} {count}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "{} ({sources})",
        t_count(language, "overview-updates", "count", total)
    );
    Some(status_line(model, vec![(Status::Advisory, text)]))
}

fn status_line<'a>(model: &Model, parts: Vec<(Status, String)>) -> Line<'a> {
    let mut spans = Vec::with_capacity(parts.len());
    for (index, (status, text)) in parts.into_iter().enumerate() {
        let gap = if index == 0 { "" } else { "   " };
        spans.push(Span::styled(
            format!("{gap}{} {text}", status_glyph(model.settings.icons, status)),
            Style::new().fg(model.theme.status(status)),
        ));
    }
    Line::from(spans)
}

/// The single neutral sentence of the normal state.
fn calm_line<'a>(model: &Model) -> Line<'a> {
    let language = model.settings.language;
    let text = match (model.last_sync, model.now) {
        (Some(at), Some(now)) => {
            let minutes = now.duration_since(at).as_secs() / 60;
            if minutes == 0 {
                t(language, "overview-calm-now")
            } else {
                t_args(
                    language,
                    "overview-calm-since",
                    &[("minutes", &minutes.to_string())],
                )
            }
        }
        _ if model.workspaces.is_empty() && model.updates.is_empty() => {
            t(language, "overview-checking")
        }
        _ => t(language, "overview-calm"),
    };
    Line::styled(text, Style::new().fg(model.theme.soft))
}

/// `KI heute  1,2 M Token  ≈ 4,10 $ API-Äquivalent`, all sources together.
fn ai_today<'a>(model: &Model) -> Option<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let report = model.usage.as_ref()?;
    let today: Vec<_> = report
        .sources
        .iter()
        .filter_map(|source| source.period(Period::Today))
        .collect();
    if today.is_empty() {
        return None;
    }
    let tokens: u64 = today.iter().map(|usage| usage.tokens.value.total()).sum();
    let cost = hud_domain::usage::CostEstimate {
        micro_usd: today.iter().map(|usage| usage.cost.value.micro_usd).sum(),
        prices_as_of: hud_domain::usage::PRICES_AS_OF,
    };
    // Only a pure API setup pays what the estimate says.
    let billing = model
        .preferences
        .as_ref()
        .map_or(Billing::Subscription, |p| {
            let api = report.sources.iter().all(|source| {
                let billing = match source.state.provider {
                    hud_domain::usage::Provider::Claude => p.claude.billing,
                    hud_domain::usage::Provider::Codex => p.codex.billing,
                };
                billing == Billing::Api
            });
            if api {
                Billing::Api
            } else {
                Billing::Subscription
            }
        });
    let basis = t(
        language,
        match billing {
            Billing::Subscription => "ai-api-equivalent",
            Billing::Api => "ai-estimate",
        },
    );
    Some(Line::from(vec![
        Span::styled(
            fit(&t(language, "overview-ai-today"), LABEL_WIDTH),
            Style::new().fg(theme.soft),
        ),
        Span::styled(
            format!(
                "{} {}   ≈ {}",
                super::ai::compact(tokens, language),
                t(language, "ai-tokens"),
                super::ai::money(cost, language)
            ),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("   {basis}"), Style::new().fg(theme.soft)),
    ]))
}

/// Right-aligned so the newest value is always at the same place.
pub(crate) fn sparkline(values: &[u8]) -> String {
    let recent = &values[values.len().saturating_sub(SPARK_SAMPLES)..];
    let mut line = " ".repeat(SPARK_SAMPLES - recent.len());
    for &value in recent {
        let index = usize::from(value.min(100)) * (BLOCKS.len() - 1) / 100;
        line.push(BLOCKS[index]);
    }
    line
}

/// `9,8 GB` or `9.8 GB` (binary gigabytes, as Activity Monitor shows them).
fn gigabytes(bytes: u64, language: Language) -> String {
    let tenths = bytes * 10 / (1024 * 1024 * 1024);
    let (whole, tenth) = (tenths / 10, tenths % 10);
    match language {
        Language::De => format!("{whole},{tenth} GB"),
        Language::En => format!("{whole}.{tenth} GB"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_is_right_aligned_and_scaled() {
        let line = sparkline(&[0, 50, 100]);
        assert_eq!(line.chars().count(), SPARK_SAMPLES);
        assert!(line.ends_with("▁▄█"), "{line:?}");
    }

    #[test]
    fn sparkline_keeps_only_the_last_two_minutes() {
        let values = vec![100; SPARK_SAMPLES + 10];
        assert_eq!(sparkline(&values), "█".repeat(SPARK_SAMPLES));
    }

    fn system_labels(icons: crate::settings::IconSet) -> Vec<String> {
        use hud_application::AppEvent;
        let mut model = Model::new(crate::UiSettings {
            icons,
            ..crate::UiSettings::default()
        });
        crate::update(
            &mut model,
            crate::Msg::App(AppEvent::System(SystemSample {
                cpu_percent: 20,
                memory_used: 1,
                memory_total: 2,
                processes: 3,
            })),
        );
        system_lines(&model)
            .iter()
            .map(|line| line.spans[0].content.trim_end().to_owned())
            .collect()
    }

    #[test]
    fn nerd_icons_mark_cpu_memory_and_processes() {
        assert_eq!(
            system_labels(crate::settings::IconSet::Nerd),
            vec!["\u{f4bc} CPU", "\u{f035b} Memory", "\u{f0ae} Processes"]
        );
    }

    #[test]
    fn other_sets_show_only_the_names() {
        assert_eq!(
            system_labels(crate::settings::IconSet::Unicode),
            vec!["CPU", "Memory", "Processes"]
        );
    }

    #[test]
    fn gigabytes_use_binary_units() {
        assert_eq!(gigabytes(16 * 1024 * 1024 * 1024, Language::De), "16,0 GB");
        assert_eq!(gigabytes(10_522_669_876, Language::En), "9.8 GB");
    }
}
