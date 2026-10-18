//! Workspaces tab: which rows the list shows and which one is selected.
//!
//! The selection is a workspace or repository, not a line number, so it stays
//! on the same entry when rows appear or disappear with a new report.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_application::{Command, WorkspaceCommand};
use hud_domain::WorkspacePath;
use hud_domain::git::{RepoPath, SkipReason};

use crate::Model;
use crate::icons::Status;
use crate::model::wrap_step;

/// State of the Workspaces tab.
#[derive(Debug, Default)]
pub(crate) struct WorkspacesTab {
    /// Selected entry; `None` or one that is not shown means the first row.
    pub(crate) selected: Option<Selection>,
    /// Every repository instead of only those that need attention.
    pub(crate) all: bool,
    /// Groups the user opened (`true`) or closed (`false`), by folder; the
    /// others follow [`opens_by_default`].
    pub(crate) open: HashMap<PathBuf, bool>,
}

/// What can be selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    /// A workspace heading.
    Workspace(WorkspacePath),
    /// A repository.
    Repo(RepoPath),
}

/// A repository with others nested in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    /// Whether the nested repositories are shown.
    pub(crate) open: bool,
    /// Number of nested repositories, at any depth.
    pub(crate) nested: usize,
    /// Nested repositories per deviating status, in [`DEVIATIONS`] order.
    pub(crate) deviations: Vec<(Status, usize)>,
}

/// Statuses that count as a deviation, mildest first.
pub(crate) const DEVIATIONS: [Status; 3] = [Status::Advisory, Status::Caution, Status::Warning];

/// One row of the list.
#[derive(Debug)]
pub(crate) enum Row<'a> {
    /// Heading of a workspace; `open` when its repositories are shown.
    Workspace {
        report: &'a WorkspaceReport,
        open: bool,
    },
    /// A repository below its workspace or nested in another repository.
    Repo {
        report: &'a RepoReport,
        /// The name is shown relative to this folder.
        base: &'a Path,
        /// Number of repositories it is nested in.
        level: usize,
        /// Set when other repositories are nested in it.
        group: Option<Group>,
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
            Row::Workspace { .. } | Row::UpToDate { .. } | Row::Empty => None,
        }
    }

    /// What selecting this row selects; `None` for rows that cannot be selected.
    fn selection(&self) -> Option<Selection> {
        match self {
            Row::Workspace { report, .. } => Some(Selection::Workspace(report.workspace.clone())),
            Row::Repo { report, .. } => Some(Selection::Repo(report.repo.clone())),
            Row::UpToDate { .. } | Row::Empty => None,
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

/// Number of `repos` per deviating status, leaving out statuses without any.
pub(crate) fn deviations<'r>(
    model: &Model,
    repos: impl IntoIterator<Item = &'r RepoReport>,
) -> Vec<(Status, usize)> {
    let statuses: Vec<Status> = repos
        .into_iter()
        .map(|report| status(model, report))
        .collect();
    DEVIATIONS
        .into_iter()
        .map(|wanted| (wanted, statuses.iter().filter(|s| **s == wanted).count()))
        .filter(|(_, count)| *count > 0)
        .collect()
}

/// Handles a key on the Workspaces tab; `None` leaves it to the global keys.
pub(crate) fn on_key(model: &mut Model, key: KeyEvent) -> Option<Vec<Command>> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => move_selection(model, 1),
        KeyCode::Up | KeyCode::Char('k') => move_selection(model, -1),
        KeyCode::Char('v') => model.workspaces_tab.all = !model.workspaces_tab.all,
        KeyCode::Left | KeyCode::Char('h') => close_or_leave(model),
        KeyCode::Right | KeyCode::Char('l') => open(model),
        // Enter does what the row offers: a heading opens or closes, a repo
        // is fast-forwarded, still guarded by the permit rules.
        KeyCode::Enter if toggle_workspace(model) => {}
        KeyCode::Char('f') | KeyCode::Enter => return Some(fast_forward_selected(model)),
        _ => return None,
    }
    Some(Vec::new())
}

/// Requests a manual fast-forward of the selected repo; the footer reports
/// at once that it runs and then what came of it.
fn fast_forward_selected(model: &mut Model) -> Vec<Command> {
    let Some(report) = selected_repo(model) else {
        return Vec::new();
    };
    let repo = report.repo.clone();
    model.ff_notice = Some(crate::model::FfNotice {
        repo: repo.clone(),
        result: None,
        at: None,
    });
    vec![Command::Workspaces(WorkspaceCommand::FastForward(repo))]
}

/// Showing every repository opens every group; otherwise a group opens by
/// default only when something in it deviates.
fn opens_by_default(model: &Model, deviates: bool) -> bool {
    model.workspaces_tab.all || deviates
}

fn is_open(model: &Model, path: &Path, deviates: bool) -> bool {
    model
        .workspaces_tab
        .open
        .get(path)
        .copied()
        .unwrap_or_else(|| opens_by_default(model, deviates))
}

/// Rows in display order. Unless every repository is wanted, a repository is
/// shown when it needs attention or holds one that does, so nested rows
/// keep their context; a closed group hides what is nested in it.
pub(crate) fn rows(model: &Model) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    for workspace in &model.workspaces {
        let repos = workspace.repos.as_deref().unwrap_or_default();
        let attention: Vec<&Path> = repos
            .iter()
            .filter(|report| status(model, report) != Status::Neutral)
            .map(|report| report.repo.as_path())
            .collect();
        let open = is_open(model, workspace.workspace.as_path(), !attention.is_empty());
        rows.push(Row::Workspace {
            report: workspace,
            open,
        });
        if workspace.repos.is_none() || !open {
            continue;
        }
        if repos.is_empty() {
            rows.push(Row::Empty);
            continue;
        }
        let mut hidden = 0;
        let mut closed: Option<&Path> = None;
        // Repos arrive sorted by path, so an outer repository always comes
        // right before the ones nested inside it.
        let mut outer: Vec<&Path> = Vec::new();
        for (index, report) in repos.iter().enumerate() {
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
            if closed.is_some_and(|group| path.starts_with(group)) {
                continue;
            }
            closed = None;
            if !model.workspaces_tab.all && !attention.iter().any(|repo| repo.starts_with(path)) {
                hidden += 1;
                continue;
            }
            let nested: Vec<&RepoReport> = repos[index + 1..]
                .iter()
                .take_while(|inner| inner.repo.as_path().starts_with(path))
                .collect();
            let group = (!nested.is_empty()).then(|| {
                let deviations = deviations(model, nested.iter().copied());
                let open = is_open(model, path, !deviations.is_empty());
                if !open {
                    closed = Some(path);
                }
                Group {
                    open,
                    nested: nested.len(),
                    deviations,
                }
            });
            rows.push(Row::Repo {
                report,
                base,
                level,
                group,
            });
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

/// Index of the selected row in `rows`: the selected entry, else the first
/// repository, else the first workspace.
pub(crate) fn selected_index(model: &Model, rows: &[Row<'_>]) -> Option<usize> {
    let wanted = model.workspaces_tab.selected.as_ref();
    wanted
        .and_then(|wanted| {
            rows.iter()
                .position(|row| row.selection().as_ref() == Some(wanted))
        })
        .or_else(|| rows.iter().position(|row| row.repo().is_some()))
        .or_else(|| rows.iter().position(|row| row.selection().is_some()))
}

/// The selected repository, as the list shows it; `None` on a workspace heading.
pub(crate) fn selected_repo(model: &Model) -> Option<&RepoReport> {
    let rows = rows(model);
    let index = selected_index(model, &rows)?;
    rows.get(index).and_then(Row::repo)
}

/// Moves the selection by `delta` rows that can be selected, wrapping at both ends.
pub(crate) fn move_selection(model: &mut Model, delta: isize) {
    let rows = rows(model);
    let selectable: Vec<usize> = (0..rows.len())
        .filter(|index| rows[*index].selection().is_some())
        .collect();
    let current = selected_index(model, &rows)
        .and_then(|index| selectable.iter().position(|row| *row == index))
        .unwrap_or(0);
    let next = selectable
        .get(wrap_step(current, delta, selectable.len()))
        .and_then(|index| rows[*index].selection());
    model.workspaces_tab.selected = next;
}

/// `←`: closes the selected group, else moves to the row it belongs to.
pub(crate) fn close_or_leave(model: &mut Model) {
    let rows = rows(model);
    let Some(row) = selected_index(model, &rows).and_then(|index| rows.get(index)) else {
        return;
    };
    let (path, selection) = match row {
        Row::Workspace { report, open } => (open.then(|| report.workspace.as_path()), None),
        Row::Repo {
            report,
            base,
            level,
            group,
        } => {
            if group.as_ref().is_some_and(|group| group.open) {
                (Some(report.repo.as_path()), None)
            } else if *level > 0 {
                let parent = RepoPath::try_from(base.to_path_buf()).ok();
                (None, parent.map(Selection::Repo))
            } else {
                let workspace = model
                    .workspaces
                    .iter()
                    .find(|w| report.repo.as_path().starts_with(w.workspace.as_path()));
                (
                    None,
                    workspace.map(|w| Selection::Workspace(w.workspace.clone())),
                )
            }
        }
        Row::UpToDate { .. } | Row::Empty => (None, None),
    };
    let path = path.map(Path::to_path_buf);
    if let Some(path) = path {
        model.workspaces_tab.open.insert(path, false);
    }
    if selection.is_some() {
        model.workspaces_tab.selected = selection;
    }
}

/// `→`: opens the selected group.
pub(crate) fn open(model: &mut Model) {
    if let Some((path, false)) = selected_group(model) {
        model.workspaces_tab.open.insert(path, true);
    }
}

/// `Enter` on a workspace heading opens or closes it; `false` when a
/// repository is selected, so Enter keeps meaning fast-forward there.
pub(crate) fn toggle_workspace(model: &mut Model) -> bool {
    let rows = rows(model);
    let Some(Row::Workspace { report, open }) =
        selected_index(model, &rows).and_then(|index| rows.get(index))
    else {
        return false;
    };
    let path = report.workspace.as_path().to_path_buf();
    let open = *open;
    model.workspaces_tab.open.insert(path, !open);
    true
}

/// Folder and state of the selected workspace or repository group.
fn selected_group(model: &Model) -> Option<(PathBuf, bool)> {
    let rows = rows(model);
    match selected_index(model, &rows).and_then(|index| rows.get(index))? {
        Row::Workspace { report, open } => Some((report.workspace.as_path().to_path_buf(), *open)),
        Row::Repo {
            report,
            group: Some(group),
            ..
        } => Some((report.repo.as_path().to_path_buf(), group.open)),
        Row::Repo { group: None, .. } | Row::UpToDate { .. } | Row::Empty => None,
    }
}

/// A click on row `index`: the first selects it, a second on the selected
/// row is `true` and acts like Enter.
pub(crate) fn click(model: &mut Model, index: usize) -> bool {
    let rows = rows(model);
    let Some(selection) = rows.get(index).and_then(Row::selection) else {
        return false;
    };
    if selected_index(model, &rows) == Some(index) {
        return true;
    }
    model.workspaces_tab.selected = Some(selection);
    false
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hud_application::usecases::workspaces::WorkspaceReport;
    use hud_application::{AppEvent, ReportSource};

    use super::*;
    use crate::{Msg, Tab, update};

    fn repo(path: &str) -> RepoPath {
        RepoPath::try_from(PathBuf::from(path)).expect("BUG: absolute")
    }

    fn report(path: &str, state: RepoState) -> RepoReport {
        RepoReport {
            repo: repo(path),
            status: None,
            state,
        }
    }

    fn workspace(repos: Vec<RepoReport>) -> AppEvent {
        AppEvent::Workspaces {
            reports: vec![WorkspaceReport {
                workspace: WorkspacePath::try_from(PathBuf::from("/ws")).expect("BUG: absolute"),
                auto_fast_forward: true,
                repos: Some(repos),
                unavailable: None,
            }],
            source: ReportSource::Sync,
        }
    }

    fn model(repos: Vec<RepoReport>) -> Model {
        let mut model = Model {
            tab: Tab::Workspaces,
            ..Model::default()
        };
        update(&mut model, Msg::App(workspace(repos)));
        model
    }

    fn shown(model: &Model) -> Vec<String> {
        rows(model)
            .iter()
            .map(|row| match row {
                Row::Workspace { report, open } => format!("{} open: {open}", report.workspace),
                Row::Repo { report, group, .. } => match group {
                    Some(group) => format!("{} open: {}", report.repo, group.open),
                    None => report.repo.to_string(),
                },
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
            ["/ws open: true", "/ws/b", "2 up to date, every: false"]
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
            [
                "/ws open: true",
                "/ws/a open: true",
                "/ws/a/inner",
                "1 up to date, every: false"
            ]
        );
    }

    #[test]
    fn a_workspace_without_deviations_starts_closed_unless_all_are_shown() {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/b", RepoState::UpToDate),
        ]);
        assert_eq!(shown(&model), ["/ws open: false"]);
        model.workspaces_tab.all = true;
        assert_eq!(shown(&model), ["/ws open: true", "/ws/a", "/ws/b"]);
    }

    #[test]
    fn the_selection_stays_on_its_repository_when_rows_above_disappear() {
        let mut model = model(vec![
            report("/ws/a", RepoState::Behind(1)),
            report("/ws/b", RepoState::Behind(1)),
        ]);
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/b")));
        update(
            &mut model,
            Msg::App(workspace(vec![
                report("/ws/a", RepoState::UpToDate),
                report("/ws/b", RepoState::Behind(1)),
            ])),
        );
        assert_eq!(
            selected_repo(&model).map(|r| r.repo.to_string()),
            Some("/ws/b".to_owned())
        );
    }

    #[test]
    fn a_hidden_selection_falls_back_to_the_first_repository() {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/b", RepoState::Behind(1)),
        ]);
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/a")));
        assert_eq!(
            selected_repo(&model).map(|r| r.repo.to_string()),
            Some("/ws/b".to_owned())
        );
    }

    fn group_model() -> Model {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/a/one", RepoState::UpToDate),
            report("/ws/a/two", RepoState::UpToDate),
            report("/ws/b", RepoState::Behind(1)),
        ]);
        model.workspaces_tab.all = true;
        model
    }

    #[test]
    fn left_closes_a_group_and_right_opens_it_again() {
        let mut model = group_model();
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/a")));
        close_or_leave(&mut model);
        assert_eq!(
            shown(&model),
            ["/ws open: true", "/ws/a open: false", "/ws/b"]
        );
        open(&mut model);
        assert_eq!(shown(&model).len(), 5);
    }

    #[test]
    fn a_closed_group_counts_its_nested_deviations() {
        let mut model = model(vec![
            report("/ws/a", RepoState::UpToDate),
            report("/ws/a/one", RepoState::Error("x".into())),
            report("/ws/a/two", RepoState::Behind(1)),
        ]);
        model
            .workspaces_tab
            .open
            .insert(PathBuf::from("/ws/a"), false);
        let rows = rows(&model);
        let Some(Row::Repo {
            group: Some(group), ..
        }) = rows.get(1)
        else {
            panic!("expected the group row, got {rows:?}")
        };
        assert_eq!(group.nested, 2);
        assert_eq!(
            group.deviations,
            [(Status::Advisory, 1), (Status::Warning, 1)]
        );
    }

    #[test]
    fn left_on_a_nested_repository_moves_to_its_parent_then_to_the_workspace() {
        let mut model = group_model();
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/a/one")));
        close_or_leave(&mut model);
        assert_eq!(
            model.workspaces_tab.selected,
            Some(Selection::Repo(repo("/ws/a")))
        );
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/b")));
        close_or_leave(&mut model);
        assert!(matches!(
            model.workspaces_tab.selected,
            Some(Selection::Workspace(_))
        ));
    }

    #[test]
    fn enter_on_a_workspace_heading_closes_and_opens_it() {
        let mut model = group_model();
        model.workspaces_tab.selected = Some(Selection::Workspace(
            WorkspacePath::try_from(PathBuf::from("/ws")).expect("BUG"),
        ));
        assert!(toggle_workspace(&mut model));
        assert_eq!(shown(&model), ["/ws open: false"]);
        assert!(toggle_workspace(&mut model));
        assert_eq!(shown(&model).len(), 5);
    }

    #[test]
    fn enter_on_a_repository_is_left_to_the_fast_forward() {
        let mut model = group_model();
        model.workspaces_tab.selected = Some(Selection::Repo(repo("/ws/b")));
        assert!(!toggle_workspace(&mut model));
    }
}
