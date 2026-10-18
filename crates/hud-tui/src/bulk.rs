//! "Update everything" from the overview: one confirmation, then a
//! fast-forward for every repository behind and an installation for every
//! package, with one progress until all of them have ended.
//!
//! Each part still passes its own permit in the agents; this only bundles
//! the requests the Workspaces and Updates tabs send one by one.

use std::time::Instant;

use hud_application::usecases::updates::{JobState, JobUpdate};
use hud_application::usecases::workspaces::{RepoReport, RepoState};
use hud_application::{Command, UpdateCommand, WorkspaceCommand};
use hud_domain::git::{RepoPath, SkipReason};
use hud_domain::update::InstallTarget;

use crate::Model;
use crate::model::{Confirm, FF_NOTICE};

/// What `a` on the overview started and what of it is still running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bulk {
    /// Fast-forwards and installations started.
    pub(crate) total: usize,
    /// Repositories whose fast-forward has not reported back yet.
    pub(crate) repos: Vec<RepoPath>,
    /// Packages whose installation has not ended yet.
    pub(crate) targets: Vec<InstallTarget>,
    /// Parts that ended without success.
    pub(crate) problems: usize,
    /// When the last part ended; the result shows for [`FF_NOTICE`].
    pub(crate) finished_at: Option<Instant>,
}

impl Bulk {
    /// Parts that have ended, with or without success.
    pub(crate) fn done(&self) -> usize {
        self.total - self.repos.len() - self.targets.len()
    }

    fn is_running(&self) -> bool {
        self.finished_at.is_none()
    }
}

/// Repositories behind their upstream, also where automatic fast-forward is off.
fn behind(model: &Model) -> Vec<RepoPath> {
    model
        .repo_rows()
        .filter(|report| {
            matches!(
                report.state,
                RepoState::Behind(_) | RepoState::Skipped(SkipReason::OptedOut)
            )
        })
        .map(|report| report.repo.clone())
        .collect()
}

/// `a` on the overview: asks to confirm what would run; nothing to do or a
/// run still going opens no dialog.
pub(crate) fn ask(model: &mut Model) {
    if model.bulk.as_ref().is_some_and(Bulk::is_running) {
        return;
    }
    let repos = behind(model);
    let targets = model.installable_rows();
    if repos.is_empty() && targets.is_empty() {
        return;
    }
    model.confirm = Some(Confirm::Everything { repos, targets });
}

/// Starts every part after the confirmation: one fast-forward per
/// repository and one confirmed installation per package.
pub(crate) fn start(
    model: &mut Model,
    repos: Vec<RepoPath>,
    targets: Vec<InstallTarget>,
) -> Vec<Command> {
    let commands = repos
        .iter()
        .cloned()
        .map(|repo| Command::Workspaces(WorkspaceCommand::FastForward(repo)))
        .chain(targets.iter().cloned().map(|target| {
            Command::Updates(UpdateCommand::Install {
                target,
                confirmed: true,
            })
        }))
        .collect();
    model.bulk = Some(Bulk {
        total: repos.len() + targets.len(),
        repos,
        targets,
        problems: 0,
        finished_at: None,
    });
    commands
}

/// A fast-forward reported back.
pub(crate) fn repo_updated(model: &mut Model, report: &RepoReport) {
    let Some(bulk) = model.bulk.as_mut() else {
        return;
    };
    let Some(index) = bulk.repos.iter().position(|repo| *repo == report.repo) else {
        return;
    };
    bulk.repos.remove(index);
    if !matches!(
        report.state,
        RepoState::FastForwarded(_) | RepoState::UpToDate
    ) {
        bulk.problems += 1;
    }
    finish_if_done(model);
}

/// An installation changed its state.
pub(crate) fn job_changed(model: &mut Model, job: &JobUpdate) {
    if !job.state.is_finished() {
        return;
    }
    let Some(bulk) = model.bulk.as_mut() else {
        return;
    };
    let Some(index) = bulk.targets.iter().position(|target| *target == job.target) else {
        return;
    };
    bulk.targets.remove(index);
    if job.state != JobState::Succeeded {
        bulk.problems += 1;
    }
    finish_if_done(model);
}

fn finish_if_done(model: &mut Model) {
    let now = model.now;
    if let Some(bulk) = model.bulk.as_mut()
        && bulk.repos.is_empty()
        && bulk.targets.is_empty()
        && bulk.is_running()
    {
        bulk.finished_at = now.or_else(|| Some(Instant::now()));
    }
}

/// Clears the result a few seconds after the last part ended.
pub(crate) fn tick(model: &mut Model, now: Instant) {
    if model
        .bulk
        .as_ref()
        .and_then(|bulk| bulk.finished_at)
        .is_some_and(|at| now.duration_since(at) >= FF_NOTICE)
    {
        model.bulk = None;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use hud_application::AppEvent;
    use hud_application::ReportSource;
    use hud_application::ports::updates::{Availability, SourceCheck};
    use hud_application::usecases::updates::{JobId, SourceReport};
    use hud_application::usecases::workspaces::WorkspaceReport;
    use hud_domain::update::{PackageKind, PackageName, SourceId, UpdateCandidate, Version};

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

    fn wget() -> InstallTarget {
        InstallTarget::Package(UpdateCandidate {
            source: SourceId::Homebrew,
            kind: PackageKind::Formula,
            name: PackageName::try_from("wget".to_owned()).expect("BUG: name"),
            current: Version::new("1.24"),
            latest: Version::new("1.25"),
            pinned: false,
        })
    }

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// One repo behind, one opted out, one up to date, and wget to update.
    fn model() -> Model {
        let mut model = Model {
            tab: Tab::Overview,
            ..Model::default()
        };
        update(
            &mut model,
            Msg::Tick(Instant::now(), jiff::Timestamp::now()),
        );
        let workspace = WorkspaceReport {
            workspace: hud_domain::WorkspacePath::try_from(PathBuf::from("/ws"))
                .expect("BUG: absolute"),
            auto_fast_forward: true,
            repos: Some(vec![
                report("/ws/a", RepoState::Behind(2)),
                report("/ws/b", RepoState::Skipped(SkipReason::OptedOut)),
                report("/ws/c", RepoState::UpToDate),
            ]),
            unavailable: None,
        };
        update(
            &mut model,
            Msg::App(AppEvent::Workspaces {
                reports: vec![workspace],
                source: ReportSource::Sync,
            }),
        );
        let InstallTarget::Package(candidate) = wget() else {
            unreachable!("BUG: wget is a package")
        };
        update(
            &mut model,
            Msg::App(AppEvent::Updates(vec![SourceReport {
                source: SourceId::Homebrew,
                availability: Availability::Available("4.6".into()),
                check: Some(Ok(SourceCheck {
                    candidates: vec![candidate],
                    installed: Vec::new(),
                    updates_known: true,
                })),
            }])),
        );
        model
    }

    #[test]
    fn a_asks_once_for_every_repository_behind_and_every_package() {
        let mut model = model();
        assert_eq!(update(&mut model, key(KeyCode::Char('a'))), Vec::new());
        assert_eq!(
            model.confirm,
            Some(Confirm::Everything {
                repos: vec![repo("/ws/a"), repo("/ws/b")],
                targets: vec![wget()],
            })
        );
    }

    #[test]
    fn confirming_starts_every_part_with_its_own_permit_request() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('a')));
        let commands = update(&mut model, key(KeyCode::Enter));
        assert_eq!(
            commands,
            vec![
                Command::Workspaces(WorkspaceCommand::FastForward(repo("/ws/a"))),
                Command::Workspaces(WorkspaceCommand::FastForward(repo("/ws/b"))),
                Command::Updates(UpdateCommand::Install {
                    target: wget(),
                    confirmed: true,
                }),
            ]
        );
        assert_eq!(model.bulk.as_ref().map(Bulk::done), Some(0));
    }

    #[test]
    fn the_run_counts_problems_and_ends_when_every_part_has_reported() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('a')));
        update(&mut model, key(KeyCode::Enter));
        for state in [
            RepoState::FastForwarded(2),
            RepoState::Skipped(SkipReason::TrackedChanges),
        ] {
            let path = if matches!(state, RepoState::FastForwarded(_)) {
                "/ws/a"
            } else {
                "/ws/b"
            };
            update(
                &mut model,
                Msg::App(AppEvent::RepoUpdated(report(path, state))),
            );
        }
        let bulk = model.bulk.clone().expect("BUG: running");
        assert_eq!((bulk.done(), bulk.problems), (2, 1));
        assert!(bulk.finished_at.is_none(), "wget still runs");
        update(
            &mut model,
            Msg::App(AppEvent::Job(JobUpdate {
                id: JobId(1),
                target: wget(),
                state: JobState::Succeeded,
            })),
        );
        assert!(model.bulk.as_ref().is_some_and(|b| b.finished_at.is_some()));
        let later = Instant::now() + Duration::from_secs(6);
        update(&mut model, Msg::Tick(later, jiff::Timestamp::now()));
        assert_eq!(model.bulk, None);
    }

    #[test]
    fn nothing_to_do_opens_no_dialog() {
        let mut model = Model {
            tab: Tab::Overview,
            ..Model::default()
        };
        update(&mut model, key(KeyCode::Char('a')));
        assert_eq!(model.confirm, None);
    }
}
