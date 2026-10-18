//! Workspaces tab: repositories per workspace with status, reason and selection.

use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_domain::git::{GitOperation, Head, SkipReason};
use std::path::Path;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, fold_glyph, source_glyph, status_glyph};
use crate::settings::Language;
use crate::workspaces_tab::{self, Group, Row};

/// The repository name column grows with the window between these widths.
const MIN_NAME_WIDTH: usize = 12;
const MAX_NAME_WIDTH: usize = 44;
const BRANCH_WIDTH: usize = 18;
const COUNTS_WIDTH: usize = 9;

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    if model.workspaces.is_empty() {
        let line = Line::styled(t(language, "ws-none"), Style::new().fg(theme.soft));
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let (lines, _, selected_line) = build(model, area.width);
    frame.render_widget(
        Paragraph::new(lines).scroll((super::follow(area, selected_line), 0)),
        area,
    );
}

/// Row of [`workspaces_tab::rows`] under line `y` of the tab's area, as the frame shows it.
pub(super) fn row_at(model: &Model, area: Rect, y: u16) -> Option<usize> {
    let (_, rows, selected_line) = build(model, area.width);
    let line = usize::from(y + super::follow(area, selected_line));
    rows.get(line).copied().flatten()
}

/// Name column for an area `width` wide: what branch, counts and the
/// longest state text leave.
fn name_width(width: u16) -> usize {
    usize::from(width)
        .saturating_sub(6 + BRANCH_WIDTH + COUNTS_WIDTH + 40)
        .clamp(MIN_NAME_WIDTH, MAX_NAME_WIDTH)
}

/// Lines, the row of each line, and the line of the selection.
fn build(model: &Model, width: u16) -> (Vec<Line<'_>>, Vec<Option<usize>>, usize) {
    let names = name_width(width);
    // Marker, glyph and spaces take four columns before the name.
    let rest = usize::from(width).saturating_sub(4 + names + BRANCH_WIDTH + COUNTS_WIDTH);
    let language = model.settings.language;
    let theme = &model.theme;
    let rows = workspaces_tab::rows(model);
    let selected = workspaces_tab::selected_index(model, &rows);
    let mut lines = Vec::new();
    let mut map = Vec::new();
    let mut selected_line = 0usize;
    let tab = &model.workspaces_tab;
    if tab.searching || !tab.query.is_empty() {
        // The first workspace heading adds the blank line below.
        lines.push(search_line(model));
        map.push(None);
    }
    for (index, row) in rows.iter().enumerate() {
        let line = match row {
            Row::Workspace { report, open } => {
                if !lines.is_empty() {
                    lines.push(Line::raw(""));
                    map.push(None);
                }
                if selected == Some(index) {
                    selected_line = lines.len();
                }
                header(model, report, *open, selected == Some(index))
            }
            Row::Repo {
                report,
                base,
                level,
                group,
            } => {
                if selected == Some(index) {
                    selected_line = lines.len();
                }
                repo_line(
                    model,
                    report,
                    &Place {
                        base,
                        level: *level,
                        group: group.as_ref(),
                    },
                    selected == Some(index),
                    names,
                    rest,
                )
            }
            Row::UpToDate { count, every } => {
                let key = if *every {
                    "ws-every-up-to-date"
                } else {
                    "ws-more-up-to-date"
                };
                let count = u32::try_from(*count).unwrap_or(u32::MAX);
                Line::styled(
                    format!(
                        "  {} {}",
                        status_glyph(model.settings.icons, Status::Neutral),
                        t_count(language, key, "count", count)
                    ),
                    Style::new().fg(theme.soft),
                )
            }
            Row::Empty => {
                let message = if model.scan.is_some() {
                    "ws-scanning"
                } else {
                    "ws-empty"
                };
                Line::styled(
                    format!("  {}", t(language, message)),
                    Style::new().fg(theme.soft),
                )
            }
        };
        lines.push(line);
        map.push(Some(index));
    }

    (lines, map, selected_line)
}

/// `Suche: portal█`, the cursor only while typing.
fn search_line<'a>(model: &Model) -> Line<'a> {
    let theme = &model.theme;
    let tab = &model.workspaces_tab;
    let query_style = if tab.searching {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink)
    };
    let mut spans = vec![
        Span::styled(
            format!("{} ", t(model.settings.language, "log-search")),
            Style::new().fg(theme.soft),
        ),
        Span::styled(tab.query.clone(), query_style),
    ];
    if tab.searching {
        spans.push(Span::styled("█", Style::new().fg(theme.focus)));
    }
    Line::from(spans)
}

/// `▌▾ /path   23 Repos   ◆ 2`: marker, fold state, path, then counts or state.
fn header<'a>(
    model: &Model,
    workspace: &'a WorkspaceReport,
    open: bool,
    selected: bool,
) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let foldable = workspace
        .repos
        .as_ref()
        .is_some_and(|repos| !repos.is_empty());
    let fold = if foldable {
        fold_glyph(model.settings.icons, open)
    } else {
        " "
    };
    let path_style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink).add_modifier(Modifier::BOLD)
    };
    let mut spans = vec![
        Span::styled(
            if selected { "▌" } else { " " },
            Style::new().fg(theme.focus),
        ),
        Span::styled(format!("{fold} "), Style::new().fg(theme.soft)),
        Span::styled(workspace.workspace.to_string(), path_style),
    ];
    if workspace.unavailable.is_some() {
        let glyph = status_glyph(model.settings.icons, Status::Warning);
        spans.push(Span::styled(
            format!("   {glyph} {}", t(language, "ws-unavailable")),
            Style::new().fg(theme.status(Status::Warning)),
        ));
        return Line::from(spans);
    }
    if let Some(repos) = workspace.repos.as_ref().filter(|repos| !repos.is_empty()) {
        spans.extend(counts(model, repos));
    }
    if !workspace.auto_fast_forward {
        spans.push(Span::styled(
            format!("   {}", t(language, "ws-auto-off")),
            Style::new().fg(theme.soft),
        ));
    }
    Line::from(spans)
}

/// `23 Repos   ◆ 2  ▲ 1  ✖ 1`, or `alles aktuell` when nothing deviates.
fn counts<'a>(model: &Model, repos: &[RepoReport]) -> Vec<Span<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let total = u32::try_from(repos.len()).unwrap_or(u32::MAX);
    let mut spans = vec![Span::styled(
        format!("   {}", t_count(language, "ws-repo-count", "count", total)),
        Style::new().fg(theme.soft),
    )];
    let deviations = workspaces_tab::deviations(model, repos.iter());
    if deviations.is_empty() {
        spans.push(Span::styled(
            format!("   {}", t(language, "ws-all-current")),
            Style::new().fg(theme.soft),
        ));
    }
    spans.extend(deviation_spans(model, &deviations));
    spans
}

/// `   ◆ 2  ✖ 1` in the status colours.
fn deviation_spans<'a>(model: &Model, deviations: &[(Status, usize)]) -> Vec<Span<'a>> {
    deviations
        .iter()
        .enumerate()
        .map(|(index, (status, count))| {
            let gap = if index == 0 { "   " } else { "  " };
            Span::styled(
                format!(
                    "{gap}{} {count}",
                    status_glyph(model.settings.icons, *status)
                ),
                Style::new().fg(model.theme.status(*status)),
            )
        })
        .collect()
}

/// Where a repository row sits: below the workspace or nested in another repo.
struct Place<'a> {
    /// The name is shown relative to this folder.
    base: &'a Path,
    /// Number of repositories it is nested in; each indents the row.
    level: usize,
    /// Set when other repositories are nested in it.
    group: Option<&'a Group>,
}

fn repo_line<'a>(
    model: &Model,
    report: &'a RepoReport,
    place: &Place<'_>,
    selected: bool,
    names: usize,
    rest: usize,
) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let status = workspaces_tab::status(model, report);
    let quiet = status == Status::Neutral;

    // Every name keeps a column for the fold marker, so siblings line up.
    let fold = place.group.map_or_else(
        || "  ".to_owned(),
        |group| format!("{} ", fold_glyph(model.settings.icons, group.open)),
    );
    let indent = format!("{}{fold}", "  ".repeat(place.level));
    let name = report
        .repo
        .as_path()
        .strip_prefix(place.base)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(
            || report.repo.to_string(),
            |relative| relative.display().to_string(),
        );
    let name_style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else if quiet {
        Style::new().fg(theme.muted)
    } else {
        Style::new().fg(theme.ink)
    };
    let marker = if selected { "▌" } else { " " };
    let git_icon = source_glyph(model.settings.icons, Source::Git)
        .map(|g| format!("{g} "))
        .unwrap_or_default();

    let (branch, counts) = match &report.status {
        Some(status) => (
            match &status.head {
                Head::Branch(name) => name.to_string(),
                Head::Detached => t(language, "branch-detached"),
            },
            counts_text(status.ahead, status.behind),
        ),
        None => (String::new(), String::new()),
    };
    let detail_style = if quiet {
        Style::new().fg(theme.soft)
    } else {
        Style::new().fg(theme.muted)
    };

    let mut spans = vec![
        Span::styled(marker, Style::new().fg(theme.focus)),
        Span::raw(" "),
        Span::styled(
            status_glyph(model.settings.icons, status),
            Style::new().fg(theme.status(status)),
        ),
        Span::raw(" "),
        Span::styled(
            format!(
                "{indent}{git_icon}{}",
                fit(&name, names.saturating_sub(indent.chars().count()))
            ),
            name_style,
        ),
        Span::styled(fit(&branch, BRANCH_WIDTH), detail_style),
        Span::styled(fit(&counts, COUNTS_WIDTH), detail_style),
    ];
    spans.extend(state_spans(model, report, place.group, status, rest));
    Line::from(spans)
}

/// The state text in the status colour. A closed group says what it hides
/// and shows its own state only when it deviates, shortened so the summary
/// still fits into `rest` columns.
fn state_spans<'a>(
    model: &Model,
    report: &RepoReport,
    group: Option<&Group>,
    status: Status,
    rest: usize,
) -> Vec<Span<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let quiet = status == Status::Neutral;
    let style = Style::new().fg(if quiet {
        theme.soft
    } else {
        theme.status(status)
    });
    let Some(group) = group.filter(|group| !group.open) else {
        return vec![Span::styled(
            super::clip(&state_text(language, &report.state), rest),
            style,
        )];
    };
    let nested = u32::try_from(group.nested).unwrap_or(u32::MAX);
    let mut summary = vec![Span::styled(
        t_count(language, "ws-nested", "count", nested),
        Style::new().fg(theme.soft),
    )];
    summary.extend(deviation_spans(model, &group.deviations));
    if quiet {
        return summary;
    }
    let width: usize = summary
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    let state = super::clip(
        &state_text(language, &report.state),
        rest.saturating_sub(width + 3),
    );
    let mut spans = vec![Span::styled(state, style), Span::raw("   ")];
    spans.extend(summary);
    spans
}

/// Footer line for the last manual fast-forward: running, done, or why not.
pub(super) fn ff_notice(model: &Model) -> Vec<Span<'static>> {
    let Some(notice) = &model.ff_notice else {
        return Vec::new();
    };
    let language = model.settings.language;
    let repo = model.repo_label(&notice.repo);
    let (status, text) = match &notice.result {
        None => (
            Status::Advisory,
            t_args(language, "ff-running", &[("repo", &repo)]),
        ),
        Some(RepoState::UpToDate) => (
            Status::Ok,
            t_args(language, "ff-up-to-date", &[("repo", &repo)]),
        ),
        Some(state @ RepoState::FastForwarded(_)) => (
            Status::Ok,
            t_args(
                language,
                "ff-result",
                &[("repo", &repo), ("result", &state_text(language, state))],
            ),
        ),
        Some(state @ RepoState::Error(_)) => (
            Status::Warning,
            t_args(
                language,
                "ff-result",
                &[("repo", &repo), ("result", &state_text(language, state))],
            ),
        ),
        Some(state @ (RepoState::Behind(_) | RepoState::Skipped(_))) => (
            Status::Caution,
            t_args(
                language,
                "ff-skipped",
                &[("repo", &repo), ("reason", &state_text(language, state))],
            ),
        ),
    };
    let glyph = status_glyph(model.settings.icons, status);
    vec![Span::styled(
        format!("{glyph} {text}"),
        Style::new().fg(model.theme.status(status)),
    )]
}

fn counts_text(ahead: u32, behind: u32) -> String {
    match (ahead, behind) {
        (0, 0) => String::new(),
        (0, b) => format!("↓{b}"),
        (a, 0) => format!("↑{a}"),
        (a, b) => format!("↑{a} ↓{b}"),
    }
}

/// Pads or truncates to a fixed number of characters so columns line up.
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

pub(super) fn state_text(language: Language, state: &RepoState) -> String {
    match state {
        RepoState::UpToDate => t(language, "state-up-to-date"),
        RepoState::Behind(count) => t_count(language, "state-behind", "count", *count),
        RepoState::FastForwarded(count) => {
            t_count(language, "state-fast-forwarded", "count", *count)
        }
        RepoState::Error(message) => t_args(language, "state-error", &[("message", message)]),
        RepoState::Skipped(reason) => match reason {
            SkipReason::OperationInProgress(operation) => {
                let name = t(language, operation_message(*operation));
                t_args(language, "skip-operation", &[("operation", &name)])
            }
            SkipReason::DetachedHead => t(language, "skip-detached"),
            SkipReason::NoCommits => t(language, "skip-no-commits"),
            SkipReason::NoUpstream => t(language, "skip-no-upstream"),
            SkipReason::TrackedChanges => t(language, "skip-tracked"),
            SkipReason::Diverged { .. } => t(language, "skip-diverged"),
            SkipReason::Ahead(count) => t_count(language, "skip-ahead", "count", *count),
            SkipReason::OptedOut => t(language, "skip-opted-out"),
        },
    }
}

fn operation_message(operation: GitOperation) -> &'static str {
    match operation {
        GitOperation::Merge => "op-merge",
        GitOperation::Rebase => "op-rebase",
        GitOperation::CherryPick => "op-cherry-pick",
        GitOperation::Revert => "op-revert",
        GitOperation::Bisect => "op-bisect",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_show_only_what_differs() {
        assert_eq!(counts_text(0, 0), "");
        assert_eq!(counts_text(0, 3), "↓3");
        assert_eq!(counts_text(2, 0), "↑2");
        assert_eq!(counts_text(2, 5), "↑2 ↓5");
    }

    #[test]
    fn fit_pads_and_truncates_to_width() {
        assert_eq!(fit("api", 6), "api   ");
        assert_eq!(fit("very-long-name", 6), "very… ");
    }

    #[test]
    fn every_skip_reason_has_a_text_in_both_languages() {
        let reasons = [
            SkipReason::OperationInProgress(GitOperation::Rebase),
            SkipReason::DetachedHead,
            SkipReason::NoCommits,
            SkipReason::NoUpstream,
            SkipReason::TrackedChanges,
            SkipReason::Diverged {
                ahead: 1,
                behind: 2,
            },
            SkipReason::Ahead(1),
            SkipReason::OptedOut,
        ];
        for language in [Language::De, Language::En] {
            for reason in reasons {
                let text = state_text(language, &RepoState::Skipped(reason));
                assert!(
                    !text.is_empty() && !text.starts_with("skip-"),
                    "{language:?} {reason:?}: {text}"
                );
            }
        }
    }
}
