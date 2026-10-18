//! State transitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hud_application::ports::telemetry::ClaudeTelemetry;
use hud_application::{
    AppEvent, Command, ReportSource, TelemetryCommand, UpdateCommand, WorkspaceCommand,
};

use hud_domain::update::InstallTarget;
use hud_domain::usage::Period;

use crate::model::{Confirm, OK_HIGHLIGHT, Quit, ScanProgress, TelemetryFlow, wrap_step};
use crate::{Model, Msg, Tab};

/// Applies a message to the model and returns the commands to run.
///
/// Pure: no I/O and no clock, so every transition is unit-testable.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Command> {
    match msg {
        Msg::Key(key) => return on_key(model, key),
        Msg::App(event) => {
            let reload = model.tab == Tab::Log && crate::log_tab::adds_entries(&event);
            on_app_event(model, event);
            if reload {
                return vec![crate::log_tab::load()];
            }
        }
        Msg::Tick(now, wall) => {
            model.now = Some(now);
            model.wall = Some(wall);
            model.ticks = model.ticks.wrapping_add(1);
            model
                .recent_ok
                .retain(|_, at| now.duration_since(*at) < OK_HIGHLIGHT);
            if model
                .ff_notice
                .as_ref()
                .and_then(|notice| notice.at)
                .is_some_and(|at| now.duration_since(at) >= crate::model::FF_NOTICE)
            {
                model.ff_notice = None;
            }
        }
        Msg::Mouse(event) => return crate::mouse::on_mouse(model, event),
        Msg::Resize(columns, rows) => model.area = ratatui::layout::Rect::new(0, 0, columns, rows),
    }
    Vec::new()
}

fn on_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    // Ctrl-C always reaches the quit logic, whatever has the focus.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return request_quit(model);
    }
    if model.quit == Quit::Asking {
        return on_quit_key(model, key);
    }
    // The overlay takes all keys; any key closes it, nothing else happens.
    if model.help {
        model.help = false;
        return Vec::new();
    }
    if model.confirm.is_some() {
        return on_confirm_key(model, key);
    }
    if model.telemetry_flow == TelemetryFlow::Confirming {
        return on_telemetry_key(model, key);
    }
    if model.tab == Tab::Settings
        && let Some(commands) = crate::settings_tab::on_key(model, key)
    {
        return commands;
    }
    if model.tab == Tab::Log
        && let Some(commands) = crate::log_tab::on_key(model, key)
    {
        return commands;
    }
    if model.tab == Tab::Workspaces
        && let Some(commands) = crate::workspaces_tab::on_key(model, key)
    {
        return commands;
    }
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            model.tab = if key.code == KeyCode::Tab {
                model.tab.next()
            } else {
                model.tab.previous()
            };
            // The log changes in the background; read it whenever it is shown.
            if model.tab == Tab::Log {
                return vec![crate::log_tab::load()];
            }
        }
        KeyCode::Char('q') => return request_quit(model),
        KeyCode::Char('?') => model.help = true,
        KeyCode::Char('r') if model.tab == Tab::Updates => {
            if model.checking_updates {
                return Vec::new();
            }
            model.checking_updates = true;
            return vec![Command::Updates(UpdateCommand::CheckNow)];
        }
        KeyCode::Char('r') if !model.syncing => {
            return vec![Command::Workspaces(WorkspaceCommand::SyncNow)];
        }
        KeyCode::Down | KeyCode::Char('j') if model.tab == Tab::Updates => {
            move_update_selection(model, 1);
        }
        KeyCode::Up | KeyCode::Char('k') if model.tab == Tab::Updates => {
            move_update_selection(model, -1);
        }
        KeyCode::Char('u') | KeyCode::Enter if model.tab == Tab::Updates => {
            ask_install_selected(model);
        }
        KeyCode::Char('a') if model.tab == Tab::Updates => ask_install_all(model),
        KeyCode::Down | KeyCode::Char('j') if scrolls_freely(model) => scroll_by(model, 1),
        KeyCode::Up | KeyCode::Char('k') if scrolls_freely(model) => scroll_by(model, -1),
        KeyCode::PageDown if scrolls_freely(model) => {
            scroll_by(model, i32::from(crate::view::page(model)));
        }
        KeyCode::PageUp if scrolls_freely(model) => {
            scroll_by(model, -i32::from(crate::view::page(model)));
        }
        KeyCode::Home if scrolls_freely(model) => scroll_by(model, i32::MIN / 2),
        KeyCode::End if scrolls_freely(model) => scroll_by(model, i32::MAX / 2),
        KeyCode::Right | KeyCode::Char('l') if model.tab == Tab::Ai => step_period(model, 1),
        KeyCode::Left | KeyCode::Char('h') if model.tab == Tab::Ai => step_period(model, -1),
        KeyCode::Char('t') if model.tab == Tab::Ai && telemetry_can_be_enabled(model) => {
            model.telemetry_flow = TelemetryFlow::Confirming;
        }
        KeyCode::Char(c @ '1'..='5') if model.tab == Tab::Ai => {
            let index = usize::from(u8::try_from(c).unwrap_or(b'1') - b'1');
            model.ai_period = Period::ALL[index];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the dialog for the selected row, unless it is pinned.
fn ask_install_selected(model: &mut Model) {
    let rows = model.update_rows();
    let selectable = rows
        .get(model.update_selected)
        .filter(|target| match target {
            InstallTarget::Package(candidate) => !candidate.pinned,
            InstallTarget::AllSkills => true,
        });
    model.confirm = selectable.cloned().map(Confirm::One);
}

/// Opens the dialog for every row that can be installed now, if any.
fn ask_install_all(model: &mut Model) {
    let targets = model.installable_rows();
    if !targets.is_empty() {
        model.confirm = Some(Confirm::All(targets));
    }
}

/// Overview and AI tab show more than fits and have no selection to follow.
fn scrolls_freely(model: &Model) -> bool {
    matches!(model.tab, Tab::Overview | Tab::Ai)
}

/// Moves the visible part by `lines`, never past the content.
fn scroll_by(model: &mut Model, lines: i32) {
    let max = crate::view::max_scroll(model);
    let offset = match model.tab {
        Tab::Overview => &mut model.overview_scroll,
        Tab::Ai => &mut model.ai_scroll,
        Tab::Workspaces | Tab::Updates | Tab::Log | Tab::Settings => return,
    };
    let next = (i32::from((*offset).min(max)) + lines).clamp(0, i32::from(max));
    *offset = u16::try_from(next).unwrap_or(0);
}

/// Quits at once unless installations run; then the dialog asks first. A
/// second request while waiting or asking quits for good.
fn request_quit(model: &mut Model) -> Vec<Command> {
    model.quit = match model.quit {
        Quit::No if !model.jobs.is_empty() => Quit::Asking,
        Quit::No | Quit::Asking | Quit::Waiting | Quit::Now => Quit::Now,
    };
    Vec::new()
}

/// The dialog takes all keys: Enter waits (the default), `a` cancels, Esc stays.
fn on_quit_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Enter | KeyCode::Char('w') => {
            model.quit = Quit::Waiting;
            return vec![Command::Updates(UpdateCommand::Drain)];
        }
        KeyCode::Char('a') => model.quit = Quit::Now,
        KeyCode::Esc | KeyCode::Char('n') => model.quit = Quit::No,
        _ => {}
    }
    Vec::new()
}

/// The dialog takes all keys: Enter installs, Esc closes, everything else is ignored.
fn on_confirm_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Enter => {
            let targets = match model.confirm.take() {
                Some(Confirm::One(target)) => vec![target],
                Some(Confirm::All(targets)) => targets,
                None => Vec::new(),
            };
            // One job per package: each passes the permit rules on its own.
            targets
                .into_iter()
                .map(|target| {
                    Command::Updates(UpdateCommand::Install {
                        target,
                        confirmed: true,
                    })
                })
                .collect()
        }
        KeyCode::Esc | KeyCode::Char('n') => {
            model.confirm = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Only when Claude Code is not set up yet and nothing conflicts.
fn telemetry_can_be_enabled(model: &Model) -> bool {
    model
        .telemetry
        .as_ref()
        .is_some_and(|status| status.claude == ClaudeTelemetry::Disabled)
}

/// The dialog takes all keys: Enter writes the entries, Esc closes.
fn on_telemetry_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    match key.code {
        KeyCode::Enter => {
            model.telemetry_flow = TelemetryFlow::Requested;
            vec![Command::Telemetry(TelemetryCommand::Enable)]
        }
        KeyCode::Esc | KeyCode::Char('n') => {
            model.telemetry_flow = TelemetryFlow::Idle;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn step_period(model: &mut Model, delta: isize) {
    let index = Period::ALL
        .iter()
        .position(|p| *p == model.ai_period)
        .unwrap_or(0);
    model.ai_period = Period::ALL[wrap_step(index, delta, Period::ALL.len())];
}

fn move_update_selection(model: &mut Model, delta: isize) {
    let count = model.update_rows().len();
    model.update_selected = wrap_step(model.update_selected, delta, count);
}

/// A complete workspace report replaces the old one and ends scan and sync.
fn on_workspaces(
    model: &mut Model,
    reports: Vec<hud_application::usecases::workspaces::WorkspaceReport>,
    source: ReportSource,
) {
    if source == ReportSource::Sync {
        model.last_sync = model.now;
    }
    let finished: Vec<_> = reports
        .iter()
        .filter_map(|w| w.repos.clone())
        .flatten()
        .collect();
    model.workspaces = reports;
    model.syncing = false;
    model.scan = None;
    model.mark_fast_forwards(&finished);
}

fn on_app_event(model: &mut Model, event: AppEvent) {
    match event {
        AppEvent::Workspaces { reports, source } => on_workspaces(model, reports, source),
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
        AppEvent::Updates(reports) => {
            model.updates = reports;
            model.checking_updates = false;
            let count = model.update_rows().len();
            model.update_selected = model.update_selected.min(count.saturating_sub(1));
        }
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
        AppEvent::Settings(preferences) => {
            model.apply_preferences(*preferences);
            model.settings_tab.notice = None;
            crate::settings_tab::clamp(model);
        }
        AppEvent::Usage(report) => model.usage = Some(report),
        AppEvent::Telemetry(status) => model.telemetry = Some(status),
        AppEvent::GitActivity(calendar) => model.git_activity = Some(calendar),
        AppEvent::System(sample) => {
            if model.load.len() == crate::view::SPARK_SAMPLES {
                model.load.pop_front();
            }
            model.load.push_back(sample);
        }
        AppEvent::Volumes(volumes) => model.volumes = volumes,
        AppEvent::JobsFinished => {
            if model.quit == Quit::Waiting {
                model.quit = Quit::Now;
            }
        }
        AppEvent::Actions(actions) => {
            model.actions = actions;
            crate::log_tab::clamp(model);
        }
        AppEvent::SettingsRejected(reason) => {
            model.settings_tab.notice = Some(crate::settings_tab::Notice::Rejected(reason));
        }
        AppEvent::RepoUpdated(report) => {
            model.mark_fast_forwards(std::slice::from_ref(&report));
            if let Some(notice) = &mut model.ff_notice
                && notice.repo == report.repo
            {
                notice.result = Some(report.state.clone());
                notice.at = model.now;
            }
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

    fn with_running_job() -> Model {
        use hud_application::usecases::updates::{JobId, JobState, JobUpdate};
        let mut model = Model::default();
        let job = JobUpdate {
            id: JobId(1),
            target: InstallTarget::AllSkills,
            state: JobState::Running,
        };
        update(&mut model, Msg::App(AppEvent::Job(job)));
        model
    }

    #[test]
    fn q_with_a_running_installation_asks_first() {
        let mut model = with_running_job();
        update(&mut model, key(KeyCode::Char('q')));
        assert!(!model.should_quit());
        assert_eq!(model.quit, Quit::Asking);
    }

    #[test]
    fn enter_in_the_quit_dialog_waits_for_the_installations() {
        let mut model = with_running_job();
        update(&mut model, key(KeyCode::Char('q')));
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![Command::Updates(UpdateCommand::Drain)]
        );
        assert!(!model.should_quit());
        update(&mut model, Msg::App(AppEvent::JobsFinished));
        assert!(model.should_quit());
    }

    #[test]
    fn a_in_the_quit_dialog_quits_and_cancels() {
        let mut model = with_running_job();
        update(&mut model, key(KeyCode::Char('q')));
        update(&mut model, key(KeyCode::Char('a')));
        assert!(model.should_quit());
    }

    #[test]
    fn escape_closes_the_quit_dialog() {
        let mut model = with_running_job();
        update(&mut model, key(KeyCode::Char('q')));
        update(&mut model, key(KeyCode::Esc));
        assert_eq!(model.quit, Quit::No);
        assert!(!model.should_quit());
    }

    #[test]
    fn q_while_waiting_quits_at_once() {
        let mut model = with_running_job();
        update(&mut model, key(KeyCode::Char('q')));
        update(&mut model, key(KeyCode::Enter));
        update(&mut model, key(KeyCode::Char('q')));
        assert!(model.should_quit());
    }

    #[test]
    fn ctrl_c_reaches_quit_even_from_an_open_input() {
        let mut model = Model {
            tab: Tab::Log,
            ..Model::default()
        };
        update(&mut model, key(KeyCode::Char('/')));
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(model.should_quit());
    }

    #[test]
    fn arrows_switch_the_ai_period_and_wrap() {
        let mut model = Model {
            tab: Tab::Ai,
            ..Model::default()
        };
        update(&mut model, key(KeyCode::Left));
        assert_eq!(model.ai_period, Period::LastMonth);
        update(&mut model, key(KeyCode::Right));
        assert_eq!(model.ai_period, Period::Today);
        update(&mut model, key(KeyCode::Char('3')));
        assert_eq!(model.ai_period, Period::Last30Days);
    }

    fn ai_with_telemetry(claude: ClaudeTelemetry) -> Model {
        use hud_application::ports::telemetry::ReceiverState;
        use hud_application::usecases::telemetry::TelemetryStatus;
        let mut model = Model {
            tab: Tab::Ai,
            ..Model::default()
        };
        let status = TelemetryStatus {
            receiver: ReceiverState::Listening(4318),
            claude,
            entries: Vec::new(),
        };
        update(&mut model, Msg::App(AppEvent::Telemetry(Box::new(status))));
        model
    }

    #[test]
    fn t_asks_before_enabling_telemetry() {
        let mut model = ai_with_telemetry(ClaudeTelemetry::Disabled);
        assert_eq!(update(&mut model, key(KeyCode::Char('t'))), Vec::new());
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![Command::Telemetry(TelemetryCommand::Enable)]
        );
    }

    #[test]
    fn t_does_nothing_when_settings_conflict() {
        let mut model = ai_with_telemetry(ClaudeTelemetry::Conflict(vec!["X=1".into()]));
        update(&mut model, key(KeyCode::Char('t')));
        assert_eq!(model.telemetry_flow, TelemetryFlow::Idle);
    }

    #[test]
    fn question_mark_opens_help_and_any_key_closes_it() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Char('?')));
        assert!(model.help);
        update(&mut model, key(KeyCode::Char('q')));
        assert!(!model.help);
        assert!(!model.should_quit(), "the closing key does nothing else");
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
        update(
            &mut model,
            Msg::Tick(Instant::now(), jiff::Timestamp::now()),
        );
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

    fn after_f_on(state: RepoState, now: Instant) -> Model {
        let mut model = on_workspaces_tab();
        model.workspaces_tab.all = true;
        update(&mut model, Msg::Tick(now, jiff::Timestamp::now()));
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::UpToDate)])),
        );
        update(&mut model, key(KeyCode::Char('f')));
        update(
            &mut model,
            Msg::App(AppEvent::RepoUpdated(report("/ws/a", state))),
        );
        model
    }

    #[test]
    fn f_shows_at_once_that_the_fast_forward_runs() {
        let mut model = on_workspaces_tab();
        model.workspaces_tab.all = true;
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::UpToDate)])),
        );
        update(&mut model, key(KeyCode::Char('f')));
        assert_eq!(model.ff_notice.and_then(|notice| notice.result), None);
    }

    #[test]
    fn f_reports_its_result_even_when_nothing_changed() {
        let model = after_f_on(RepoState::UpToDate, Instant::now());
        assert_eq!(
            model.ff_notice.and_then(|notice| notice.result),
            Some(RepoState::UpToDate)
        );
    }

    #[test]
    fn the_fast_forward_result_disappears_after_a_few_seconds() {
        let start = Instant::now();
        let mut model = after_f_on(RepoState::UpToDate, start);
        update(
            &mut model,
            Msg::Tick(start + Duration::from_secs(6), jiff::Timestamp::now()),
        );
        assert_eq!(model.ff_notice, None);
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
        // Only the behind repository is shown, so it is selected.
        let commands = update(&mut model, key(KeyCode::Char('f')));
        assert_eq!(
            commands,
            vec![Command::Workspaces(WorkspaceCommand::FastForward(
                RepoPath::try_from(PathBuf::from("/ws/b")).expect("BUG")
            ))]
        );
    }

    #[test]
    fn enter_on_a_repository_fast_forwards_like_f() {
        let mut model = on_workspaces_tab();
        update(
            &mut model,
            Msg::App(workspaces(vec![report("/ws/a", RepoState::Behind(1))])),
        );
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![Command::Workspaces(WorkspaceCommand::FastForward(
                RepoPath::try_from(PathBuf::from("/ws/a")).expect("BUG")
            ))]
        );
    }

    #[test]
    fn selection_wraps_at_both_ends_and_passes_the_workspace_heading() {
        let mut model = on_workspaces_tab();
        update(
            &mut model,
            Msg::App(workspaces(vec![
                report("/ws/a", RepoState::UpToDate),
                report("/ws/b", RepoState::UpToDate),
            ])),
        );
        let selected =
            |model: &Model| crate::workspaces_tab::selected_repo(model).map(|r| r.repo.to_string());
        update(&mut model, key(KeyCode::Char('v')));
        assert_eq!(selected(&model).as_deref(), Some("/ws/a"));
        update(&mut model, key(KeyCode::Up));
        assert_eq!(
            selected(&model),
            None,
            "the heading above the first repository"
        );
        update(&mut model, key(KeyCode::Up));
        assert_eq!(
            selected(&model).as_deref(),
            Some("/ws/b"),
            "up from the top wraps"
        );
        update(&mut model, key(KeyCode::Down));
        assert_eq!(selected(&model), None, "down from the bottom wraps");
    }

    #[test]
    fn selection_in_an_empty_list_stays_empty() {
        let mut model = on_workspaces_tab();
        update(&mut model, key(KeyCode::Down));
        update(&mut model, key(KeyCode::Up));
        assert_eq!(model.workspaces_tab.selected, None);
    }

    #[test]
    fn update_selection_wraps_at_both_ends() {
        let mut model = on_updates_tab(vec![npm_candidate("a", false), npm_candidate("b", false)]);
        update(&mut model, key(KeyCode::Char('k')));
        assert_eq!(model.update_selected, 1, "up from the first row");
        update(&mut model, key(KeyCode::Char('j')));
        assert_eq!(model.update_selected, 0, "down from the last row");
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
        update(&mut model, Msg::Tick(start, jiff::Timestamp::now()));
        update(
            &mut model,
            Msg::App(workspaces(vec![report(
                "/ws/a",
                RepoState::FastForwarded(3),
            )])),
        );
        let repo = RepoPath::try_from(PathBuf::from("/ws/a")).expect("BUG");
        assert!(model.is_recent_ok(&repo));
        update(
            &mut model,
            Msg::Tick(start + Duration::from_secs(31), jiff::Timestamp::now()),
        );
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

    use hud_application::ports::updates::{Availability, SourceCheck};
    use hud_application::usecases::updates::{JobId, JobState, JobUpdate, SourceReport};
    use hud_domain::update::{PackageKind, PackageName, SourceId, UpdateCandidate, Version};

    #[test]
    fn the_selected_update_stays_visible_and_clickable() {
        let candidates = (0..30)
            .map(|i| npm_candidate(&format!("pkg-{i}"), false))
            .collect();
        let mut model = on_updates_tab(candidates);
        update(&mut model, Msg::Resize(115, 16));
        update(&mut model, key(KeyCode::Up));
        assert_eq!(model.update_selected, 29, "wrapped to the last row");
        let shown = (0..16).any(|y| {
            (0..115).any(|x| {
                crate::view::hit(&model, model.area, x, y) == Some(crate::view::Hit::Row(29))
            })
        });
        assert!(shown, "the last row is on screen and resolves to itself");
    }

    fn npm_candidate(name: &str, pinned: bool) -> UpdateCandidate {
        UpdateCandidate {
            source: SourceId::Npm,
            kind: PackageKind::NpmGlobal,
            name: PackageName::try_from(name.to_owned()).expect("BUG"),
            current: Version::new("1.0.0"),
            latest: Version::new("1.1.0"),
            pinned,
        }
    }

    fn on_updates_tab(candidates: Vec<UpdateCandidate>) -> Model {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Tab));
        update(&mut model, key(KeyCode::Tab));
        let check = SourceCheck {
            candidates,
            installed: Vec::new(),
            updates_known: true,
        };
        let report = SourceReport {
            source: SourceId::Npm,
            availability: Availability::Available("11".into()),
            check: Some(Ok(check)),
        };
        update(&mut model, Msg::App(AppEvent::Updates(vec![report])));
        model
    }

    fn install(name: &str) -> Command {
        Command::Updates(UpdateCommand::Install {
            target: InstallTarget::Package(npm_candidate(name, false)),
            confirmed: true,
        })
    }

    #[test]
    fn a_confirms_once_and_installs_every_unpinned_package() {
        let mut model = on_updates_tab(vec![
            npm_candidate("a", false),
            npm_candidate("b", true),
            npm_candidate("c", false),
        ]);
        assert_eq!(update(&mut model, key(KeyCode::Char('a'))), Vec::new());
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![install("a"), install("c")]
        );
    }

    #[test]
    fn a_leaves_out_packages_that_are_already_installing() {
        let mut model = on_updates_tab(vec![npm_candidate("a", false), npm_candidate("b", false)]);
        let running = JobUpdate {
            id: JobId(1),
            target: InstallTarget::Package(npm_candidate("a", false)),
            state: JobState::Running,
        };
        update(&mut model, Msg::App(AppEvent::Job(running)));
        update(&mut model, key(KeyCode::Char('a')));
        assert_eq!(update(&mut model, key(KeyCode::Enter)), vec![install("b")]);
    }

    #[test]
    fn a_opens_no_dialog_when_nothing_can_be_installed() {
        let mut model = on_updates_tab(vec![npm_candidate("a", true)]);
        update(&mut model, key(KeyCode::Char('a')));
        assert_eq!(model.confirm, None);
    }

    #[test]
    fn u_opens_the_dialog_and_enter_sends_a_confirmed_install() {
        let mut model = on_updates_tab(vec![npm_candidate("a", false), npm_candidate("b", false)]);
        update(&mut model, key(KeyCode::Down));
        assert_eq!(update(&mut model, key(KeyCode::Char('u'))), Vec::new());
        assert!(model.confirm.is_some());
        let commands = update(&mut model, key(KeyCode::Enter));
        assert_eq!(
            commands,
            vec![Command::Updates(UpdateCommand::Install {
                target: InstallTarget::Package(npm_candidate("b", false)),
                confirmed: true,
            })]
        );
        assert_eq!(model.confirm, None);
    }

    #[test]
    fn escape_closes_the_dialog_without_installing() {
        let mut model = on_updates_tab(vec![npm_candidate("a", false)]);
        update(&mut model, key(KeyCode::Char('u')));
        assert_eq!(update(&mut model, key(KeyCode::Esc)), Vec::new());
        assert_eq!(model.confirm, None);
    }

    #[test]
    fn q_does_not_quit_while_the_dialog_is_open() {
        let mut model = on_updates_tab(vec![npm_candidate("a", false)]);
        update(&mut model, key(KeyCode::Char('u')));
        update(&mut model, key(KeyCode::Char('q')));
        assert!(!model.should_quit());
        assert!(model.confirm.is_some());
    }

    #[test]
    fn pinned_packages_do_not_open_the_dialog() {
        let mut model = on_updates_tab(vec![npm_candidate("a", true)]);
        update(&mut model, key(KeyCode::Char('u')));
        assert_eq!(model.confirm, None);
    }

    #[test]
    fn r_on_the_updates_tab_checks_sources_instead_of_syncing() {
        let mut model = on_updates_tab(vec![]);
        assert_eq!(
            update(&mut model, key(KeyCode::Char('r'))),
            vec![Command::Updates(UpdateCommand::CheckNow)]
        );
    }

    #[test]
    fn r_on_the_updates_tab_waits_for_the_running_check() {
        let mut model = on_updates_tab(vec![]);
        update(&mut model, key(KeyCode::Char('r')));
        assert_eq!(update(&mut model, key(KeyCode::Char('r'))), Vec::new());
    }

    #[test]
    fn an_update_report_ends_the_manual_check() {
        let mut model = on_updates_tab(vec![]);
        update(&mut model, key(KeyCode::Char('r')));
        update(&mut model, Msg::App(AppEvent::Updates(Vec::new())));
        assert!(!model.checking_updates);
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
