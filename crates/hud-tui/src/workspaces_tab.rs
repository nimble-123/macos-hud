//! Workspaces tab: which rows the list shows and which one is selected.
//!
//! The selection is a repository, not a line number, so it stays on the same
//! repository when rows appear or disappear with a new report.

use std::path::Path;

use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_domain::git::{RepoPath, SkipReason};

use crate::Model;
use crate::icons::Status;
use crate::model::wrap_step;

/// State of the Workspaces tab.
#[derive(Debug, Default)]
pub(crate) struct WorkspacesTab {
    /// Selected repository; `None` or one that is not shown means the first row.
    pub(crate) selected: Option<RepoPath>,
    /// Every repository instead of only those that need attention.
    pub(crate) all: bool,
}

/// One row of the list.
#[derive(Debug)]
pub(crate) enum Row<'a> {
    /// Heading of a workspace.
    Workspace(&'a WorkspaceReport),
    /// A repository below its workspace or nested in another repository.
    Repo {
        report: &'a RepoReport,
        /// The name is shown relative to this folder.
        base: &'a Path,
        /// Number of repositories it is nested in.
        level: usize,
    },
    /// Up-to-date repositories of a workspace that are left out; `every`
    /// when that is all of them.
    UpToDate { count: usize, every: bool },
    /// The workspace holds no repositories, or the scan has not found any yet.
    Empty,
}

impl<'a> Row<'a> {
    /// The repository a row shows, if it shows one.
    pub(crate) fn repo(&self) -> Option<&'a RepoReport> {
        match self {
            Row::Repo { report, .. } => Some(report),
            Row::Workspace(_) | Row::UpToDate { .. } | Row::Empty => None,
        }
    }
}

/// Status of a repository as the list shows it.
pub(crate) fn status(model: &Model, report: &RepoReport) -> Status {
    match &report.state {
        RepoState::FastForwarded(_) if model.is_recent_ok(&report.repo) => Status::Ok,
        RepoState::UpToDate | RepoState::FastForwarded(_) => Status::Neutral,
        RepoState::Behind(_) => Status::Advisory,
        RepoState::Skipped(SkipReason::Diverged { .. }) | RepoState::Error(_) => Status::Warning,
        RepoState::Skipped(_) => Status::Caution,
    }
}

/// Rows in display order. Unless every repository is wanted, a repository is
/// shown when it needs attention or holds one that does, so nested rows
/// keep their context.
pub(crate) fn rows(model: &Model) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    for workspace in &model.workspaces {
        rows.push(Row::Workspace(workspace));
        let Some(repos) = &workspace.repos else {
            continue;
        };
        if repos.is_empty() {
            rows.push(Row::Empty);
            continue;
        }
        let attention: Vec<&Path> = repos
            .iter()
            .filter(|report| status(model, report) != Status::Neutral)
            .map(|report| report.repo.as_path())
            .collect();
        let mut hidden = 0;
        // Repos arrive sorted by path, so an outer repository always comes
        // right before the ones nested inside it.
        let mut outer: Vec<&Path> = Vec::new();
        for report in repos {
            let path = report.repo.as_path();
            while outer.last().is_some_and(|parent| !path.starts_with(parent)) {
                outer.pop();
            }
            let base = outer
                .last()
                .copied()
                .unwrap_or(workspace.workspace.as_path());
            let level = outer.len();
            outer.push(path);
            if model.workspaces_tab.all || attention.iter().any(|repo| repo.starts_with(path)) {
                rows.push(Row::Repo {
                    report,
                    base,
                    level,
                });
            } else {
                hidden += 1;
            }
        }
        if hidden > 0 {
            rows.push(Row::UpToDate {
                count: hidden,
                every: hidden == repos.len(),
            });
        }
    }
    rows
}

/// Index of the selected row in `rows`: the selected repository, else the first one.
pub(crate) fn selected_index(model: &Model, rows: &[Row<'_>]) -> Option<usize> {
    let wanted = model.workspaces_tab.selected.as_ref();
    wanted
        .and_then(|repo| {
            rows.iter()
                .position(|row| row.repo().is_some_and(|report| report.repo == *repo))
        })
        .or_else(|| rows.iter().position(|row| row.repo().is_some()))
}

/// The selected repository, as the list shows it.
pub(crate) fn selected_repo(model: &Model) -> Option<&RepoReport> {
    let rows = rows(model);
    let index = selected_index(model, &rows)?;
    rows.get(index).and_then(Row::repo)
}

/// Moves the selection by `delta` repositories, wrapping at both ends.
pub(crate) fn move_selection(model: &mut Model, delta: isize) {
    let rows = rows(model);
    let repos: Vec<&RepoPath> = rows
        .iter()
        .filter_map(|row| row.repo().map(|report| &report.repo))
        .collect();
    let current = selected_index(model, &rows)
        .and_then(|index| rows.get(index).and_then(Row::repo))
        .and_then(|report| repos.iter().position(|repo| **repo == report.repo))
        .unwrap_or(0);
    let next = repos
        .get(wrap_step(current, delta, repos.len()))
        .map(|repo| (*repo).clone());
    model.workspaces_tab.selected = next;
}

/// A click on row `index`: the first selects the repository, a second on the
/// selected one is `true` and acts like Enter.
pub(crate) fn click(model: &mut Model, index: usize) -> bool {
    let rows = rows(model);
    let Some(report) = rows.get(index).and_then(Row::repo) else {
        return false;
    };
    if selected_index(model, &rows) == Some(index) {
        return true;
    }
    let repo = report.repo.clone();
    model.workspaces_tab.selected = Some(repo);
    false
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hud_application::usecases::workspaces::WorkspaceReport;
    use hud_application::{AppEvent, ReportSource};

    use super::*;
    use crate::{Msg, Tab, update};

    fn report(path: &str, state: RepoState) -> RepoReport {
        RepoReport {
            repo: RepoPath::try_from(PathBuf::from(path)).expect("BUG: absolute"),
            status: None,
            state,
        }
    }

    fn model(repos: Vec<RepoReport>) -> Model {
        let mut model = Model {
            tab: Tab::Workspaces,
            ..Model::default()
        };
        let workspace = WorkspaceReport {
            workspace: hud_domain::WorkspacePath::try_from(PathBuf::from("/ws"))
                .expect("BUG: absolute"),
            auto_fast_forward: true,
            repos: Some(repos),
            unavailable: None,
        };
        update(
            &mut model,
            Msg::App(AppEvent::Workspaces {
                reports: vec![workspace],
                source: ReportSource::Sync,
            }),
        );
        model
    }

    fn shown(model: &Model) -> Vec<String> {
        rows(model)
            .iter()
            .map(|row| match row {
                Row::Workspace(workspace) => workspace.workspace.to_string(),
                Row::Repo { report, .. } => report.repo.to_string(),
                Row::UpToDate { count, every } => format!("{count} up to date, every: {every}"),
                Row::Empty => "empty".into(),
            })
            .collect()
    }

    #[test]
    fn up_to_date_repositories_are_summed_up() {
        let model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/b", RepoState::Behind(1)),
            report("/ws/c", RepoState::UpToDate),
        ]);
        assert_eq!(
            shown(&model),
            ["/ws", "/ws/b", "2 up to date, every: false"]
        );
    }

    #[test]
    fn a_repository_holding_one_that_needs_attention_stays_for_context() {
        let model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/a/inner", RepoState::Error("x".into())),
            report("/ws/a/other", RepoState::UpToDate),
        ]);
        assert_eq!(
            shown(&model),
            ["/ws", "/ws/a", "/ws/a/inner", "1 up to date, every: false"]
        );
    }

    #[test]
    fn showing_all_lists_every_repository() {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/b", RepoState::UpToDate),
        ]);
        assert_eq!(shown(&model), ["/ws", "2 up to date, every: true"]);
        model.workspaces_tab.all = true;
        assert_eq!(shown(&model), ["/ws", "/ws/a", "/ws/b"]);
    }

    #[test]
    fn the_selection_stays_on_its_repository_when_rows_above_disappear() {
        let mut model = model(vec![
            report("/ws/a", RepoState::Behind(1)),
            report("/ws/b", RepoState::Behind(1)),
        ]);
        move_selection(&mut model, 1);
        let mut model_after = model;
        let workspace = WorkspaceReport {
            workspace: hud_domain::WorkspacePath::try_from(PathBuf::from("/ws"))
                .expect("BUG: absolute"),
            auto_fast_forward: true,
            repos: Some(vec![
                report("/ws/a", RepoState::UpToDate),
                report("/ws/b", RepoState::Behind(1)),
            ]),
            unavailable: None,
        };
        update(
            &mut model_after,
            Msg::App(AppEvent::Workspaces {
                reports: vec![workspace],
                source: ReportSource::Sync,
            }),
        );
        assert_eq!(
            selected_repo(&model_after).map(|r| r.repo.to_string()),
            Some("/ws/b".to_owned())
        );
    }

    #[test]
    fn a_hidden_selection_falls_back_to_the_first_row() {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/b", RepoState::Behind(1)),
        ]);
        model.workspaces_tab.selected =
            Some(RepoPath::try_from(PathBuf::from("/ws/a")).expect("BUG"));
        assert_eq!(
            selected_repo(&model).map(|r| r.repo.to_string()),
            Some("/ws/b".to_owned())
        );
    }
}
