//! Updates tab: sources, candidates, job states and the confirmation dialog.

use hud_application::ports::updates::{Availability, UpdateError};
use hud_application::usecases::updates::{JobState, SourceReport};
use hud_domain::update::{InstallTarget, PackageKind, SourceId, UpdateCandidate, VersionBump};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, source_glyph, status_glyph};

const NAME_WIDTH: usize = 34;
const VERSION_WIDTH: usize = 26;

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    if model.updates.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                t(language, "src-checking"),
                Style::new().fg(theme.soft),
            )),
            area,
        );
        return;
    }
    let (lines, _) = build(model);
    frame.render_widget(Paragraph::new(lines), area);
    if let Some(target) = &model.confirm {
        render_confirm(model, target, frame, area);
    }
}

/// Update row under line `y` of the tab's area.
pub(super) fn row_at(model: &Model, y: u16) -> Option<usize> {
    if model.updates.is_empty() {
        return None;
    }
    build(model).1.get(usize::from(y)).copied().flatten()
}

/// Lines and, per line, the index into [`Model::update_rows`] it shows.
fn build(model: &Model) -> (Vec<Line<'_>>, Vec<Option<usize>>) {
    let language = model.settings.language;
    let theme = &model.theme;
    let rows = model.update_rows();
    let index_of = |target: &InstallTarget| rows.iter().position(|row| row == target);
    let mut lines = Vec::new();
    let mut map = Vec::new();
    let mut push = |line: Line<'static>, row: Option<usize>, lines: &mut Vec<Line<'static>>| {
        lines.push(line);
        map.push(row);
    };
    for report in &model.updates {
        if !lines.is_empty() {
            push(Line::raw(""), None, &mut lines);
        }
        push(header(model, report), None, &mut lines);
        let Some(check) = &report.check else { continue };
        match check {
            Err(error) => push(error_line(model, error), None, &mut lines),
            Ok(check) if check.updates_known && check.candidates.is_empty() => {
                let glyph = status_glyph(model.settings.icons, Status::Neutral);
                let line = Line::styled(
                    format!("  {glyph} {}", t(language, "src-up-to-date")),
                    Style::new().fg(theme.muted),
                );
                push(line, None, &mut lines);
            }
            Ok(check) if check.updates_known => {
                for candidate in &check.candidates {
                    let target = InstallTarget::Package(candidate.clone());
                    let index = index_of(&target);
                    let selected = index == Some(model.update_selected);
                    push(
                        candidate_line(model, candidate, &target, selected),
                        index,
                        &mut lines,
                    );
                }
            }
            Ok(check) => {
                let count = u32::try_from(check.installed.len()).unwrap_or(u32::MAX);
                let line = Line::styled(
                    format!(
                        "  {}",
                        t_count(language, "src-skills-installed", "count", count)
                    ),
                    Style::new().fg(theme.muted),
                );
                push(line, None, &mut lines);
                let target = InstallTarget::AllSkills;
                let index = index_of(&target);
                let selected = index == Some(model.update_selected);
                push(update_all_line(model, &target, selected), index, &mut lines);
            }
        }
    }
    (lines, map)
}

fn source_name(source: SourceId) -> &'static str {
    match source {
        SourceId::Homebrew => "src-homebrew",
        SourceId::Npm => "src-npm",
        SourceId::Skills => "src-skills",
    }
}

fn header<'a>(model: &Model, report: &SourceReport) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let icon = match report.source {
        SourceId::Homebrew => source_glyph(model.settings.icons, Source::Homebrew),
        SourceId::Npm => source_glyph(model.settings.icons, Source::Npm),
        SourceId::Skills => source_glyph(model.settings.icons, Source::Skills),
    }
    .map(|glyph| format!("{glyph} "))
    .unwrap_or_default();
    let mut spans = vec![Span::styled(
        format!("{icon}{}", t(language, source_name(report.source))),
        Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
    )];
    let warning = Style::new().fg(theme.status(Status::Warning));
    let glyph = status_glyph(model.settings.icons, Status::Warning);
    match &report.availability {
        Availability::Available(version) => spans.push(Span::styled(
            format!("   {version}"),
            Style::new().fg(theme.soft),
        )),
        Availability::Missing => spans.push(Span::styled(
            format!("   {glyph} {}", t(language, "src-missing")),
            warning,
        )),
        Availability::Broken(message) => {
            spans.push(Span::styled(
                format!(
                    "   {glyph} {}",
                    t_args(language, "src-broken", &[("message", message)])
                ),
                warning,
            ));
        }
    }
    Line::from(spans)
}

fn error_line<'a>(model: &Model, error: &UpdateError) -> Line<'a> {
    let glyph = status_glyph(model.settings.icons, Status::Warning);
    let text = t_args(
        model.settings.language,
        "src-broken",
        &[("message", &error.to_string())],
    );
    Line::styled(
        format!("  {glyph} {text}"),
        Style::new().fg(model.theme.status(Status::Warning)),
    )
}

fn candidate_line<'a>(
    model: &Model,
    candidate: &UpdateCandidate,
    target: &InstallTarget,
    selected: bool,
) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let (status, note) = job_status(model, target).unwrap_or_else(|| {
        if candidate.pinned {
            (Status::Neutral, t(language, "pinned"))
        } else if candidate.bump() == VersionBump::Major {
            (Status::Caution, t(language, "bump-major"))
        } else {
            (Status::Advisory, String::new())
        }
    });
    let glyph = spinner_or_glyph(model, target, status);
    let name_style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink)
    };
    let versions = format!("{} → {}", candidate.current, candidate.latest);
    Line::from(vec![
        Span::styled(
            if selected { "▌" } else { " " },
            Style::new().fg(theme.focus),
        ),
        Span::raw(" "),
        Span::styled(glyph, Style::new().fg(theme.status(status))),
        Span::raw(" "),
        Span::styled(fit(candidate.name.as_str(), NAME_WIDTH), name_style),
        Span::styled(fit(&versions, VERSION_WIDTH), Style::new().fg(theme.muted)),
        Span::styled(note, Style::new().fg(theme.status(status))),
    ])
}

fn update_all_line<'a>(model: &Model, target: &InstallTarget, selected: bool) -> Line<'a> {
    let theme = &model.theme;
    let (status, note) = job_status(model, target).unwrap_or((Status::Advisory, String::new()));
    let glyph = spinner_or_glyph(model, target, status);
    let style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink)
    };
    Line::from(vec![
        Span::styled(
            if selected { "▌" } else { " " },
            Style::new().fg(theme.focus),
        ),
        Span::raw(" "),
        Span::styled(glyph, Style::new().fg(theme.status(status))),
        Span::raw(" "),
        Span::styled(
            fit(
                &t(model.settings.language, "src-update-all"),
                NAME_WIDTH + VERSION_WIDTH,
            ),
            style,
        ),
        Span::styled(note, Style::new().fg(theme.status(status))),
    ])
}

/// Status and text from the job for `target`, if there is one.
fn job_status(model: &Model, target: &InstallTarget) -> Option<(Status, String)> {
    let language = model.settings.language;
    let job = model.job_for(target)?;
    Some(match &job.state {
        JobState::Queued => (Status::Advisory, t(language, "job-queued")),
        JobState::Running => (Status::Advisory, t(language, "job-running")),
        JobState::Succeeded => (Status::Ok, t(language, "job-succeeded")),
        JobState::NeedsInteraction => (Status::Caution, t(language, "job-needs-interaction")),
        JobState::Denied(_) => (Status::Warning, t(language, "job-denied")),
        JobState::Failed(message) => (
            Status::Warning,
            t_args(language, "job-failed", &[("message", message)]),
        ),
        JobState::TimedOut => (Status::Warning, t(language, "job-timed-out")),
        JobState::Cancelled => (Status::Caution, t(language, "job-cancelled")),
    })
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn spinner(model: &Model) -> &'static str {
    SPINNER[usize::try_from(model.ticks % 10).unwrap_or(0)]
}

fn spinner_or_glyph(model: &Model, target: &InstallTarget, status: Status) -> &'static str {
    let running = model.jobs.values().any(|job| job.target == *target);
    if running {
        spinner(model)
    } else {
        status_glyph(model.settings.icons, status)
    }
}

/// The exact command an installation runs, shown before confirming.
pub(super) fn command_preview(target: &InstallTarget) -> String {
    match target {
        InstallTarget::Package(candidate) => match candidate.kind {
            PackageKind::Formula => format!("brew upgrade {}", candidate.name),
            PackageKind::Cask => format!("brew upgrade --cask {}", candidate.name),
            PackageKind::NpmGlobal => format!(
                "npm install --global {}@{}",
                candidate.name, candidate.latest
            ),
            PackageKind::Skill => format!("npx skills update --global {}", candidate.name),
        },
        InstallTarget::AllSkills => "npx skills update --global --yes".to_owned(),
    }
}

fn render_confirm(model: &Model, target: &InstallTarget, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let [dialog] = Layout::horizontal([Constraint::Length(72)])
        .flex(Flex::Center)
        .areas(area);
    let [dialog] = Layout::vertical([Constraint::Length(9)])
        .flex(Flex::Center)
        .areas(dialog);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  {}", command_preview(target)),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
    ];
    if let InstallTarget::Package(candidate) = target
        && candidate.bump() == VersionBump::Major
    {
        let glyph = status_glyph(model.settings.icons, Status::Caution);
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("  {glyph} {}", t(language, "confirm-major")),
            Style::new().fg(theme.status(Status::Caution)),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", t(language, "key-enter")),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}   ", t(language, "confirm-install")),
            Style::new().fg(theme.soft),
        ),
        Span::styled(
            format!("{} ", t(language, "key-esc")),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(t(language, "confirm-cancel"), Style::new().fg(theme.soft)),
    ]));
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(" {} ", t(language, "confirm-title")),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Clear, dialog);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        dialog,
    );
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
    use hud_domain::update::{PackageName, Version};

    use super::*;

    fn candidate(kind: PackageKind, name: &str) -> UpdateCandidate {
        UpdateCandidate {
            source: SourceId::Npm,
            kind,
            name: PackageName::try_from(name.to_owned()).expect("BUG"),
            current: Version::new("9.7.2"),
            latest: Version::new("10.1.0"),
            pinned: false,
        }
    }

    #[test]
    fn preview_shows_the_exact_command() {
        assert_eq!(
            command_preview(&InstallTarget::Package(candidate(
                PackageKind::NpmGlobal,
                "@sap/cds-dk"
            ))),
            "npm install --global @sap/cds-dk@10.1.0"
        );
        assert_eq!(
            command_preview(&InstallTarget::Package(candidate(
                PackageKind::Cask,
                "firefox"
            ))),
            "brew upgrade --cask firefox"
        );
        assert_eq!(
            command_preview(&InstallTarget::AllSkills),
            "npx skills update --global --yes"
        );
    }
}
