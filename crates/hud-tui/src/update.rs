//! State transitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hud_application::{AppEvent, Command, UpdateCommand, WorkspaceCommand};

use crate::model::{OK_HIGHLIGHT, ScanProgress};
use crate::{Model, Msg, Tab};

/// Applies a message to the model and returns the commands to run.
///
/// Pure: no I/O and no clock, so every transition is unit-testable.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Command> {
    match msg {
        Msg::Key(key) => return on_key(model, key),
        Msg::App(event) => on_app_event(model, event),
        Msg::Tick(now) => {
            model.now = Some(now);
            model.ticks = model.ticks.wrapping_add(1);
            model
                .recent_ok
                .retain(|_, at| now.duration_since(*at) < OK_HIGHLIGHT);
        }
        Msg::Mouse(_) | Msg::Resize => {}
    }
    Vec::new()
}

fn on_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Tab => model.tab = model.tab.next(),
        KeyCode::BackTab => model.tab = model.tab.previous(),
        KeyCode::Char('q') => model.quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => model.quit = true,
        KeyCode::Char('r') if model.tab == Tab::Updates => {
            return vec![Command::Updates(UpdateCommand::CheckNow)];
        }
        KeyCode::Char('r') if !model.syncing => {
            return vec![Command::Workspaces(WorkspaceCommand::SyncNow)];
        }
        KeyCode::Down | KeyCode::Char('j') if model.tab == Tab::Workspaces => {
            move_selection(model, 1);
        }
        KeyCode::Up | KeyCode::Char('k') if model.tab == Tab::Workspaces => {
            move_selection(model, -1);
        }
        KeyCode::Char('f') if model.tab == Tab::Workspaces => {
            if let Some(report) = model.selected_repo() {
                return vec![Command::Workspaces(WorkspaceCommand::FastForward(
                    report.repo.clone(),
                ))];
            }
        }
        _ => {}
    }
    Vec::new()
}

fn move_selection(model: &mut Model, delta: isize) {
    let count = model.repo_rows().count();
    if count == 0 {
        return;
    }
    model.selected = model.selected.saturating_add_signed(delta).min(count - 1);
}

fn on_app_event(model: &mut Model, event: AppEvent) {
    match event {
        AppEvent::Workspaces { reports, source: _ } => {
            let finished: Vec<_> = reports
                .iter()
                .filter_map(|w| w.repos.clone())
                .flatten()
                .collect();
            model.workspaces = reports;
            model.syncing = false;
            model.scan = None;
            model.mark_fast_forwards(&finished);
            let count = model.repo_rows().count();
            model.selected = model.selected.min(count.saturating_sub(1));
        }
        AppEvent::SyncStarted => model.syncing = true,
        AppEvent::ScanStarted { total, fetch } => {
            model.scan = Some(ScanProgress {
                done: 0,
                total,
                fetch,
            });
        }
        AppEvent::RepoScanned { workspace, report } => {
            if let Some(scan) = &mut model.scan {
                scan.done = (scan.done + 1).min(scan.total);
            }
            model.mark_fast_forwards(std::slice::from_ref(&report));
            if let Some(entry) = model
                .workspaces
                .iter_mut()
                .find(|w| w.workspace == workspace)
            {
                let repos = entry.repos.get_or_insert_with(Vec::new);
                match repos.binary_search_by(|existing| existing.repo.cmp(&report.repo)) {
                    Ok(position) => repos[position] = report,
                    Err(position) => repos.insert(position, report),
                }
            }
        }
        AppEvent::Updates(reports) => model.updates = reports,
        AppEvent::SourceUpdated(report) => {
            match model
                .updates
                .iter_mut()
                .find(|existing| existing.source == report.source)
            {
                Some(existing) => *existing = report,
                None => model.updates.push(report),
            }
        }
        AppEvent::Job(job) => {
            if job.state.is_finished() {
                model.jobs.remove(&job.id);
                model.finished_jobs.insert(job.id, job);
            } else {
                model.jobs.insert(job.id, job);
            }
        }
        AppEvent::RepoUpdated(report) => {
            model.mark_fast_forwards(std::slice::from_ref(&report));
            let slot = model
                .workspaces
                .iter_mut()
                .filter_map(|w| w.repos.as_mut())
                .flatten()
                .find(|r| r.repo == report.repo);
            if let Some(slot) = slot {
                *slot = report;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tab;

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_moves_to_next_tab() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Tab));
        assert_eq!(model.tab(), Tab::Workspaces);
    }

    #[test]
    fn back_tab_moves_to_previous_tab() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::BackTab));
        assert_eq!(model.tab(), Tab::Settings);
    }

    #[test]
    fn q_requests_quit() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Char('q')));
        assert!(model.should_quit());
    }

    #[test]
    fn ctrl_c_requests_quit() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(model.should_quit());
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Char('c')));
        assert!(!model.should_quit());
    }

    #[test]
    fn tick_changes_neither_tab_nor_quit() {
        let mut model = Model::default();
        update(&mut model, Msg::Tick(Instant::now()));
        assert_eq!(model.tab(), Tab::Overview);
        assert!(!model.should_quit());
    }

    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use hud_application::ReportSource;
    use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
    use hud_domain::WorkspacePath;
    use hud_domain::git::RepoPath;

    fn report(path: &str, state: RepoState) -> RepoReport {
        RepoReport {
            repo: RepoPath::try_from(PathBuf::from(path)).expect("BUG"),
            status: None,
            state,
        }
    }

    fn workspaces(repos: Vec<RepoReport>) -> AppEvent {
        AppEvent::Workspaces {
            reports: vec![WorkspaceReport {
                workspace: WorkspacePath::try_from(PathBuf::from("/ws")).expect("BUG"),
                auto_fast_forward: true,
                repos: Some(repos),
                unavailable: None,
            }],
            source: ReportSource::Sync,
        }
    }

    fn on_workspaces_tab() -> Model {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Tab));
        model
    }

    #[test]
    fn r_requests_a_sync_unless_one_is_running() {
        let mut model = Model::default();
        assert_eq!(
            update(&mut model, key(KeyCode::Char('r'))),
            vec![Command::Workspaces(WorkspaceCommand::SyncNow)]
        );
        update(&mut model, Msg::App(AppEvent::SyncStarted));
        assert_eq!(update(&mut model, key(KeyCode::Char('r'))), Vec::new());
    }

    #[test]
    fn f_fast_forwards_the_selected_repository() {
        let mut model = on_workspaces_tab();
        update(
            &mut model,
            Msg::App(workspaces(vec![
                report("/ws/a", RepoState::UpToDate),
                report("/ws/b", RepoState::Behind(2)),
            ])),
        );
        update(&mut model, key(KeyCode::Down));
        let commands = update(&mut model, key(KeyCode::Char('f')));
        assert_eq!(
            commands,
            vec![Command::Workspaces(WorkspaceCommand::FastForward(
                RepoPath::try_from(PathBuf::from("/ws/b")).expect("BUG")
            ))]
        );
    }

    #[test]
    fn selection_stays_within_the_list() {
        let mut model = on_workspaces_tab();
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::UpToDate)])),
        );
        update(&mut model, key(KeyCode::Down));
        update(&mut model, key(KeyCode::Down));
        assert_eq!(model.selected, 0);
        update(&mut model, key(KeyCode::Up));
        assert_eq!(model.selected, 0);
    }

    #[test]
    fn f_outside_the_workspaces_tab_does_nothing() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::Behind(1))])),
        );
        assert_eq!(update(&mut model, key(KeyCode::Char('f'))), Vec::new());
    }

    #[test]
    fn fast_forward_highlight_expires_after_thirty_seconds() {
        let mut model = Model::default();
        let start = Instant::now();
        update(&mut model, Msg::Tick(start));
        update(
            &mut model,
            Msg::App(workspaces(vec![report(
                "/ws/a",
                RepoState::FastForwarded(3),
            )])),
        );
        let repo = RepoPath::try_from(PathBuf::from("/ws/a")).expect("BUG");
        assert!(model.is_recent_ok(&repo));
        update(&mut model, Msg::Tick(start + Duration::from_secs(31)));
        assert!(!model.is_recent_ok(&repo));
    }

    #[test]
    fn workspace_report_ends_the_sync_indicator() {
        let mut model = Model::default();
        update(&mut model, Msg::App(AppEvent::SyncStarted));
        update(&mut model, Msg::App(workspaces(vec![])));
        assert!(!model.syncing);
    }

    #[test]
    fn scan_progress_counts_and_inserts_repositories_in_order() {
        let mut model = Model::default();
        update(&mut model, Msg::App(workspaces(vec![])));
        update(
            &mut model,
            Msg::App(AppEvent::ScanStarted {
                total: 2,
                fetch: false,
            }),
        );
        let ws = WorkspacePath::try_from(PathBuf::from("/ws")).expect("BUG");
        for path in ["/ws/b", "/ws/a"] {
            update(
                &mut model,
                Msg::App(AppEvent::RepoScanned {
                    workspace: ws.clone(),
                    report: report(path, RepoState::UpToDate),
                }),
            );
        }
        assert_eq!(model.scan.map(|s| (s.done, s.total)), Some((2, 2)));
        let order: Vec<_> = model.repo_rows().map(|r| r.repo.to_string()).collect();
        assert_eq!(order, vec!["/ws/a", "/ws/b"]);
    }

    #[test]
    fn complete_report_ends_the_scan_progress() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::App(AppEvent::ScanStarted {
                total: 1,
                fetch: true,
            }),
        );
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::UpToDate)])),
        );
        assert_eq!(model.scan, None);
    }

    #[test]
    fn manual_result_replaces_only_that_repository() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::App(workspaces(vec![
                report("/ws/a", RepoState::Behind(1)),
                report("/ws/b", RepoState::Behind(2)),
            ])),
        );
        update(
            &mut model,
            Msg::App(AppEvent::RepoUpdated(report(
                "/ws/a",
                RepoState::FastForwarded(1),
            ))),
        );
        let states: Vec<_> = model.repo_rows().map(|r| r.state.clone()).collect();
        assert_eq!(
            states,
            vec![RepoState::FastForwarded(1), RepoState::Behind(2)]
        );
    }
}
