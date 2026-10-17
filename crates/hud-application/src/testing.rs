//! Hand-written fakes for the ports (fakes first, mockall only for call checks).
#![expect(
    clippy::unused_async_trait_impl,
    reason = "fakes answer immediately; the trait still needs async signatures"
)]
#![expect(
    clippy::significant_drop_tightening,
    reason = "short-lived locks in single-threaded tests cannot contend"
)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use hud_domain::WorkspacePath;
use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
use hud_domain::git::{
    BranchName, CommitId, FastForwardPermit, Head, RepoPath, RepoStatus, Upstream,
};
use jiff::Timestamp;

use crate::ports::discovery::{DiscoveryError, RepoDiscovery};
use crate::ports::git::{FfOutcome, GitError, GitPort};
use crate::ports::store::{ActionLog, CachedRepo, Clock, ScanCache, StoreError};
use crate::settings::{ScanSettings, WorkspaceSettings};

pub(crate) fn repo(path: &str) -> RepoPath {
    RepoPath::try_from(PathBuf::from(path)).expect("BUG: absolute path")
}

pub(crate) fn workspace(path: &str, auto_fast_forward: bool) -> WorkspaceSettings {
    WorkspaceSettings {
        path: WorkspacePath::try_from(PathBuf::from(path)).expect("BUG: absolute path"),
        auto_fast_forward,
    }
}

/// Clean `main` that is `behind` commits behind `origin/main`.
pub(crate) fn behind(behind: u32) -> RepoStatus {
    RepoStatus {
        head: Head::Branch(BranchName::try_from("main".to_owned()).expect("BUG")),
        head_oid: Some(CommitId::try_from("1".repeat(40).as_str()).expect("BUG")),
        upstream: Some(Upstream {
            name: "origin/main".into(),
            oid: CommitId::try_from("2".repeat(40).as_str()).expect("BUG"),
        }),
        ahead: 0,
        behind,
        tracked_changes: 0,
        untracked: 0,
        operation: None,
    }
}

#[derive(Default)]
pub(crate) struct FakeGit {
    pub statuses: Mutex<HashMap<RepoPath, RepoStatus>>,
    pub failing_fetches: Vec<RepoPath>,
    pub failing_status: Vec<RepoPath>,
    pub state_changed: bool,
    pub fetched: Mutex<Vec<RepoPath>>,
    pub fast_forwarded: Mutex<Vec<RepoPath>>,
}

impl FakeGit {
    pub fn with(statuses: &[(&str, RepoStatus)]) -> Self {
        let map = statuses
            .iter()
            .map(|(path, status)| (repo(path), status.clone()))
            .collect();
        Self {
            statuses: Mutex::new(map),
            ..Self::default()
        }
    }
}

impl GitPort for FakeGit {
    async fn status(&self, repo: &RepoPath) -> Result<RepoStatus, GitError> {
        if self.failing_status.contains(repo) {
            return Err(GitError::Failed {
                action: "status",
                message: "broken".into(),
            });
        }
        self.statuses
            .lock()
            .expect("BUG")
            .get(repo)
            .cloned()
            .ok_or(GitError::Failed {
                action: "status",
                message: "unknown repo".into(),
            })
    }

    async fn fetch(&self, repo: &RepoPath) -> Result<(), GitError> {
        self.fetched.lock().expect("BUG").push(repo.clone());
        if self.failing_fetches.contains(repo) {
            Err(GitError::Failed {
                action: "fetch",
                message: "Permission denied (publickey)".into(),
            })
        } else {
            Ok(())
        }
    }

    async fn fast_forward(&self, permit: FastForwardPermit) -> Result<FfOutcome, GitError> {
        self.fast_forwarded
            .lock()
            .expect("BUG")
            .push(permit.repo().clone());
        if self.state_changed {
            return Ok(FfOutcome::StateChanged);
        }
        Ok(FfOutcome::Advanced {
            from: permit.head_oid().clone(),
            to: permit.target_oid().clone(),
            commits: permit.commits(),
        })
    }
}

pub(crate) struct FakeDiscovery {
    pub repos: HashMap<String, Vec<RepoPath>>,
}

impl RepoDiscovery for FakeDiscovery {
    async fn discover(
        &self,
        workspace: &WorkspacePath,
        _scan: &ScanSettings,
    ) -> Result<Vec<RepoPath>, DiscoveryError> {
        self.repos
            .get(&workspace.to_string())
            .cloned()
            .ok_or_else(|| DiscoveryError::Unavailable(workspace.to_string()))
    }
}

#[derive(Default)]
pub(crate) struct FakeLog {
    pub records: Mutex<Vec<ActionRecord>>,
}

impl ActionLog for FakeLog {
    async fn start(
        &self,
        at: Timestamp,
        kind: ActionKind,
        target: &str,
        trigger: Trigger,
    ) -> Result<ActionId, StoreError> {
        let mut records = self.records.lock().expect("BUG");
        let id = ActionId(i64::try_from(records.len()).expect("BUG") + 1);
        records.push(ActionRecord {
            id,
            started_at: at,
            finished_at: None,
            kind,
            target: target.to_owned(),
            trigger,
            outcome: None,
            summary: None,
        });
        Ok(id)
    }

    async fn finish(
        &self,
        id: ActionId,
        at: Timestamp,
        outcome: Outcome,
        summary: Option<String>,
    ) -> Result<(), StoreError> {
        let mut records = self.records.lock().expect("BUG");
        let record = records
            .iter_mut()
            .find(|r| r.id == id)
            .expect("BUG: started");
        record.finished_at = Some(at);
        record.outcome = Some(outcome);
        record.summary = summary;
        Ok(())
    }

    async fn recent(&self, _limit: u32) -> Result<Vec<ActionRecord>, StoreError> {
        Ok(self.records.lock().expect("BUG").clone())
    }
}

#[derive(Default)]
pub(crate) struct FakeCache {
    pub entries: Mutex<Vec<CachedRepo>>,
}

impl ScanCache for FakeCache {
    async fn save(&self, entry: CachedRepo) -> Result<(), StoreError> {
        let mut entries = self.entries.lock().expect("BUG");
        entries.retain(|e| e.repo != entry.repo);
        entries.push(entry);
        Ok(())
    }

    async fn load_all(&self) -> Result<Vec<CachedRepo>, StoreError> {
        Ok(self.entries.lock().expect("BUG").clone())
    }

    async fn retain(&self, repos: Vec<RepoPath>) -> Result<(), StoreError> {
        self.entries
            .lock()
            .expect("BUG")
            .retain(|e| repos.contains(&e.repo));
        Ok(())
    }
}

pub(crate) struct FakeClock;

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        "2026-10-17T12:00:00Z".parse().expect("BUG")
    }
}
