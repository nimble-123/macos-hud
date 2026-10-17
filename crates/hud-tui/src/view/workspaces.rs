//! Workspaces tab: repositories per workspace with status, reason and selection.

use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_domain::git::{GitOperation, Head, SkipReason};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::Model;
use crate::i18n::{t, t_args, t_count};
use crate::icons::{Source, Status, source_glyph, status_glyph};
use crate::settings::Language;

const NAME_WIDTH: usize = 28;
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

    let mut lines = Vec::new();
    let mut row = 0usize;
    let mut selected_line = 0usize;
    for workspace in &model.workspaces {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        lines.push(header(model, workspace));
        match &workspace.repos {
            None => {}
            Some(repos) if repos.is_empty() => {
                let message = if model.scan.is_some() {
                    "ws-scanning"
                } else {
                    "ws-empty"
                };
                lines.push(Line::styled(
                    format!("  {}", t(language, message)),
                    Style::new().fg(theme.soft),
                ));
            }
            Some(repos) => {
                for report in repos {
                    if row == model.selected {
                        selected_line = lines.len();
                    }
                    lines.push(repo_line(model, workspace, report, row == model.selected));
                    row += 1;
                }
            }
        }
    }

    // Keep the selection visible: scroll so the selected line stays on screen.
    let height = usize::from(area.height).max(1);
    let scroll = selected_line.saturating_sub(height.saturating_sub(2));
    let scroll = u16::try_from(scroll).unwrap_or(u16::MAX);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}

fn header<'a>(model: &Model, workspace: &'a WorkspaceReport) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut spans = vec![Span::styled(
        workspace.workspace.to_string(),
        Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
    )];
    if workspace.unavailable.is_some() {
        let glyph = status_glyph(model.settings.icons, Status::Warning);
        spans.push(Span::styled(
            format!("   {glyph} {}", t(language, "ws-unavailable")),
            Style::new().fg(theme.status(Status::Warning)),
        ));
    } else if !workspace.auto_fast_forward {
        spans.push(Span::styled(
            format!("   {}", t(language, "ws-auto-off")),
            Style::new().fg(theme.soft),
        ));
    }
    Line::from(spans)
}

fn repo_line<'a>(
    model: &Model,
    workspace: &WorkspaceReport,
    report: &'a RepoReport,
    selected: bool,
) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let status = status_of(model, report);
    let quiet = status == Status::Neutral;

    let name = report
        .repo
        .as_path()
        .strip_prefix(workspace.workspace.as_path())
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

    Line::from(vec![
        Span::styled(marker, Style::new().fg(theme.focus)),
        Span::raw(" "),
        Span::styled(
            status_glyph(model.settings.icons, status),
            Style::new().fg(theme.status(status)),
        ),
        Span::raw(" "),
        Span::styled(format!("{git_icon}{}", fit(&name, NAME_WIDTH)), name_style),
        Span::styled(fit(&branch, BRANCH_WIDTH), detail_style),
        Span::styled(fit(&counts, COUNTS_WIDTH), detail_style),
        Span::styled(
            state_text(language, &report.state),
            Style::new().fg(if quiet {
                theme.soft
            } else {
                theme.status(status)
            }),
        ),
    ])
}

fn status_of(model: &Model, report: &RepoReport) -> Status {
    match &report.state {
        RepoState::FastForwarded(_) if model.is_recent_ok(&report.repo) => Status::Ok,
        RepoState::UpToDate | RepoState::FastForwarded(_) => Status::Neutral,
        RepoState::Behind(_) => Status::Advisory,
        RepoState::Skipped(SkipReason::Diverged { .. }) | RepoState::Error(_) => Status::Warning,
        RepoState::Skipped(_) => Status::Caution,
    }
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
