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
use crate::model::Confirm;

/// The package name column grows with the window between these widths.
const MIN_NAME_WIDTH: usize = 12;
const MAX_NAME_WIDTH: usize = 44;
const VERSION_WIDTH: usize = 26;
/// Width of the install dialogs, borders included.
const DIALOG_WIDTH: u16 = 72;

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
    let (lines, map) = build(model, area.width);
    let offset = super::follow(area, selected_line(&map, model.update_selected));
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), area);
    match &model.confirm {
        Some(Confirm::One(target)) => render_confirm(model, target, frame, area),
        Some(Confirm::All(targets)) => render_confirm_all(model, None, targets, frame, area),
        // Drawn over whichever tab asked for it, see `view`.
        Some(Confirm::Everything { .. }) | None => {}
    }
}

/// Line that shows row `selected`; the view keeps it on screen.
fn selected_line(map: &[Option<usize>], selected: usize) -> usize {
    map.iter()
        .position(|row| *row == Some(selected))
        .unwrap_or(0)
}

/// Update row under line `y` of the tab's area, as the frame shows it.
pub(super) fn row_at(model: &Model, area: Rect, y: u16) -> Option<usize> {
    if model.updates.is_empty() {
        return None;
    }
    let map = build(model, area.width).1;
    let offset = super::follow(area, selected_line(&map, model.update_selected));
    map.get(usize::from(y + offset)).copied().flatten()
}

/// Lines and, per line, the index into [`Model::update_rows`] it shows.
/// Name column for an area `width` wide: what marker, versions and the
/// longest job note leave.
fn name_width(width: u16) -> usize {
    usize::from(width)
        .saturating_sub(6 + VERSION_WIDTH + 30)
        .clamp(MIN_NAME_WIDTH, MAX_NAME_WIDTH)
}

fn build(model: &Model, width: u16) -> (Vec<Line<'_>>, Vec<Option<usize>>) {
    let names = name_width(width);
    // Marker, glyph and spaces take four columns before the name.
    let rest = usize::from(width).saturating_sub(4 + names + VERSION_WIDTH);
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
        if report.availability == Availability::Missing {
            push(missing_hint(model, report.source), None, &mut lines);
        }
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
                        candidate_line(model, candidate, &target, selected, names, rest),
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
                push(
                    update_all_line(model, &target, selected, names),
                    index,
                    &mut lines,
                );
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

/// Where hud looked and how to point it at the tool.
fn missing_hint<'a>(model: &Model, source: SourceId) -> Line<'a> {
    // Skills runs through npx, which hud expects next to npm.
    let (program, key) = match source {
        SourceId::Homebrew => ("brew", "brew"),
        SourceId::Npm => ("npm", "npm"),
        SourceId::Skills => ("npx", "npm"),
    };
    Line::styled(
        format!(
            "  {}",
            t_args(
                model.settings.language,
                "src-missing-hint",
                &[("program", program), ("key", key)]
            )
        ),
        Style::new().fg(model.theme.muted),
    )
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
    names: usize,
    rest: usize,
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
        Span::styled(fit(candidate.name.as_str(), names), name_style),
        Span::styled(fit(&versions, VERSION_WIDTH), Style::new().fg(theme.muted)),
        Span::styled(
            super::clip(&note, rest),
            Style::new().fg(theme.status(status)),
        ),
    ])
}

fn update_all_line<'a>(
    model: &Model,
    target: &InstallTarget,
    selected: bool,
    names: usize,
) -> Line<'a> {
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
                names + VERSION_WIDTH,
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
        JobState::Succeeded if crate::model::hud_update(target).is_some() => {
            (Status::Ok, t(language, "job-succeeded-restart"))
        }
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
    let [dialog] = Layout::horizontal([Constraint::Length(DIALOG_WIDTH)])
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

/// Dialog after `a`: how many packages per source, and which are major updates.
/// Confirmation for many installations at once; with `repos`, also for that
/// many fast-forwards ("update everything").
pub(super) fn render_confirm_all(
    model: &Model,
    repos: Option<usize>,
    targets: &[InstallTarget],
    frame: &mut Frame,
    area: Rect,
) {
    let language = model.settings.language;
    let theme = &model.theme;
    let majors: Vec<String> = targets
        .iter()
        .filter_map(|target| match target {
            InstallTarget::Package(candidate) if candidate.bump() == VersionBump::Major => {
                Some(candidate.name.to_string())
            }
            InstallTarget::Package(_) | InstallTarget::AllSkills => None,
        })
        .collect();

    let mut lines = vec![Line::raw("")];
    lines.extend(amount_lines(model, repos, targets));
    // The major warning may wrap; each extra row needs room in the dialog.
    let mut wrapped = 0;
    if !majors.is_empty() {
        let glyph = status_glyph(model.settings.icons, Status::Caution);
        let names = majors.join(", ");
        let text = format!(
            "  {glyph} {}",
            t_args(language, "confirm-all-major", &[("names", &names)])
        );
        wrapped = text.chars().count().saturating_sub(1) / usize::from(DIALOG_WIDTH - 2);
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            text,
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
            format!(
                "{}   ",
                t(
                    language,
                    if repos.is_some() {
                        "confirm-everything"
                    } else {
                        "confirm-install-all"
                    }
                )
            ),
            Style::new().fg(theme.soft),
        ),
        Span::styled(
            format!("{} ", t(language, "key-esc")),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(t(language, "confirm-cancel"), Style::new().fg(theme.soft)),
    ]));
    let [dialog] = Layout::horizontal([Constraint::Length(DIALOG_WIDTH)])
        .flex(Flex::Center)
        .areas(area);
    let height = u16::try_from(lines.len() + wrapped + 2).unwrap_or(u16::MAX);
    let [dialog] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(dialog);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(
                " {} ",
                t(
                    language,
                    if repos.is_some() {
                        "confirm-everything-title"
                    } else {
                        "confirm-all-title"
                    }
                )
            ),
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

/// One line per kind of work: fast-forwards, then packages per source.
fn amount_lines<'a>(
    model: &Model,
    repos: Option<usize>,
    targets: &[InstallTarget],
) -> Vec<Line<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut sources: Vec<(SourceId, u32, bool)> = Vec::new();
    for target in targets {
        let (source, all) = match target {
            InstallTarget::Package(candidate) => (candidate.source, false),
            InstallTarget::AllSkills => (SourceId::Skills, true),
        };
        match sources.iter_mut().find(|(id, ..)| *id == source) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 |= all;
            }
            None => sources.push((source, 1, all)),
        }
    }
    let mut lines = Vec::new();
    if let Some(count) = repos.filter(|count| *count > 0) {
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", fit(&t(language, "confirm-repos"), 16)),
                Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t_count(language, "confirm-all-repos", "count", count),
                Style::new().fg(theme.muted),
            ),
        ]));
    }
    for (source, count, all) in &sources {
        let amount = if *all {
            t(language, "confirm-all-skills")
        } else {
            t_count(language, "confirm-all-packages", "count", *count)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", fit(&t(language, source_name(*source)), 16)),
                Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
            ),
            Span::styled(amount, Style::new().fg(theme.muted)),
        ]));
    }
    lines
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
