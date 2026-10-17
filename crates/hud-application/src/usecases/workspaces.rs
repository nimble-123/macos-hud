//! Scanning workspaces and keeping clean repositories fast-forwarded.

use futures::StreamExt;
use hud_domain::WorkspacePath;
use hud_domain::action::{ActionKind, Outcome, Trigger};
use hud_domain::git::{RepoPath, RepoStatus, SkipReason, SyncDecision, SyncMode, sync_decision};

use crate::ports::discovery::{DiscoveryError, RepoDiscovery};
use crate::ports::git::{FfOutcome, GitError, GitPort};
use crate::ports::store::{ActionLog, CachedRepo, Clock, ScanCache};
use crate::settings::{ScanSettings, WorkspaceSettings};

/// Status reads running at the same time; enough to hide latency, few
/// enough not to swamp the machine.
const PARALLEL_STATUS: usize = 8;

/// What the UI shows for a repository. Unlike [`SyncDecision`] it carries
/// no permit, so it can be stored, cloned and sent around freely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoState {
    /// Nothing to do.
    UpToDate,
    /// Behind its upstream and allowed to fast-forward by this many commits.
    Behind(u32),
    /// A rule blocks the fast-forward.
    Skipped(SkipReason),
    /// Status could not be read.
    Error(String),
    /// Fast-forwarded by this many commits during this run.
    FastForwarded(u32),
}

/// One repository in a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoReport {
    /// Repository.
    pub repo: RepoPath,
    /// Last known status, if it could be read.
    pub status: Option<RepoStatus>,
    /// What it means.
    pub state: RepoState,
}

/// One workspace in a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceReport {
    /// Workspace folder.
    pub workspace: WorkspacePath,
    /// Whether automatic fast-forward is on.
    pub auto_fast_forward: bool,
    /// `None` if the folder is not available.
    pub repos: Option<Vec<RepoReport>>,
    /// Why the folder is not available.
    pub unavailable: Option<String>,
}

/// State of a repository as seen by the automatic rules.
#[must_use]
pub fn repo_state(repo: &RepoPath, status: &RepoStatus, auto_fast_forward: bool) -> RepoState {
    match sync_decision(
        repo,
        status,
        SyncMode::Auto {
            opted_out: !auto_fast_forward,
        },
    ) {
        SyncDecision::UpToDate => RepoState::UpToDate,
        SyncDecision::FastForward(permit) => RepoState::Behind(permit.commits()),
        SyncDecision::Skip(reason) => RepoState::Skipped(reason),
    }
}

/// Reports the last cached state without touching any repository.
pub struct LoadCachedRepos<S> {
    cache: S,
}

impl<S: ScanCache> LoadCachedRepos<S> {
    /// Use case over the given cache.
    pub fn new(cache: S) -> Self {
        Self { cache }
    }

    /// Cached repositories grouped by the configured workspaces.
    pub async fn execute(&self, workspaces: &[WorkspaceSettings]) -> Vec<WorkspaceReport> {
        let cached = self.cache.load_all().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "scan cache unreadable");
            Vec::new()
        });
        workspaces
            .iter()
            .map(|workspace| {
                let repos = cached
                    .iter()
                    .filter(|entry| entry.repo.as_path().starts_with(workspace.path.as_path()))
                    .map(|entry| RepoReport {
                        repo: entry.repo.clone(),
                        status: Some(entry.status.clone()),
                        state: repo_state(&entry.repo, &entry.status, workspace.auto_fast_forward),
                    })
                    .collect();
                WorkspaceReport {
                    workspace: workspace.path.clone(),
                    auto_fast_forward: workspace.auto_fast_forward,
                    repos: Some(repos),
                    unavailable: None,
                }
            })
            .collect()
    }
}

/// Discovers repositories, fetches them and fast-forwards those the rules allow.
pub struct SyncRepositories<G, D, L, S, C> {
    git: G,
    discovery: D,
    log: L,
    cache: S,
    clock: C,
}

impl<G, D, L, S, C> SyncRepositories<G, D, L, S, C>
where
    G: GitPort,
    D: RepoDiscovery,
    L: ActionLog,
    S: ScanCache,
    C: Clock,
{
    /// Use case over the given ports.
    pub fn new(git: G, discovery: D, log: L, cache: S, clock: C) -> Self {
        Self {
            git,
            discovery,
            log,
            cache,
            clock,
        }
    }

    /// One full run. With `fetch = false` only local state is read, which is
    /// fast and never touches the network.
    pub async fn execute(
        &self,
        workspaces: &[WorkspaceSettings],
        scan: &ScanSettings,
        fetch: bool,
    ) -> Vec<WorkspaceReport> {
        let mut reports = Vec::with_capacity(workspaces.len());
        let mut seen = Vec::new();
        for workspace in workspaces {
            let report = match self.discovery.discover(&workspace.path, scan).await {
                Ok(repos) => {
                    seen.extend(repos.iter().cloned());
                    let results: Vec<RepoReport> = futures::stream::iter(repos)
                        .map(|repo| self.sync_one(repo, workspace.auto_fast_forward, fetch))
                        .buffer_unordered(PARALLEL_STATUS)
                        .collect()
                        .await;
                    let mut results = results;
                    results.sort_by(|a, b| a.repo.cmp(&b.repo));
                    WorkspaceReport {
                        workspace: workspace.path.clone(),
                        auto_fast_forward: workspace.auto_fast_forward,
                        repos: Some(results),
                        unavailable: None,
                    }
                }
                Err(error) => WorkspaceReport {
                    workspace: workspace.path.clone(),
                    auto_fast_forward: workspace.auto_fast_forward,
                    repos: None,
                    unavailable: Some(match error {
                        DiscoveryError::Unavailable(_) => "unavailable".to_owned(),
                        DiscoveryError::Unreadable { message, .. } => message,
                    }),
                },
            };
            reports.push(report);
        }
        if let Err(error) = self.cache.retain(seen).await {
            tracing::warn!(%error, "could not prune scan cache");
        }
        reports
    }

    async fn sync_one(&self, repo: RepoPath, auto_fast_forward: bool, fetch: bool) -> RepoReport {
        if fetch && let Err(error) = self.git.fetch(&repo).await {
            // Successful fetches are not logged: every 15 minutes for every
            // repository they would drown the actions that matter.
            self.log_failed_fetch(&repo, &error).await;
        }
        let status = match self.git.status(&repo).await {
            Ok(status) => status,
            Err(error) => {
                return RepoReport {
                    repo,
                    status: None,
                    state: RepoState::Error(error.to_string()),
                };
            }
        };
        self.save_cache(&repo, &status).await;
        let mode = SyncMode::Auto {
            opted_out: !auto_fast_forward,
        };
        let state = match sync_decision(&repo, &status, mode) {
            SyncDecision::UpToDate => RepoState::UpToDate,
            SyncDecision::Skip(reason) => RepoState::Skipped(reason),
            SyncDecision::FastForward(permit) => {
                let commits = permit.commits();
                match fast_forward(&self.git, &self.log, &self.clock, permit, Trigger::Auto).await {
                    Ok(FfOutcome::Advanced { .. }) => RepoState::FastForwarded(commits),
                    Ok(FfOutcome::StateChanged) => RepoState::Behind(commits),
                    Err(error) => RepoState::Error(error.to_string()),
                }
            }
        };
        RepoReport {
            repo,
            status: Some(status),
            state,
        }
    }

    async fn log_failed_fetch(&self, repo: &RepoPath, error: &GitError) {
        let target = repo.to_string();
        let now = self.clock.now();
        match self
            .log
            .start(now, ActionKind::Fetch, &target, Trigger::Auto)
            .await
        {
            Ok(id) => {
                let outcome = Outcome::Failed(error.to_string());
                if let Err(log_error) = self.log.finish(id, self.clock.now(), outcome, None).await {
                    tracing::warn!(%log_error, "could not log failed fetch");
                }
            }
            Err(log_error) => tracing::warn!(%log_error, "could not log failed fetch"),
        }
    }

    async fn save_cache(&self, repo: &RepoPath, status: &RepoStatus) {
        let entry = CachedRepo {
            repo: repo.clone(),
            status: status.clone(),
            scanned_at: self.clock.now(),
        };
        if let Err(error) = self.cache.save(entry).await {
            tracing::warn!(%error, "could not cache repository status");
        }
    }
}

/// Fast-forwards one repository on request of the user. The workspace
/// opt-out does not apply; every safety rule does.
pub struct FastForwardRepo<G, L, C> {
    git: G,
    log: L,
    clock: C,
}

impl<G: GitPort, L: ActionLog, C: Clock> FastForwardRepo<G, L, C> {
    /// Use case over the given ports.
    pub fn new(git: G, log: L, clock: C) -> Self {
        Self { git, log, clock }
    }

    /// Reads the status, applies the rules in manual mode and fast-forwards if allowed.
    pub async fn execute(&self, repo: &RepoPath) -> RepoReport {
        let status = match self.git.status(repo).await {
            Ok(status) => status,
            Err(error) => {
                return RepoReport {
                    repo: repo.clone(),
                    status: None,
                    state: RepoState::Error(error.to_string()),
                };
            }
        };
        let state = match sync_decision(repo, &status, SyncMode::Manual) {
            SyncDecision::UpToDate => RepoState::UpToDate,
            SyncDecision::Skip(reason) => RepoState::Skipped(reason),
            SyncDecision::FastForward(permit) => {
                let commits = permit.commits();
                match fast_forward(&self.git, &self.log, &self.clock, permit, Trigger::Manual).await
                {
                    Ok(FfOutcome::Advanced { .. }) => RepoState::FastForwarded(commits),
                    Ok(FfOutcome::StateChanged) => RepoState::Behind(commits),
                    Err(error) => RepoState::Error(error.to_string()),
                }
            }
        };
        RepoReport {
            repo: repo.clone(),
            status: Some(status),
            state,
        }
    }
}

/// Logs, fast-forwards and logs again. A failing log write never blocks the
/// fast-forward itself: the permit was granted and Git still protects local work.
async fn fast_forward(
    git: &impl GitPort,
    log: &impl ActionLog,
    clock: &impl Clock,
    permit: hud_domain::git::FastForwardPermit,
    trigger: Trigger,
) -> Result<FfOutcome, GitError> {
    let target = permit.repo().to_string();
    let id = log
        .start(clock.now(), ActionKind::FastForward, &target, trigger)
        .await;
    if let Err(error) = &id {
        tracing::warn!(%error, "could not log fast-forward start");
    }
    let result = git.fast_forward(permit).await;
    if let Ok(id) = id {
        let (outcome, summary) = match &result {
            Ok(FfOutcome::Advanced { commits, to, .. }) => (
                Outcome::Succeeded,
                Some(format!("{commits} commits to {}", &to.as_str()[..12])),
            ),
            Ok(FfOutcome::StateChanged) => {
                (Outcome::Skipped("HEAD moved since the check".into()), None)
            }
            Err(error) => (Outcome::Failed(error.to_string()), None),
        };
        if let Err(error) = log.finish(id, clock.now(), outcome, summary).await {
            tracing::warn!(%error, "could not log fast-forward result");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::testing::{
        FakeCache, FakeClock, FakeDiscovery, FakeGit, FakeLog, behind, repo, workspace,
    };

    fn discovery(entries: &[(&str, &[&str])]) -> FakeDiscovery {
        FakeDiscovery {
            repos: entries
                .iter()
                .map(|(ws, repos)| ((*ws).to_owned(), repos.iter().map(|r| repo(r)).collect()))
                .collect::<HashMap<_, _>>(),
        }
    }

    async fn run(
        git: FakeGit,
        discovery: FakeDiscovery,
        workspaces: &[WorkspaceSettings],
    ) -> (
        Vec<WorkspaceReport>,
        SyncRepositories<FakeGit, FakeDiscovery, FakeLog, FakeCache, FakeClock>,
    ) {
        let use_case = SyncRepositories::new(
            git,
            discovery,
            FakeLog::default(),
            FakeCache::default(),
            FakeClock,
        );
        let reports = use_case
            .execute(workspaces, &ScanSettings::default(), true)
            .await;
        (reports, use_case)
    }

    #[tokio::test]
    async fn clean_repo_behind_upstream_is_fast_forwarded_and_logged() {
        let git = FakeGit::with(&[("/ws/api", behind(3))]);
        let (reports, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;

        let repos = reports[0].repos.as_ref().expect("BUG: available");
        assert_eq!(repos[0].state, RepoState::FastForwarded(3));
        let records = use_case.log.records.lock().expect("BUG").clone();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, ActionKind::FastForward);
        assert_eq!(records[0].trigger, Trigger::Auto);
        assert_eq!(records[0].outcome, Some(Outcome::Succeeded));
    }

    #[tokio::test]
    async fn opted_out_workspace_is_never_fast_forwarded() {
        let git = FakeGit::with(&[("/ws/api", behind(3))]);
        let (reports, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", false)],
        )
        .await;

        assert_eq!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::Skipped(SkipReason::OptedOut)
        );
        assert!(use_case.git.fast_forwarded.lock().expect("BUG").is_empty());
    }

    #[tokio::test]
    async fn repo_with_tracked_changes_is_skipped_without_touching_git() {
        let dirty = RepoStatus {
            tracked_changes: 2,
            ..behind(3)
        };
        let git = FakeGit::with(&[("/ws/api", dirty)]);
        let (reports, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;

        assert_eq!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::Skipped(SkipReason::TrackedChanges)
        );
        assert!(use_case.git.fast_forwarded.lock().expect("BUG").is_empty());
        assert!(use_case.log.records.lock().expect("BUG").is_empty());
    }

    #[tokio::test]
    async fn failed_fetch_is_logged_and_status_is_still_read() {
        let mut git = FakeGit::with(&[("/ws/api", behind(0))]);
        git.failing_fetches = vec![repo("/ws/api")];
        let (reports, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;

        assert_eq!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::UpToDate
        );
        let records = use_case.log.records.lock().expect("BUG").clone();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, ActionKind::Fetch);
        assert!(matches!(records[0].outcome, Some(Outcome::Failed(_))));
    }

    #[tokio::test]
    async fn successful_fetch_is_not_logged() {
        let git = FakeGit::with(&[("/ws/api", behind(0))]);
        let (_, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;
        assert_eq!(use_case.git.fetched.lock().expect("BUG").len(), 1);
        assert!(use_case.log.records.lock().expect("BUG").is_empty());
    }

    #[tokio::test]
    async fn local_only_run_never_fetches() {
        let git = FakeGit::with(&[("/ws/api", behind(0))]);
        let use_case = SyncRepositories::new(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            FakeLog::default(),
            FakeCache::default(),
            FakeClock,
        );
        let _ = use_case
            .execute(&[workspace("/ws", true)], &ScanSettings::default(), false)
            .await;
        assert!(use_case.git.fetched.lock().expect("BUG").is_empty());
    }

    #[tokio::test]
    async fn unavailable_workspace_does_not_stop_the_others() {
        let git = FakeGit::with(&[("/ws/api", behind(0))]);
        let workspaces = [workspace("/Volumes/Ext", true), workspace("/ws", true)];
        let (reports, _) = run(git, discovery(&[("/ws", &["/ws/api"])]), &workspaces).await;

        assert!(reports[0].repos.is_none());
        assert_eq!(reports[0].unavailable.as_deref(), Some("unavailable"));
        assert_eq!(reports[1].repos.as_ref().expect("BUG").len(), 1);
    }

    #[tokio::test]
    async fn unreadable_status_becomes_an_error_state() {
        let mut git = FakeGit::with(&[("/ws/api", behind(1))]);
        git.failing_status = vec![repo("/ws/api")];
        let (reports, _) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;
        assert!(matches!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::Error(_)
        ));
    }

    #[tokio::test]
    async fn moved_head_leaves_the_repo_behind_and_logs_a_skip() {
        let mut git = FakeGit::with(&[("/ws/api", behind(2))]);
        git.state_changed = true;
        let (reports, use_case) = run(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            &[workspace("/ws", true)],
        )
        .await;

        assert_eq!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::Behind(2)
        );
        let records = use_case.log.records.lock().expect("BUG").clone();
        assert!(matches!(records[0].outcome, Some(Outcome::Skipped(_))));
    }

    #[tokio::test]
    async fn repos_are_cached_and_vanished_ones_are_pruned() {
        let git = FakeGit::with(&[("/ws/api", behind(0))]);
        let cache = FakeCache::default();
        cache.entries.lock().expect("BUG").push(CachedRepo {
            repo: repo("/ws/gone"),
            status: behind(0),
            scanned_at: FakeClock.now(),
        });
        let use_case = SyncRepositories::new(
            git,
            discovery(&[("/ws", &["/ws/api"])]),
            FakeLog::default(),
            cache,
            FakeClock,
        );
        let _ = use_case
            .execute(&[workspace("/ws", true)], &ScanSettings::default(), true)
            .await;

        let cached: Vec<_> = use_case
            .cache
            .entries
            .lock()
            .expect("BUG")
            .iter()
            .map(|e| e.repo.clone())
            .collect();
        assert_eq!(cached, vec![repo("/ws/api")]);
    }

    #[tokio::test]
    async fn manual_fast_forward_ignores_the_opt_out_but_not_the_rules() {
        let git = FakeGit::with(&[
            ("/ws/api", behind(2)),
            (
                "/ws/dirty",
                RepoStatus {
                    tracked_changes: 1,
                    ..behind(2)
                },
            ),
        ]);
        let use_case = FastForwardRepo::new(git, FakeLog::default(), FakeClock);

        assert_eq!(
            use_case.execute(&repo("/ws/api")).await.state,
            RepoState::FastForwarded(2)
        );
        assert_eq!(
            use_case.execute(&repo("/ws/dirty")).await.state,
            RepoState::Skipped(SkipReason::TrackedChanges)
        );
        let records = use_case.log.records.lock().expect("BUG").clone();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].trigger, Trigger::Manual);
    }

    #[tokio::test]
    async fn cached_repos_are_grouped_by_workspace_with_recomputed_state() {
        let cache = FakeCache::default();
        for (path, status) in [("/ws/api", behind(3)), ("/other/x", behind(0))] {
            cache.entries.lock().expect("BUG").push(CachedRepo {
                repo: repo(path),
                status,
                scanned_at: FakeClock.now(),
            });
        }
        let reports = LoadCachedRepos::new(cache)
            .execute(&[workspace("/ws", true), workspace("/other", false)])
            .await;

        assert_eq!(
            reports[0].repos.as_ref().expect("BUG")[0].state,
            RepoState::Behind(3)
        );
        assert_eq!(
            reports[1].repos.as_ref().expect("BUG")[0].state,
            RepoState::UpToDate
        );
    }
}
