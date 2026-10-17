//! Drives the workspace use cases: on start, on a timer, on command and
//! whenever the settings change.

use std::collections::HashSet;
use std::time::Duration;

use futures::StreamExt;
use hud_domain::activity::{ActivityCalendar, calendar, calendar_start};
use hud_domain::git::RepoPath;
use jiff::tz::TimeZone;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, Interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, ReportSource, WorkspaceCommand};
use crate::ports::discovery::RepoDiscovery;
use crate::ports::git::GitPort;
use crate::ports::store::{ActionLog, Clock, ScanCache};
use crate::settings::Settings;
use crate::usecases::workspaces::{
    FastForwardRepo, LoadCachedRepos, SyncRepositories, WorkspaceReport,
};

/// Repositories read at once for the activity calendar.
const LOG_CONCURRENCY: usize = 8;

/// Runs the workspace use cases until cancelled.
pub struct WorkspaceAgent<G, D, L, S, C> {
    cached: LoadCachedRepos<S>,
    sync: SyncRepositories<G, D, L, S, C>,
    manual: FastForwardRepo<G, L, C>,
    settings: watch::Receiver<Settings>,
    git: G,
    clock: C,
    time_zone: TimeZone,
}

/// What woke the agent up.
enum Step {
    Command(WorkspaceCommand),
    SettingsChanged,
}

impl<G, D, L, S, C> WorkspaceAgent<G, D, L, S, C>
where
    G: GitPort + Clone,
    D: RepoDiscovery,
    L: ActionLog + Clone,
    S: ScanCache + Clone,
    C: Clock + Clone,
{
    /// Agent over the given ports; workspaces, scan settings and the fetch
    /// interval come from `settings` and follow its changes. Days of the
    /// activity calendar follow `time_zone`.
    pub fn new(
        git: G,
        discovery: D,
        log: L,
        cache: S,
        clock: C,
        settings: watch::Receiver<Settings>,
        time_zone: TimeZone,
    ) -> Self {
        Self {
            cached: LoadCachedRepos::new(cache.clone()),
            manual: FastForwardRepo::new(git.clone(), log.clone(), clock.clone()),
            sync: SyncRepositories::new(git.clone(), discovery, log, cache, clock.clone()),
            settings,
            git,
            clock,
            time_zone,
        }
    }

    /// Cache first, then a quick local scan, then fetch-and-sync on every
    /// interval tick or command. A settings change triggers a local scan.
    /// Stops between steps once `cancel` fires.
    pub async fn run(
        mut self,
        mut commands: mpsc::Receiver<WorkspaceCommand>,
        events: mpsc::Sender<AppEvent>,
        cancel: CancellationToken,
    ) {
        let current = self.settings.borrow_and_update().clone();
        let reports = self.cached.execute(&current.workspaces).await;
        if send(
            &events,
            AppEvent::Workspaces {
                reports,
                source: ReportSource::Cache,
            },
        )
        .await
        .is_err()
        {
            return;
        }
        let Ok(reports) = self.local_scan(&current, &events).await else {
            return;
        };
        if self.report_activity(&reports, &events).await.is_err() {
            return;
        }

        // The first tick fires immediately: fetch right after the local scan.
        let mut fetch_interval = current.fetch_interval;
        let mut timer = fetch_timer(Instant::now(), fetch_interval);
        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return,
                _ = timer.tick() => Some(Step::Command(WorkspaceCommand::SyncNow)),
                Ok(()) = self.settings.changed() => Some(Step::SettingsChanged),
                command = commands.recv() => command.map(Step::Command),
            };
            let Some(step) = next else { return };
            if cancel.is_cancelled() {
                return;
            }
            let current = self.settings.borrow_and_update().clone();
            let event = match step {
                Step::SettingsChanged => {
                    if current.fetch_interval != fetch_interval {
                        fetch_interval = current.fetch_interval;
                        timer = fetch_timer(Instant::now() + fetch_interval, fetch_interval);
                    }
                    let Ok(reports) = self.local_scan(&current, &events).await else {
                        return;
                    };
                    if self.report_activity(&reports, &events).await.is_err() {
                        return;
                    }
                    continue;
                }
                Step::Command(WorkspaceCommand::SyncNow) => {
                    if send(&events, AppEvent::SyncStarted).await.is_err() {
                        return;
                    }
                    let reports = self
                        .sync
                        .execute(&current.workspaces, &current.scan, true, Some(&events))
                        .await;
                    timer.reset();
                    // New commits arrive with the fetch; the calendar follows each sync.
                    if send(
                        &events,
                        AppEvent::Workspaces {
                            reports: reports.clone(),
                            source: ReportSource::Sync,
                        },
                    )
                    .await
                    .is_err()
                        || self.report_activity(&reports, &events).await.is_err()
                    {
                        return;
                    }
                    continue;
                }
                Step::Command(WorkspaceCommand::FastForward(repo)) => {
                    AppEvent::RepoUpdated(self.manual.execute(&repo).await)
                }
            };
            if send(&events, event).await.is_err() {
                return;
            }
        }
    }

    /// Scans without contacting remotes and reports the result.
    async fn local_scan(
        &self,
        current: &Settings,
        events: &mpsc::Sender<AppEvent>,
    ) -> Result<Vec<WorkspaceReport>, ()> {
        let reports = self
            .sync
            .execute(&current.workspaces, &current.scan, false, Some(events))
            .await;
        send(
            events,
            AppEvent::Workspaces {
                reports: reports.clone(),
                source: ReportSource::LocalScan,
            },
        )
        .await?;
        Ok(reports)
    }

    /// Own commits per day across all repositories of the reports; the same
    /// commit in several clones counts once.
    async fn report_activity(
        &self,
        reports: &[WorkspaceReport],
        events: &mpsc::Sender<AppEvent>,
    ) -> Result<(), ()> {
        let now = self.clock.now().to_zoned(self.time_zone.clone());
        let today = now.date();
        let since = calendar_start(today)
            .to_zoned(self.time_zone.clone())
            .map_or(now.timestamp(), |start| start.timestamp());
        let repos: Vec<RepoPath> = reports
            .iter()
            .filter_map(|workspace| workspace.repos.as_ref())
            .flatten()
            .map(|report| report.repo.clone())
            .collect();
        let commits: Vec<_> = futures::stream::iter(repos)
            .map(|repo| async move {
                self.git
                    .own_commits(&repo, since)
                    .await
                    .unwrap_or_else(|error| {
                        tracing::debug!(%error, repo = %repo, "no commit history");
                        Vec::new()
                    })
            })
            .buffer_unordered(LOG_CONCURRENCY)
            .collect()
            .await;
        let mut seen = HashSet::new();
        let times: Vec<_> = commits
            .into_iter()
            .flatten()
            .filter(|(id, _)| seen.insert(id.clone()))
            .map(|(_, at)| at)
            .collect();
        tracing::debug!(commits = times.len(), "own commits counted");
        let calendar: ActivityCalendar = calendar(&times, today, &self.time_zone);
        send(events, AppEvent::GitActivity(calendar)).await
    }
}

fn fetch_timer(start: Instant, period: Duration) -> Interval {
    let mut timer = tokio::time::interval_at(start, period);
    timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
    timer
}

/// A closed channel means the UI is gone; the agent then stops.
async fn send(events: &mpsc::Sender<AppEvent>, event: AppEvent) -> Result<(), ()> {
    events.send(event).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use jiff::tz::TimeZone;

    use super::*;
    use crate::testing::{
        FakeCache, FakeClock, FakeDiscovery, FakeGit, FakeLog, behind, repo, workspace,
    };
    use crate::usecases::workspaces::RepoState;

    #[derive(Clone)]
    struct Clk;
    impl Clock for Clk {
        fn now(&self) -> jiff::Timestamp {
            FakeClock.now()
        }
    }

    type TestAgent = WorkspaceAgent<Arc<FakeGit>, FakeDiscovery, Arc<FakeLog>, Arc<FakeCache>, Clk>;

    fn agent(git: FakeGit, auto: bool) -> (TestAgent, Arc<FakeGit>, watch::Sender<Settings>) {
        let git = Arc::new(git);
        let discovery = FakeDiscovery {
            repos: HashMap::from([
                ("/ws".to_owned(), vec![repo("/ws/api")]),
                ("/other".to_owned(), Vec::new()),
            ]),
        };
        let (settings, watched) = watch::channel(Settings {
            workspaces: vec![workspace("/ws", auto)],
            fetch_interval: Duration::from_hours(1),
            ..Settings::default()
        });
        let agent = WorkspaceAgent::new(
            git.clone(),
            discovery,
            Arc::new(FakeLog::default()),
            Arc::new(FakeCache::default()),
            Clk,
            watched,
            TimeZone::UTC,
        );
        (agent, git, settings)
    }

    #[tokio::test]
    async fn start_sends_cache_then_local_scan_then_sync() {
        let (agent, git, _settings) = agent(FakeGit::with(&[("/ws/api", behind(2))]), true);
        let (_commands_tx, commands) = mpsc::channel(4);
        let (events_tx, mut events) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands, events_tx, cancel.clone()));

        let mut sources = Vec::new();
        while sources.len() < 3 {
            if let AppEvent::Workspaces { source, reports } =
                events.recv().await.expect("BUG: agent alive")
            {
                if source == ReportSource::Sync {
                    assert_eq!(
                        reports[0].repos.as_ref().expect("BUG")[0].state,
                        RepoState::FastForwarded(2)
                    );
                }
                sources.push(source);
            }
        }
        assert_eq!(
            sources,
            vec![
                ReportSource::Cache,
                ReportSource::LocalScan,
                ReportSource::Sync
            ]
        );
        assert_eq!(git.fetched.lock().expect("BUG").len(), 1);
        cancel.cancel();
        task.await.expect("BUG: agent task");
    }

    #[tokio::test]
    async fn fast_forward_command_runs_the_manual_use_case() {
        let (agent, _git, _settings) = agent(FakeGit::with(&[("/ws/api", behind(1))]), false);
        let (commands_tx, commands) = mpsc::channel(4);
        let (events_tx, mut events) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands, events_tx, cancel.clone()));
        commands_tx
            .send(WorkspaceCommand::FastForward(repo("/ws/api")))
            .await
            .expect("BUG");

        let updated = loop {
            if let AppEvent::RepoUpdated(report) = events.recv().await.expect("BUG: agent alive") {
                break report;
            }
        };
        assert_eq!(updated.state, RepoState::FastForwarded(1));
        cancel.cancel();
        task.await.expect("BUG: agent task");
    }

    #[tokio::test]
    async fn changed_workspaces_are_scanned_locally_at_once() {
        let (agent, git, settings) = agent(FakeGit::with(&[("/ws/api", behind(0))]), true);
        let (_commands_tx, commands) = mpsc::channel(4);
        let (events_tx, mut events) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands, events_tx, cancel.clone()));
        // Wait for the start sequence to end with the first sync.
        while !matches!(
            events.recv().await,
            Some(AppEvent::Workspaces {
                source: ReportSource::Sync,
                ..
            })
        ) {}

        settings.send_modify(|current| current.workspaces.push(workspace("/other", true)));
        let reports = loop {
            if let Some(AppEvent::Workspaces { reports, source }) = events.recv().await {
                assert_eq!(source, ReportSource::LocalScan);
                break reports;
            }
        };
        assert_eq!(reports.len(), 2);
        assert_eq!(git.fetched.lock().expect("BUG").len(), 1, "no extra fetch");
        cancel.cancel();
        task.await.expect("BUG: agent task");
    }

    #[tokio::test]
    async fn the_activity_counts_a_commit_in_two_clones_once() {
        use hud_domain::git::CommitId;
        let commit = |id: char, at: &str| {
            (
                CommitId::try_from(id.to_string().repeat(40).as_str()).expect("BUG"),
                at.parse::<jiff::Timestamp>().expect("BUG"),
            )
        };
        let mut git = FakeGit::with(&[("/ws/api", behind(0)), ("/ws/api-copy", behind(0))]);
        git.commits = HashMap::from([
            (
                repo("/ws/api"),
                vec![
                    commit('a', "2026-10-17T08:00:00Z"),
                    commit('b', "2026-10-16T08:00:00Z"),
                ],
            ),
            (
                repo("/ws/api-copy"),
                vec![commit('a', "2026-10-17T08:00:00Z")],
            ),
        ]);
        let discovery = FakeDiscovery {
            repos: HashMap::from([(
                "/ws".to_owned(),
                vec![repo("/ws/api"), repo("/ws/api-copy")],
            )]),
        };
        let (_settings, watched) = watch::channel(Settings {
            workspaces: vec![workspace("/ws", true)],
            fetch_interval: Duration::from_hours(1),
            ..Settings::default()
        });
        let agent = WorkspaceAgent::new(
            Arc::new(git),
            discovery,
            Arc::new(FakeLog::default()),
            Arc::new(FakeCache::default()),
            Clk,
            watched,
            TimeZone::UTC,
        );
        let (_commands_tx, commands) = mpsc::channel(4);
        let (events_tx, mut events) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands, events_tx, cancel.clone()));
        let calendar = loop {
            if let Some(AppEvent::GitActivity(calendar)) = events.recv().await {
                break calendar;
            }
        };
        assert_eq!(
            calendar.days.iter().sum::<u64>(),
            2,
            "a counts once, b once"
        );
        assert_eq!(calendar.days.last(), Some(&1));
        cancel.cancel();
        task.await.expect("BUG: agent task");
    }

    #[tokio::test]
    async fn agent_stops_when_the_ui_is_gone() {
        let (agent, _git, _settings) = agent(FakeGit::with(&[("/ws/api", behind(0))]), true);
        let (_commands_tx, commands) = mpsc::channel(4);
        let (events_tx, events) = mpsc::channel(16);
        drop(events);
        agent
            .run(commands, events_tx, CancellationToken::new())
            .await;
    }
}
