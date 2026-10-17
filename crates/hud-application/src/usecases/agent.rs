//! Drives the workspace use cases: on start, on a timer and on command.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, ReportSource, WorkspaceCommand};
use crate::ports::discovery::RepoDiscovery;
use crate::ports::git::GitPort;
use crate::ports::store::{ActionLog, Clock, ScanCache};
use crate::settings::{ScanSettings, WorkspaceSettings};
use crate::usecases::workspaces::{FastForwardRepo, LoadCachedRepos, SyncRepositories};

/// Everything the agent needs to know about the configuration.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Configured workspaces.
    pub workspaces: Vec<WorkspaceSettings>,
    /// Discovery settings.
    pub scan: ScanSettings,
    /// Pause between syncs with fetch.
    pub fetch_interval: Duration,
}

/// Runs the workspace use cases until cancelled.
pub struct WorkspaceAgent<G, D, L, S, C> {
    cached: LoadCachedRepos<S>,
    sync: SyncRepositories<G, D, L, S, C>,
    manual: FastForwardRepo<G, L, C>,
    config: WorkspaceConfig,
}

impl<G, D, L, S, C> WorkspaceAgent<G, D, L, S, C>
where
    G: GitPort + Clone,
    D: RepoDiscovery,
    L: ActionLog + Clone,
    S: ScanCache + Clone,
    C: Clock + Clone,
{
    /// Agent over the given ports.
    pub fn new(git: G, discovery: D, log: L, cache: S, clock: C, config: WorkspaceConfig) -> Self {
        Self {
            cached: LoadCachedRepos::new(cache.clone()),
            manual: FastForwardRepo::new(git.clone(), log.clone(), clock.clone()),
            sync: SyncRepositories::new(git, discovery, log, cache, clock),
            config,
        }
    }

    /// Cache first, then a quick local scan, then fetch-and-sync on every
    /// interval tick or command. Stops between steps once `cancel` fires.
    pub async fn run(
        self,
        mut commands: mpsc::Receiver<WorkspaceCommand>,
        events: mpsc::Sender<AppEvent>,
        cancel: CancellationToken,
    ) {
        let workspaces = &self.config.workspaces;
        let reports = self.cached.execute(workspaces).await;
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
        let reports = self
            .sync
            .execute(workspaces, &self.config.scan, false, Some(&events))
            .await;
        if send(
            &events,
            AppEvent::Workspaces {
                reports,
                source: ReportSource::LocalScan,
            },
        )
        .await
        .is_err()
        {
            return;
        }

        // The first tick fires immediately: fetch right after the local scan.
        let mut timer = tokio::time::interval(self.config.fetch_interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return,
                _ = timer.tick() => Some(WorkspaceCommand::SyncNow),
                command = commands.recv() => command,
            };
            let Some(command) = next else { return };
            if cancel.is_cancelled() {
                return;
            }
            let event = match command {
                WorkspaceCommand::SyncNow => {
                    if send(&events, AppEvent::SyncStarted).await.is_err() {
                        return;
                    }
                    let reports = self
                        .sync
                        .execute(workspaces, &self.config.scan, true, Some(&events))
                        .await;
                    timer.reset();
                    AppEvent::Workspaces {
                        reports,
                        source: ReportSource::Sync,
                    }
                }
                WorkspaceCommand::FastForward(repo) => {
                    AppEvent::RepoUpdated(self.manual.execute(&repo).await)
                }
            };
            if send(&events, event).await.is_err() {
                return;
            }
        }
    }
}

/// A closed channel means the UI is gone; the agent then stops.
async fn send(events: &mpsc::Sender<AppEvent>, event: AppEvent) -> Result<(), ()> {
    events.send(event).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

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

    fn agent(git: FakeGit, auto: bool) -> (TestAgent, Arc<FakeGit>) {
        let git = Arc::new(git);
        let discovery = FakeDiscovery {
            repos: HashMap::from([("/ws".to_owned(), vec![repo("/ws/api")])]),
        };
        let config = WorkspaceConfig {
            workspaces: vec![workspace("/ws", auto)],
            scan: ScanSettings::default(),
            fetch_interval: Duration::from_hours(1),
        };
        let agent = WorkspaceAgent::new(
            git.clone(),
            discovery,
            Arc::new(FakeLog::default()),
            Arc::new(FakeCache::default()),
            Clk,
            config,
        );
        (agent, git)
    }

    #[tokio::test]
    async fn start_sends_cache_then_local_scan_then_sync() {
        let (agent, git) = agent(FakeGit::with(&[("/ws/api", behind(2))]), true);
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
        let (agent, _git) = agent(FakeGit::with(&[("/ws/api", behind(1))]), false);
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
    async fn agent_stops_when_the_ui_is_gone() {
        let (agent, _git) = agent(FakeGit::with(&[("/ws/api", behind(0))]), true);
        let (_commands_tx, commands) = mpsc::channel(4);
        let (events_tx, events) = mpsc::channel(16);
        drop(events);
        agent
            .run(commands, events_tx, CancellationToken::new())
            .await;
    }
}
